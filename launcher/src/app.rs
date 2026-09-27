// SPDX-License-Identifier: AGPL-3.0-only
//! Coordinates window messages, settings and application state.
//! Computer editing and selection live in `computers`; session lifecycle lives in
//! `connections`. The shared UI types are in `model`, and drawing is in `crate::view`.
mod computers;
mod connections;
mod model;

pub use model::{
    ComputerForm, Confirm, ConnectionOption, ConnectionOptions, Dialog, Message, Page, PasswordForm, Session,
    SessionState,
};
use model::{Confirmed, HostPanel, Secret, Toast};

use std::time::Duration;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{Id, operation};
use iced::{Size, Subscription, Task, event, window};

use crate::backend::Backend;
use crate::host;
use crate::library::{Layout, Library, Preferences, Profile, parse_iso};
use crate::style::{self, Tokens};

pub const COMPUTER_BOX: &str = "computer-box";
pub const FILTER: &str = "filter";
pub const PASSWORD: &str = "password";
pub const FORM_NAME: &str = "form-name";
pub const FORM_USERNAME: &str = "form-username";
pub const HOST_USER: &str = "host-user";

pub struct App {
    backend: Box<dyn Backend>,
    pub library: Library,
    pub version: &'static str,
    pub user: String,
    pub layout: Layout,
    pub page: Page,
    // simple layout
    pub computer_box: String,
    pub chosen: Option<String>,
    pub combo_open: bool,
    /// What the open dropdown is narrowed by: the typed text, or nothing when opened from its button.
    combo_query: String,
    pub options_open: bool,
    pub options: ConnectionOptions,
    /// The options set while the box names no saved computer, kept for the new one it will become.
    draft_options: ConnectionOptions,
    // full layout
    pub filter: String,
    pub sessions: Vec<Session>,
    /// Open dialogs, topmost last.
    pub dialogs: Vec<Dialog>,
    pub toast: Option<Toast>,
    toast_seq: u64,
    pub host: HostPanel,
    quitting: bool,
    pub window: Option<window::Id>,
}

/// Looks like a computer address rather than part of a saved name: `name.domain`, `a.b.c.d`, `host:port`.
pub fn looks_like_address(v: &str) -> bool {
    let (host, port) = v.rsplit_once(':').map_or((v, None), |(h, p)| (h, Some(p)));
    let host_ok = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        && !host.split('.').any(str::is_empty);
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()));
    host_ok && port_ok && (v.contains('.') || v.contains(':'))
}
/// Newest "last opened" first, then by name.
pub fn by_recent(a: &Profile, b: &Profile) -> std::cmp::Ordering {
    b.last_connected
        .cmp(&a.last_connected)
        .then_with(|| a.name.cmp(&b.name))
}
pub fn relative_date(iso: &str, now_ms: i64) -> String {
    let Some(timestamp) = parse_iso(iso) else {
        return "before".into();
    };
    let elapsed_days = now_ms.saturating_sub(timestamp).div_euclid(86_400_000);
    match elapsed_days {
        ..=0 => "today".into(),
        1 => "yesterday".into(),
        2..=29 => format!("{elapsed_days} days ago"),
        _ => iso.get(..10).unwrap_or(iso).into(),
    }
}

