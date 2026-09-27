// SPDX-License-Identifier: AGPL-3.0-only
//! Sign-in, session lifecycle and the status reported by desktop processes.
use iced::Task;
use iced::widget::{Id, operation};

use crate::backend::Ended;
use crate::library::now_iso;

use super::{
    App, Confirm, Confirmed, ConnectionOptions, Dialog, Message, PASSWORD, PasswordForm, Secret, Session, SessionState,
    looks_like_address,
};

impl App {
    pub fn password_dialog(&self) -> Option<&PasswordForm> {
        self.dialogs
            .iter()
            .find_map(|d| if let Dialog::Password(f) = d { Some(f) } else { None })
    }

    pub(super) fn connect_chosen(&mut self) -> Task<Message> {
        let value = self.computer_box.trim().to_owned();
        let Some(mut profile) = self.simple_match().cloned() else {
            if looks_like_address(&value) {
                return self.edit_computer(None, true, &value);
            }
            return Task::none();
        };
        // The options panel edits this connection; keep the profile in step.
        let profile_id = profile.id.clone();
        if ConnectionOptions::from_profile(&profile) != self.options {
            self.options.apply_to_profile(&mut profile);
            match self.backend.save_profile(profile) {
                Ok((library, _)) => self.library = library,
                // Sign-in uses the saved profile. Continuing here would silently restore
                // the old clipboard, audio or other options after the user changed them.
                Err(error) => return self.notify(error),
            }
        }
        self.prompt_password(&profile_id, None)
    }

    // ---------- connecting ----------
    /// `refused` is the sentence the computer gave for the previous attempt, shown
    /// above the box so a retyped password lands in a dialog that says what went wrong.
    pub(super) fn prompt_password(&mut self, id: &str, refused: Option<String>) -> Task<Message> {
        let Some(p) = self.profile(id).cloned() else {
            return Task::none();
        };
        if p.username.is_empty() {
            return self.edit_computer(Some(id), true, "");
        }
        // A refusal can land while the user is already typing into a dialog for another
        // computer. Re-pointing that dialog would send the password they are typing to
        // a different host, so report the refusal and leave the open dialog alone.
        if self.password_dialog().is_some() {
            return match refused {
                Some(r) => self.notify(format!("{}: {r}", p.name)),
                None => Task::none(),
            };
        }
        self.combo_open = false;
        self.dialogs.push(Dialog::Password(PasswordForm {
            profile: p.id.clone(),
            title: format!("Connect to {}", p.name),
            account: format!("{} at {}", p.username, p.address),
            error: refused,
            password: Secret::default(),
            fullscreen: p.fullscreen,
        }));
        operation::focus(Id::new(PASSWORD))
    }
    pub(super) fn submit_password(&mut self) -> Task<Message> {
        let Some(index) = self.dialogs.iter().rposition(|d| matches!(d, Dialog::Password(_))) else {
            return Task::none();
        };
        let Dialog::Password(form) = self.dialogs.remove(index) else {
            unreachable!()
        };
        let Some(p) = self.profile(&form.profile).cloned() else {
            return Task::none();
        };
        match self.backend.launch(&p, form.password.as_str(), form.fullscreen) {
            Ok(session) => {
                self.sessions.push(Session {
                    id: session,
                    name: p.name,
                    profile_id: p.id,
                    state: SessionState::Connecting,
                    transport: None,
                    stamped: false,
                });
                Task::none()
            }
            Err(e) => self.notify(e),
        }
        // `form.password` is zeroized as it drops here.
    }
    /// "Last opened" means the computer actually answered. Stamping it at launch put a
    /// refused sign-in at the top of the Recent list and claimed it had been opened.
    fn stamp_connected(&mut self, index: usize) -> Task<Message> {
        let session = &mut self.sessions[index];
        if session.stamped {
            return Task::none();
        }
        session.stamped = true;
        let profile_id = session.profile_id.clone();
        let Some(mut p) = self.profile(&profile_id).cloned() else {
            return Task::none();
        };
        p.last_connected = now_iso();
        match self.backend.save_profile(p) {
            Ok((library, _)) => {
                self.library = library;
                Task::none()
            }
            Err(e) => self.notify(e),
        }
    }
    /// A desktop window that closed by itself. The session publishes why it stopped;
    /// without this a mistyped password just closed the window and said nothing.
    fn report_end(&mut self, session: Option<Session>, ended: &Ended) -> Task<Message> {
        let name = session
            .as_ref()
            .map_or_else(|| "The desktop".to_owned(), |s| s.name.clone());
        let Some(failure) = ended.report.as_ref().filter(|r| r.state == "failed") else {
            // No word from the session at all: it stopped before it could report.
            if let Some(s) = &session
                && ended.report.is_none()
                && s.state != SessionState::Connected
            {
                return self.notify(format!("{name} closed before the desktop opened. See the session log."));
            }
            // It reported, but then stopped on a signal or a non-zero exit rather than a
            // clean close: say so instead of letting the window vanish without a word.
            if session.is_some() && ended.code != Some(0) {
                return self.notify(format!("{name} stopped unexpectedly. See the session log."));
            }
            return Task::none();
        };
        let retry = failure.reason == "credentials";
        let detail = if !retry && failure.reason != "account" && !failure.detail.is_empty() {
            format!("\n{}", failure.detail)
        } else {
            String::new()
        };
        let toast = self.notify(format!("{name}: {}{detail}", failure.message));
        match session {
            Some(s) if retry => Task::batch([
                toast,
                self.prompt_password(&s.profile_id, Some(failure.message.clone())),
            ]),
            _ => toast,
        }
    }
    pub(super) fn poll(&mut self) -> Task<Message> {
        let status = self.backend.poll();
        let mut tasks = Vec::new();
        for ended in &status.ended {
            let Some(index) = self.sessions.iter().position(|s| s.id == ended.session) else {
                continue;
            };
            let session = self.sessions.remove(index);
            tasks.push(self.report_end(Some(session), ended));
        }
        for live in &status.live {
            let Some(index) = self.sessions.iter().position(|s| s.id == live.session) else {
                continue;
            };
            let session = &mut self.sessions[index];
            if session.transport.as_ref().map(|t| &t.label) != live.transport.as_ref().map(|t| &t.label) {
                session.transport = live.transport.clone();
                if session.transport.is_some() {
                    session.state = SessionState::Connected;
                }
            }
            if session.transport.is_some() {
                tasks.push(self.stamp_connected(index));
            }
        }
        Task::batch(tasks)
    }
    pub(super) fn close_session(&mut self, id: &str) {
        if let Some(index) = self.sessions.iter().position(|s| s.id == id) {
            self.backend.kill(id);
            self.sessions.remove(index);
        }
    }
    /// The Windows-style "are you sure" before a live desktop is dropped.
    pub(super) fn ask_disconnect(&mut self, id: &str) {
        let Some(s) = self.sessions.iter().find(|s| s.id == id) else {
            return;
        };
        if s.state != SessionState::Connected {
            self.close_session(id);
            return;
        }
        self.dialogs.push(Dialog::Confirm(Confirm {
            title: format!("Disconnect from {}?", s.name),
            text: "The remote session stays signed in on the computer. You can connect again to pick up where you left off.".into(),
            yes: "Disconnect", action: Confirmed::Disconnect(id.to_owned()),
        }));
    }
}
