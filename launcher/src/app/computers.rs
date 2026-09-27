// SPDX-License-Identifier: AGPL-3.0-only
//! Computer selection, connection-option drafts and saved-computer editing.
use iced::Task;
use iced::widget::{Id, operation};

use crate::library::{Layout, Profile};

use super::{
    App, ComputerForm, ConnectionOptions, Dialog, FORM_NAME, FORM_USERNAME, Message, Page, Session, by_recent,
    looks_like_address,
};

impl App {
    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.library.get(id)
    }
    pub fn live_for(&self, profile_id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.profile_id == profile_id)
    }
    pub(super) fn profile_by_name(&self, v: &str) -> Option<&Profile> {
        let q = v.trim().to_lowercase();
        self.library
            .computers
            .iter()
            .find(|p| p.name.to_lowercase() == q || p.address.to_lowercase() == q)
    }
    /// A typed prefix that narrows the saved computers to exactly one counts as choosing it.
    fn sole_match(&self, value: &str) -> Option<&Profile> {
        let q = value.trim().to_lowercase();
        if q.is_empty() || looks_like_address(&q) {
            return None;
        }
        let mut hits = self
            .library
            .computers
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&q) || p.address.to_lowercase().contains(&q));
        let first = hits.next()?;
        hits.next().is_none().then_some(first)
    }
    /// The saved computer the Computer box currently means, if any.
    pub fn simple_match(&self) -> Option<&Profile> {
        let value = self.computer_box.trim();
        let chosen = self
            .chosen
            .as_deref()
            .and_then(|id| self.profile(id))
            .filter(|p| p.name == value || p.address == value);
        chosen
            .or_else(|| self.profile_by_name(value))
            .or_else(|| self.sole_match(value))
    }
    pub fn can_connect_simple(&self) -> bool {
        self.simple_match().is_some() || looks_like_address(self.computer_box.trim())
    }
    pub fn combo_items(&self) -> Vec<&Profile> {
        let q = self.combo_query.trim().to_lowercase();
        let mut items: Vec<&Profile> = self
            .library
            .computers
            .iter()
            .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q) || p.address.to_lowercase().contains(&q))
            .collect();
        items.sort_by(|a, b| by_recent(a, b));
        items
    }
    pub fn recent(&self) -> Vec<&Profile> {
        let mut items: Vec<&Profile> = self.library.computers.iter().collect();
        items.sort_by(|a, b| by_recent(a, b));
        items
    }
    /// Computers on the current page of the full layout: (open now, saved).
    pub fn library_rows(&self) -> (Vec<&Profile>, Vec<&Profile>) {
        let query = self.filter.trim().to_lowercase();
        let mut all: Vec<&Profile> = self
            .library
            .computers
            .iter()
            .filter(|p| {
                (self.page != Page::Favourites || p.favorite)
                    && format!("{} {} {}", p.name, p.address, p.username)
                        .to_lowercase()
                        .contains(&query)
            })
            .collect();
        all.sort_by(|a, b| by_recent(a, b));
        let (open, saved): (Vec<&Profile>, Vec<&Profile>) =
            all.into_iter().partition(|p| self.live_for(&p.id).is_some());
        (open, if self.page == Page::Open { Vec::new() } else { saved })
    }

    // ---------- simple layout ----------
    /// Keeps the simple window's choice consistent with the library after it changes.
    pub(super) fn resync(&mut self) {
        if self.chosen.as_deref().is_some_and(|id| self.profile(id).is_none()) {
            self.chosen = None;
        }
        self.sync_simple();
        if self.computer_box.is_empty()
            && let Some(p) = self.chosen.as_deref().and_then(|id| self.profile(id))
        {
            self.computer_box = p.name.clone();
        }
    }
    /// Follows the Computer box: a saved computer it names becomes the chosen one and brings its
    /// options; when it stops naming one, the options set for a new computer come back.
    pub(super) fn sync_simple(&mut self) {
        let matched = self
            .simple_match()
            .map(|p| (p.id.clone(), ConnectionOptions::from_profile(p)));
        match matched {
            Some((id, options)) if self.chosen.as_deref() != Some(id.as_str()) => {
                self.chosen = Some(id);
                self.options = options;
            }
            Some(_) => {}
            None => {
                if self.chosen.take().is_some() {
                    self.options = self.draft_options;
                }
            }
        }
    }
    pub(super) fn choose(&mut self, id: &str) {
        let Some(p) = self.profile(id) else { return };
        let (name, options) = (p.name.clone(), ConnectionOptions::from_profile(p));
        self.chosen = Some(id.to_owned());
        self.computer_box = name;
        self.options = options;
        self.combo_open = false;
        self.resync();
    }
    // ---------- computer dialog ----------
    pub(super) fn edit_computer(&mut self, id: Option<&str>, fresh: bool, address: &str) -> Task<Message> {
        let existing = id.and_then(|id| self.profile(id)).cloned();
        let mut form = ComputerForm {
            editing: existing.as_ref().map(|p| p.id.clone()),
            connect_after: fresh,
            title: if fresh {
                "Finish this computer"
            } else if existing.is_some() {
                "Edit computer"
            } else {
                "New computer"
            },
            name: String::new(),
            address: address.to_owned(),
            username: String::new(),
            options: ConnectionOptions::default(),
            favourite: false,
            error: None,
        };
        if let Some(p) = &existing {
            // Older quick connections stored the address as their generated display name.
            form.name = if fresh && p.name == p.address {
                String::new()
            } else {
                p.name.clone()
            };
            form.address = p.address.clone();
            form.username = p.username.clone();
            form.options = ConnectionOptions::from_profile(p);
            form.favourite = p.favorite;
        } else if fresh && self.layout == Layout::Simple {
            form.options = self.options;
        }
        self.combo_open = false;
        self.dialogs.push(Dialog::Computer(form));
        operation::focus(Id::new(if fresh { FORM_USERNAME } else { FORM_NAME }))
    }
    pub(super) fn save_computer(&mut self) -> Task<Message> {
        let Some(Dialog::Computer(form)) = self.dialogs.last() else {
            return Task::none();
        };
        let previous = form.editing.as_deref().and_then(|id| self.profile(id)).cloned();
        let address = form.address.trim().to_owned();
        let mut profile = previous.unwrap_or_else(Profile::draft);
        profile.name = match form.name.trim() {
            "" => address.chars().take(100).collect(),
            name => name.to_owned(),
        };
        profile.address = address;
        profile.username = form.username.trim().to_owned();
        profile.favorite = form.favourite;
        form.options.apply_to_profile(&mut profile);
        let connect_after = form.connect_after && !profile.username.is_empty();
        match self.backend.save_profile(profile) {
            Ok((library, saved)) => {
                self.library = library;
                self.dialogs.pop();
                // Editing the selected computer also refreshes its simple-layout options.
                // Leave another computer's unsaved selection and options alone.
                if self.layout == Layout::Simple || self.chosen.as_deref() == Some(saved.as_str()) {
                    self.choose(&saved);
                }
                self.resync();
                if connect_after {
                    return self.prompt_password(&saved, None);
                }
                Task::none()
            }
            Err(e) => {
                if let Some(Dialog::Computer(form)) = self.dialogs.last_mut() {
                    form.error = Some(e);
                }
                Task::none()
            }
        }
    }

    pub(super) fn with_form(&mut self, edit: impl FnOnce(&mut ComputerForm)) {
        if let Some(Dialog::Computer(form)) = self.dialogs.last_mut() {
            edit(form);
            form.error = None;
        }
    }
}