impl App {
    pub fn new(mut backend: Box<dyn Backend>) -> (Self, Option<String>) {
        let (library, error) = match backend.load() {
            Ok(l) => (l, None),
            Err(e) => (Library::default(), Some(e)),
        };
        let layout = library.preferences.layout;
        let mut app = Self {
            backend,
            library,
            version: env!("CARGO_PKG_VERSION"),
            user: std::env::var("USER").unwrap_or_default(),
            layout,
            page: Page::Computers,
            computer_box: String::new(),
            chosen: None,
            combo_open: false,
            combo_query: String::new(),
            options_open: false,
            options: ConnectionOptions::default(),
            draft_options: ConnectionOptions::default(),
            filter: String::new(),
            sessions: Vec::new(),
            dialogs: Vec::new(),
            toast: None,
            toast_seq: 0,
            host: HostPanel::default(),
            quitting: false,
            window: None,
        };
        app.resync();
        (app, error)
    }
    pub fn tokens(&self) -> Tokens {
        if self.library.preferences.dark {
            style::DARK
        } else {
            style::LIGHT
        }
    }
    pub fn title(&self) -> String {
        "Win RDP".into()
    }
    /// Shows a self-dismissing notice; longer when it carries detail lines.
    fn notify(&mut self, text: impl Into<String>) -> Task<Message> {
        let text = text.into();
        let wait = if text.contains('\n') { 12 } else { 6 };
        self.toast_seq += 1;
        let seq = self.toast_seq;
        self.toast = Some(Toast { text, seq });
        Task::perform(tokio::time::sleep(Duration::from_secs(wait)), move |()| {
            Message::ToastExpired(seq)
        })
    }
    fn confirm_close(&mut self) -> Task<Message> {
        if self.quitting
            || self.dialogs.iter().any(|d| {
                matches!(
                    d,
                    Dialog::Confirm(Confirm {
                        action: Confirmed::Quit,
                        ..
                    })
                )
            })
        {
            return Task::none();
        }
        match self.sessions.len() {
            0 => self.quit(),
            open => {
                let lead = if open == 1 {
                    "One desktop is".to_owned()
                } else {
                    format!("{open} desktops are")
                };
                self.dialogs.push(Dialog::Confirm(Confirm {
                    title: "Close Win RDP?".into(),
                    text: format!(
                        "{lead} still open. The remote sessions stay signed in; every connection from this app closes."
                    ),
                    yes: "Close and disconnect",
                    action: Confirmed::Quit,
                }));
                Task::none()
            }
        }
    }
    fn quit(&mut self) -> Task<Message> {
        self.quitting = true;
        self.backend.kill_all();
        iced::exit()
    }

