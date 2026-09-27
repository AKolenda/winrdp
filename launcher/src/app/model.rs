// SPDX-License-Identifier: AGPL-3.0-only
//! UI state and typed messages shared by the launcher update loop and its views.
use iced::{keyboard, window};
use zeroize::Zeroizing;

use crate::backend::Transport;
use crate::host;
use crate::library::{Layout, Profile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Computers,
    Favourites,
    Open,
}

/// The per-connection options shared by the simple window's options and the computer dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionOptions {
    pub fullscreen: bool,
    pub clipboard: bool,
    pub audio: bool,
    pub microphone: bool,
    pub printer: bool,
}
impl Default for ConnectionOptions {
    /// The defaults for a computer that is not saved yet: clipboard and audio on.
    fn default() -> Self {
        Self {
            fullscreen: false,
            clipboard: true,
            audio: true,
            microphone: false,
            printer: false,
        }
    }
}
impl ConnectionOptions {
    pub fn from_profile(p: &Profile) -> Self {
        Self {
            fullscreen: p.fullscreen,
            clipboard: p.clipboard,
            audio: p.audio,
            microphone: p.microphone,
            printer: p.printer,
        }
    }
    pub(super) fn apply_to_profile(self, p: &mut Profile) {
        p.fullscreen = self.fullscreen;
        p.clipboard = self.clipboard;
        p.audio = self.audio;
        p.microphone = self.microphone;
        p.printer = self.printer;
    }

    pub(super) fn set(&mut self, option: ConnectionOption, enabled: bool) {
        match option {
            ConnectionOption::Fullscreen => self.fullscreen = enabled,
            ConnectionOption::Clipboard => self.clipboard = enabled,
            ConnectionOption::Audio => self.audio = enabled,
            ConnectionOption::Microphone => self.microphone = enabled,
            ConnectionOption::Printer => self.printer = enabled,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionOption {
    Fullscreen,
    Clipboard,
    Audio,
    Microphone,
    Printer,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionState {
    Connecting,
    Connected,
}
/// A desktop open in its own session window.
#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub profile_id: String,
    pub state: SessionState,
    pub transport: Option<Transport>,
    pub(super) stamped: bool,
}
impl Session {
    pub fn label(&self) -> &str {
        self.transport.as_ref().map_or("", |t| t.label.as_str())
    }
    pub fn udp(&self) -> bool {
        self.transport.as_ref().is_some_and(|t| t.transport == "udp")
    }
}

#[derive(Debug)]
pub struct ComputerForm {
    pub editing: Option<String>,
    pub connect_after: bool,
    pub title: &'static str,
    pub name: String,
    pub address: String,
    pub username: String,
    pub options: ConnectionOptions,
    pub favourite: bool,
    pub error: Option<String>,
}
/// A password on its way through the UI: zeroed on drop, and never printed by `Debug`.
#[derive(Clone, Default)]
pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
}
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[derive(Debug)]
pub struct PasswordForm {
    pub profile: String,
    pub title: String,
    pub account: String,
    pub error: Option<String>,
    pub password: Secret,
    pub fullscreen: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Confirmed {
    Disconnect(String),
    Remove(String),
    Quit,
}
#[derive(Debug)]
pub struct Confirm {
    pub title: String,
    pub text: String,
    pub yes: &'static str,
    pub action: Confirmed,
}
#[derive(Debug)]
pub enum Dialog {
    Computer(ComputerForm),
    Password(PasswordForm),
    Confirm(Confirm),
    Settings,
}

#[derive(Debug, Default)]
pub struct HostPanel {
    pub status: Option<host::Status>,
    pub mode: Option<host::Mode>,
    pub user: String,
    pub pass: Secret,
    pub busy: bool,
}
impl HostPanel {
    /// Sharing needs a name and password before it can be switched on.
    pub fn needs_credentials(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|s| s.available && !s.sharing && !s.credentials)
    }
}

pub struct Toast {
    pub text: String,
    pub(super) seq: u64,
}

#[derive(Clone, Debug)]
pub enum Message {
    WindowOpened(Option<window::Id>),
    Tick,
    Drag,
    Minimize,
    ToggleMaximize,
    Resize(window::Direction),
    CloseRequested,
    Key(keyboard::Key, keyboard::Modifiers),
    // simple layout
    BoxInput(String),
    BoxSubmit,
    ComboToggle,
    ComboPick(String),
    ComboDismiss,
    OptionsToggle,
    SetConnectionOption(ConnectionOption, bool),
    Choose(String),
    Activate(String),
    ConnectChosen,
    NewFromBox,
    EditChosen,
    // full layout
    ShowPage(Page),
    FilterInput(String),
    FilterSubmit,
    NewComputer,
    RowPressed(String),
    Disconnect(String),
    Edit(String),
    // dialogs
    FormName(String),
    FormAddress(String),
    FormUsername(String),
    FormConnectionOption(ConnectionOption, bool),
    FormFavourite(bool),
    FormSave,
    FormRemove,
    PasswordInput(Secret),
    PasswordFullscreen(bool),
    PasswordSubmit,
    ConfirmYes,
    CloseDialog,
    OpenSettings,
    SetLayout(Layout),
    SetDark(bool),
    // hosting
    HostRefresh,
    HostLoaded(host::Status),
    HostEnabled(Result<host::Status, String>),
    HostDisabled(Result<host::Status, String>),
    HostSwitch(bool),
    HostMode(host::Mode),
    HostUser(String),
    HostPass(Secret),
    HostSubmit,
    HostOpenSettings,
    Notify(String),
    DismissToast,
    ToastExpired(u64),
}