    // ---------- settings ----------
    fn save_preferences(&mut self, edit: impl FnOnce(&mut Preferences)) -> Result<(), String> {
        let mut preferences = self.library.preferences.clone();
        edit(&mut preferences);
        self.library = self.backend.save_preferences(preferences)?;
        self.resync();
        Ok(())
    }
    /// Moves the launcher into the layout's window: the simple window is a small dialog, the
    /// full one a workspace. Each layout gets a new window rather than a resize: on Wayland,
    /// winit applies a requested size without reporting it, so iced keeps drawing the old size.
    pub fn apply_layout(&mut self, layout: Layout) -> Task<Message> {
        self.layout = layout;
        self.combo_open = false;
        self.resync();
        let Some(old) = self.window else { return Task::none() };
        let (_, opened) = window::open(window_settings(layout));
        // The old window closes once the new one is open; closing the last window exits the app.
        opened
            .map(|id| Message::WindowOpened(Some(id)))
            .chain(window::close(old))
    }
    fn refresh_host(&mut self) -> Task<Message> {
        self.host.busy = true;
        Task::perform(
            async { tokio::task::spawn_blocking(host::status).await.unwrap_or_default() },
            Message::HostLoaded,
        )
    }
    /// Turns sharing on, then reads the status back, so what the user is told is what happened.
    fn enable_host(&mut self, username: String) -> Task<Message> {
        let password = std::mem::take(&mut self.host.pass);
        let mode = self.host.mode.unwrap_or_default();
        self.host.busy = true;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    host::enable(&username, password.as_str(), mode).map(|()| host::status())
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            },
            Message::HostEnabled,
        )
    }
    fn host_loaded(&mut self, status: host::Status) {
        self.host.busy = false;
        if self.host.mode.is_none() {
            self.host.mode = Some(if status.extend {
                host::Mode::Extend
            } else {
                host::Mode::Mirror
            });
        }
        if self.host.user.is_empty() {
            self.host.user = self.user.clone();
        }
        self.host.status = Some(status);
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        // Anything the user does outside the Computer box closes its suggestions.
        if !matches!(
            message,
            Message::BoxInput(_)
                | Message::ComboToggle
                | Message::Key(..)
                | Message::Tick
                | Message::WindowOpened(_)
                | Message::ToastExpired(_)
                | Message::Notify(_)
                | Message::HostLoaded(_)
                | Message::HostEnabled(_)
                | Message::HostDisabled(_)
        ) {
            self.combo_open = false;
        }
        match message {
            Message::WindowOpened(id) => {
                self.window = id;
                Task::none()
            }
            Message::Tick => {
                if self.quitting {
                    Task::none()
                } else {
                    self.poll()
                }
            }
            Message::Drag => self.window.map_or_else(Task::none, window::drag),
            Message::Minimize => self.window.map_or_else(Task::none, |id| window::minimize(id, true)),
            Message::ToggleMaximize => match (self.window, self.layout) {
                (Some(id), Layout::Full) => window::toggle_maximize(id),
                _ => Task::none(),
            },
            Message::Resize(direction) => self
                .window
                .map_or_else(Task::none, |id| window::drag_resize(id, direction)),
            Message::CloseRequested => self.confirm_close(),
            Message::Key(key, modifiers) => self.key(key, modifiers),
            Message::BoxInput(value) => {
                self.combo_query = value.clone();
                self.computer_box = value;
                self.sync_simple();
                self.combo_open = true;
                Task::none()
            }
            Message::BoxSubmit => {
                self.combo_open = false;
                self.connect_chosen()
            }
            Message::ComboToggle => {
                self.combo_open = !self.combo_open;
                self.combo_query.clear();
                operation::focus(Id::new(COMPUTER_BOX))
            }
            Message::ComboPick(id) | Message::Choose(id) => {
                self.choose(&id);
                Task::none()
            }
            Message::ComboDismiss => {
                self.combo_open = false;
                Task::none()
            }
            Message::OptionsToggle => {
                self.options_open = !self.options_open;
                Task::none()
            }
            Message::SetConnectionOption(which, on) => {
                self.options.set(which, on);
                if self.chosen.is_none() {
                    self.draft_options = self.options;
                }
                Task::none()
            }
            Message::Activate(id) => {
                self.choose(&id);
                self.connect_chosen()
            }
            Message::ConnectChosen => self.connect_chosen(),
            Message::NewFromBox => {
                let value = self.computer_box.trim().to_owned();
                self.edit_computer(None, false, if looks_like_address(&value) { &value } else { "" })
            }
            Message::EditChosen => match self.chosen.clone() {
                Some(id) => self.edit_computer(Some(&id), false, ""),
                None => Task::none(),
            },
            Message::ShowPage(page) => {
                self.page = page;
                Task::none()
            }
            Message::FilterInput(value) => {
                self.filter = value;
                Task::none()
            }
            Message::FilterSubmit => {
                let value = self.filter.trim().to_owned();
                if let Some(id) = self.profile_by_name(&value).map(|p| p.id.clone()) {
                    return self.prompt_password(&id, None);
                }
                if looks_like_address(&value) {
                    self.edit_computer(None, true, &value)
                } else {
                    Task::none()
                }
            }
            Message::NewComputer => self.edit_computer(None, false, ""),
            Message::RowPressed(id) => {
                if self.live_for(&id).is_some() {
                    Task::none()
                } else {
                    self.prompt_password(&id, None)
                }
            }
            Message::Disconnect(session) => {
                self.ask_disconnect(&session);
                Task::none()
            }
            Message::Edit(id) => self.edit_computer(Some(&id), false, ""),
            Message::FormName(v) => {
                self.with_form(|f| f.name = v);
                Task::none()
            }
            Message::FormAddress(v) => {
                self.with_form(|f| f.address = v);
                Task::none()
            }
            Message::FormUsername(v) => {
                self.with_form(|f| f.username = v);
                Task::none()
            }
            Message::FormConnectionOption(which, on) => {
                self.with_form(|form| form.options.set(which, on));
                Task::none()
            }
            Message::FormFavourite(favourite) => {
                self.with_form(|form| form.favourite = favourite);
                Task::none()
            }
            Message::FormSave => self.save_computer(),
            Message::FormRemove => {
                let Some(Dialog::Computer(ComputerForm { editing: Some(id), .. })) = self.dialogs.pop() else {
                    return Task::none();
                };
                self.dialogs.push(Dialog::Confirm(Confirm {
                    title: "Remove this computer?".into(),
                    text: "Only the saved connection is removed. The remote computer is not changed.".into(),
                    yes: "Remove",
                    action: Confirmed::Remove(id),
                }));
                Task::none()
            }
            Message::PasswordInput(v) => {
                if let Some(Dialog::Password(f)) = self.dialogs.last_mut() {
                    f.password = v;
                }
                Task::none()
            }
            Message::PasswordFullscreen(on) => {
                if let Some(Dialog::Password(f)) = self.dialogs.last_mut() {
                    f.fullscreen = on;
                }
                Task::none()
            }
            Message::PasswordSubmit => {
                if matches!(self.dialogs.last(), Some(Dialog::Password(f)) if f.password.is_empty()) {
                    return Task::none();
                }
                self.submit_password()
            }
            Message::ConfirmYes => {
                let Some(Dialog::Confirm(confirm)) = self.dialogs.pop() else {
                    return Task::none();
                };
                match confirm.action {
                    Confirmed::Disconnect(id) => {
                        self.close_session(&id);
                        Task::none()
                    }
                    Confirmed::Remove(id) => {
                        let library = match self.backend.delete_profile(&id) {
                            Ok(library) => library,
                            Err(error) => return self.notify(error),
                        };
                        if self.chosen.as_deref() == Some(id.as_str()) {
                            self.chosen = None;
                            self.computer_box.clear();
                        }
                        self.library = library;
                        self.resync();
                        Task::none()
                    }
                    Confirmed::Quit => self.quit(),
                }
            }
            Message::CloseDialog => {
                self.dialogs.pop();
                Task::none()
            }
            Message::OpenSettings => {
                if !self.dialogs.iter().any(|d| matches!(d, Dialog::Settings)) {
                    self.dialogs.push(Dialog::Settings);
                }
                self.combo_open = false;
                self.refresh_host()
            }
            Message::SetLayout(layout) => match self.save_preferences(|p| p.layout = layout) {
                Ok(()) => self.apply_layout(layout),
                Err(error) => self.notify(error),
            },
            Message::SetDark(dark) => match self.save_preferences(|p| p.dark = dark) {
                Ok(()) => Task::none(),
                Err(error) => self.notify(error),
            },
            Message::HostRefresh => self.refresh_host(),
            Message::HostLoaded(status) => {
                self.host_loaded(status);
                Task::none()
            }
            Message::HostEnabled(Ok(status)) => {
                let on = status.sharing;
                self.host_loaded(status);
                self.notify(if on {
                    "Sharing is on. Windows can connect to this desktop."
                } else {
                    "Sharing did not start. Open GNOME\u{2019}s Remote Desktop settings to check."
                })
            }
            Message::HostDisabled(Ok(status)) => {
                self.host_loaded(status);
                self.notify("Sharing is off.")
            }
            Message::HostEnabled(Err(e)) | Message::HostDisabled(Err(e)) => {
                self.host.busy = false;
                Task::batch([self.notify(e), self.refresh_host()])
            }
            Message::HostSwitch(on) => {
                let Some(status) = self.host.status.clone().filter(|s| s.available) else {
                    return Task::none();
                };
                if !on {
                    if !status.sharing {
                        return Task::none();
                    }
                    self.host.busy = true;
                    return Task::perform(
                        async {
                            tokio::task::spawn_blocking(|| host::disable().map(|()| host::status()))
                                .await
                                .unwrap_or_else(|e| Err(e.to_string()))
                        },
                        Message::HostDisabled,
                    );
                }
                if status.sharing {
                    return Task::none();
                }
                if !status.credentials {
                    return operation::focus(Id::new(HOST_USER));
                }
                self.enable_host(String::new())
            }
            Message::HostMode(mode) => {
                self.host.mode = Some(mode);
                let status = self.host.status.clone().unwrap_or_default();
                if status.sharing && status.credentials {
                    self.enable_host(String::new())
                } else {
                    Task::none()
                }
            }
            Message::HostUser(v) => {
                self.host.user = v;
                Task::none()
            }
            Message::HostPass(v) => {
                self.host.pass = v;
                Task::none()
            }
            Message::HostSubmit => {
                let user = self.host.user.trim().to_owned();
                self.enable_host(user)
            }
            Message::HostOpenSettings => match host::open_settings() {
                Ok(()) => Task::none(),
                Err(e) => self.notify(e),
            },
            Message::Notify(text) => self.notify(text),
            Message::DismissToast => {
                self.toast = None;
                Task::none()
            }
            Message::ToastExpired(seq) => {
                if self.toast.as_ref().is_some_and(|t| t.seq == seq) {
                    self.toast = None;
                }
                Task::none()
            }
        }
    }
    fn key(&mut self, key: Key, modifiers: keyboard::Modifiers) -> Task<Message> {
        match key.as_ref() {
            Key::Named(Named::Escape) => {
                if self.combo_open {
                    self.combo_open = false;
                } else {
                    self.dialogs.pop();
                }
                Task::none()
            }
            Key::Named(Named::Tab) => {
                if modifiers.shift() {
                    operation::focus_previous()
                } else {
                    operation::focus_next()
                }
            }
            Key::Character("n") if modifiers.command() && self.dialogs.is_empty() => {
                self.edit_computer(None, false, "")
            }
            Key::Character(",") if modifiers.command() && self.dialogs.is_empty() => self.update(Message::OpenSettings),
            _ => Task::none(),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick),
            window::close_requests().map(|_| Message::CloseRequested),
            event::listen_with(|event, status, _window| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    Some(Message::Key(key, modifiers))
                }
                // A click on nothing in particular closes the Computer box's suggestions.
                iced::Event::Mouse(iced::mouse::Event::ButtonPressed(_)) if status == event::Status::Ignored => {
                    Some(Message::ComboDismiss)
                }
                _ => None,
            }),
        ])
    }
}

/// (minimum, initial) window size for a layout.
pub fn window_size(layout: Layout) -> (Size, Size) {
    match layout {
        Layout::Simple => (Size::new(460.0, 420.0), Size::new(480.0, 600.0)),
        Layout::Full => (Size::new(780.0, 540.0), Size::new(1240.0, 820.0)),
    }
}

/// The launcher window for a layout, opened at startup and again on every layout change.
pub fn window_settings(layout: Layout) -> window::Settings {
    let (min, size) = window_size(layout);
    window::Settings {
        size,
        min_size: Some(min),
        position: window::Position::Centered,
        resizable: layout == Layout::Full,
        decorations: !crate::view::undecorated(),
        exit_on_close_request: false,
        icon: window_icon(),
        // Matches StartupWMClass in io.winrdp.Next.desktop, so the dock shows the app icon.
        platform_specific: window::settings::PlatformSpecific {
            application_id: "winrdp-next".into(),
            ..Default::default()
        },
        ..window::Settings::default()
    }
}

/// The app icon, for window managers that read it from the window (X11 `_NET_WM_ICON`).
fn window_icon() -> Option<window::Icon> {
    let decoder = png::Decoder::new(std::io::Cursor::new(
        include_bytes!("../../packaging/icons/64.png").as_slice(),
    ));
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut rgba).ok()?;
    rgba.truncate(frame.buffer_size());
    window::icon::from_rgba(rgba, frame.width, frame.height).ok()
}
