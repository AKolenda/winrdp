// SPDX-License-Identifier: AGPL-3.0-only
//! The launcher's behaviour, checked through the window it draws. Each test drives the
//! real view in iced's headless simulator (clicks, typing, Enter), hands the messages
//! that produces to [`App::update`] as the runtime would, and reads the outcome back
//! from the redrawn view. What a widget does not report to the simulator (check marks,
//! radio buttons, switches) is read from the app's state instead.
//!
//! The backend is [`Demo`]: computers live in memory and sessions are records whose
//! events each test reports itself, so nothing touches the disk, the network or GNOME.
//!
//! `cargo test -p winrdp-next -- --ignored screenshots` redraws the published screenshots.
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::LazyLock;

use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::theme::Base as _;
use iced::widget::Id;
use iced::{Event, Point, Rectangle, Settings, Size, mouse, window};
use iced_test::Simulator;
use iced_test::selector::{Candidate, Target};
use iced_test::simulator::{click, press_key, release_key};

use crate::app::{
    App, COMPUTER_BOX, ComputerForm, Dialog, FILTER, FORM_NAME, FORM_USERNAME, Message, PASSWORD, looks_like_address,
    relative_date, window_size,
};
use crate::backend::{Backend, Demo, Ended, Live, Report, Status, Transport};
use crate::library::{Layout, Library, Preferences, Profile, format_iso, parse_iso};
use crate::{style, view};

const REFUSED: &str = "The user name or password is incorrect.";

/// `App::update` sets its toast timers up with tokio, which the iced runtime provides
/// around every update in the real window.
static TOKIO: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("a tokio runtime")
});

/// Failures a test can inject without changing the in-memory library.
#[derive(Default)]
struct BackendErrors {
    save_profile: Option<String>,
    delete_profile: Option<String>,
    save_preferences: Option<String>,
}

/// The in-memory backend, shared so a test can read what the app saved, launched and
/// killed, and queue what the session processes report next.
struct Shared {
    demo: Rc<RefCell<Demo>>,
    errors: Rc<RefCell<BackendErrors>>,
}
impl Backend for Shared {
    fn load(&mut self) -> Result<Library, String> {
        self.demo.borrow_mut().load()
    }
    fn save_profile(&mut self, profile: Profile) -> Result<(Library, String), String> {
        if let Some(error) = &self.errors.borrow().save_profile {
            return Err(error.clone());
        }
        self.demo.borrow_mut().save_profile(profile)
    }
    fn delete_profile(&mut self, id: &str) -> Result<Library, String> {
        if let Some(error) = &self.errors.borrow().delete_profile {
            return Err(error.clone());
        }
        self.demo.borrow_mut().delete_profile(id)
    }
    fn save_preferences(&mut self, preferences: Preferences) -> Result<Library, String> {
        if let Some(error) = &self.errors.borrow().save_preferences {
            return Err(error.clone());
        }
        self.demo.borrow_mut().save_preferences(preferences)
    }
    fn launch(&mut self, profile: &Profile, password: &str, fullscreen: bool) -> Result<String, String> {
        self.demo.borrow_mut().launch(profile, password, fullscreen)
    }
    fn poll(&mut self) -> Status {
        self.demo.borrow_mut().poll()
    }
    fn kill(&mut self, session: &str) {
        self.demo.borrow_mut().kill(session);
    }
    fn kill_all(&mut self) {
        self.demo.borrow_mut().kill_all();
    }
}

/// The launcher window over a [`Demo`] backend, at the size the app gives its layout.
struct Launcher {
    app: App,
    demo: Rc<RefCell<Demo>>,
    backend_errors: Rc<RefCell<BackendErrors>>,
    _tokio: tokio::runtime::EnterGuard<'static>,
}

impl Launcher {
    fn open(mut demo: Demo, layout: Layout) -> Self {
        let tokio = TOKIO.enter();
        demo.library.preferences.layout = layout;
        let demo = Rc::new(RefCell::new(demo));
        let backend_errors = Rc::new(RefCell::new(BackendErrors::default()));
        let (app, error) = App::new(Box::new(Shared {
            demo: Rc::clone(&demo),
            errors: Rc::clone(&backend_errors),
        }));
        assert_eq!(error, None);
        Self {
            app,
            demo,
            backend_errors,
            _tokio: tokio,
        }
    }
    /// The simple window with the two computers the web launcher's checks used.
    fn simple() -> Self {
        Self::open(two_computers(), Layout::Simple)
    }
    /// The full workspace with the same two computers.
    fn full() -> Self {
        Self::open(two_computers(), Layout::Full)
    }

    // ---------- driving the window ----------

    /// The current view in a simulator, drawn with the window's font settings (`main.rs`).
    fn ui(&self) -> Simulator<'_, Message> {
        let settings = Settings {
            default_font: style::ui_font(),
            default_text_size: 13.into(),
            ..Settings::default()
        };
        Simulator::with_size(settings, window_size(self.app.layout).1, view::view(&self.app))
    }
    /// Runs one interaction against the current view, then applies the messages it
    /// produced. The tasks `update` returns (focus moves, timers, window changes) are not
    /// run, and focus does not survive into the next view: fields are clicked before typing.
    fn act(&mut self, interact: impl FnOnce(&mut Simulator<'_, Message>)) -> Vec<Message> {
        let messages: Vec<Message> = {
            let mut ui = self.ui();
            interact(&mut ui);
            ui.into_messages().collect()
        };
        for message in &messages {
            let _ = self.app.update(message.clone());
        }
        messages
    }
    /// Clicks the topmost widget that reads `text`: a dialog's button rather than the
    /// window's button of the same name beneath it.
    fn click(&mut self, text: &str) -> Vec<Message> {
        self.click_at(text, |bounds| bounds.center())
    }
    /// Clicks relative to the topmost text reading `text`.
    fn click_at(&mut self, text: &str, spot: impl FnOnce(Rectangle) -> Point) -> Vec<Message> {
        self.act(|ui| {
            let bounds = topmost(ui, text).unwrap_or_else(|| panic!("nothing on screen reads {text:?}"));
            press(ui, spot(bounds));
        })
    }
    /// Settings' radio buttons draw no text of their own; each sits left of its choice's title.
    fn click_choice(&mut self, title: &str) {
        let messages = self.click_at(title, |title| Point::new(title.x - 20.0, title.y + 8.0));
        assert!(!messages.is_empty(), "no radio button left of {title:?}");
    }
    /// Settings' switches draw no text of their own; each sits right of its label.
    fn click_switch(&mut self, label: &str) {
        let messages = self.click_at(label, |label| {
            Point::new(label.x + label.width + 10.0, label.center_y())
        });
        assert!(!messages.is_empty(), "no switch right of {label:?}");
    }
    /// The close button in the top-right corner of the window's own title bar.
    fn close_window(&mut self) {
        let width = window_size(self.app.layout).1.width;
        let messages = self.act(|ui| press(ui, Point::new(width - 22.0, 20.0)));
        assert!(
            messages.iter().any(|m| matches!(m, Message::CloseRequested)),
            "the title bar's close button is expected in the top-right corner"
        );
    }
    /// Replaces the text of a field, as selecting all and typing does.
    fn type_into(&mut self, field: &'static str, text: &str) {
        self.dismiss_suggestions();
        let _ = self.act(|ui| {
            focus(ui, field);
            replace(ui, text);
        });
    }
    /// Types into a field and presses Enter there.
    fn enter(&mut self, field: &'static str, text: &str) {
        self.dismiss_suggestions();
        let _ = self.act(|ui| {
            focus(ui, field);
            replace(ui, text);
            let _ = ui.tap_key(Named::Enter);
        });
    }
    /// The Computer box's suggestions lie over the Recent list; the simulator does not run
    /// the subscription that closes them on a stray click, so Escape closes them first.
    fn dismiss_suggestions(&mut self) {
        if self.app.combo_open {
            self.press(Named::Escape);
        }
    }
    /// A key pressed anywhere in the window. The app hears keys through a subscription,
    /// which the simulator does not run, so the key goes to `update` directly.
    fn press(&mut self, key: Named) {
        let _ = self.app.update(Message::Key(Key::Named(key), Modifiers::empty()));
    }
    /// Ctrl+, opens Settings from either layout.
    fn open_settings(&mut self) {
        let _ = self
            .app
            .update(Message::Key(Key::Character(",".into()), Modifiers::COMMAND));
        assert!(self.shows("Settings"));
    }
    fn change_layout(&mut self, layout: Layout) {
        self.open_settings();
        self.click_choice(match layout {
            Layout::Simple => "Simple",
            Layout::Full => "Full",
        });
        let _ = self.click("Close");
        assert_eq!(self.app.layout, layout);
    }
    /// Filters the workspace to one computer, then opens its edit button.
    fn edit_in_full(&mut self, name: &str) {
        assert_eq!(self.app.layout, Layout::Full);
        self.type_into(FILTER, name);
        let _ = self.click("\u{2026}");
        assert_eq!(self.computer_form().name, name);
    }
    /// Opens a saved computer the way a user does and signs in; returns the new session.
    fn connect(&mut self, name: &str, password: &str) -> String {
        // In the simple window a click picks the computer and Connect opens it; in the
        // full workspace a click on its row does both.
        let _ = self.click(name);
        if self.app.layout == Layout::Simple {
            let _ = self.click("Connect");
        }
        assert!(self.shows(&format!("Connect to {name}")));
        self.type_into(PASSWORD, password);
        let _ = self.click("Connect");
        self.app.sessions.last().expect("a session opened").id.clone()
    }
    /// Types a new address into the Computer box, presses Enter, gives the account and saves.
    fn add_by_address(&mut self, address: &str, username: &str) {
        self.enter(COMPUTER_BOX, address);
        self.type_into(FORM_USERNAME, username);
        let _ = self.click("Save");
    }

    // ---------- session events ----------

    /// What the session processes report at the next poll of the window's timer.
    fn report(&mut self, status: Status) {
        self.demo.borrow_mut().pending = status;
        let _ = self.app.update(Message::Tick);
    }
    /// The computer answered, over UDP v2.
    fn answer(&mut self, session: &str) {
        self.report(Status {
            live: vec![Live {
                session: session.into(),
                transport: Some(udp_v2()),
            }],
            ..Status::default()
        });
    }
    /// The computer refused the sign-in and the session exited saying so.
    fn refuse(&mut self, session: &str) {
        self.report(Status {
            ended: vec![refusal(session)],
            ..Status::default()
        });
    }

    // ---------- reading the window ----------

    fn shows(&self, text: &str) -> bool {
        texts(&mut self.ui()).iter().any(|(content, _)| content == text)
    }
    /// What a text field shows: its value, or its placeholder while it is empty.
    fn field(&self, id: &'static str) -> String {
        match self.ui().find(Id::new(id)) {
            Ok(Target::TextInput { content, .. }) => content,
            other => panic!("no text field {id}: {other:?}"),
        }
    }
    /// The computer dialog's Address field as shown. It has no id of its own; it is the
    /// field after Name.
    fn address_field(&self) -> String {
        let fields = fields(&mut self.ui());
        let name = fields
            .iter()
            .position(|(id, _)| *id == Some(Id::new(FORM_NAME)))
            .expect("the computer dialog is open");
        fields[name + 1].1.clone()
    }
    fn computer_form(&self) -> &ComputerForm {
        self.app
            .dialogs
            .iter()
            .rev()
            .find_map(|d| {
                if let Dialog::Computer(form) = d {
                    Some(form)
                } else {
                    None
                }
            })
            .expect("a computer dialog is open")
    }
    fn toast(&self) -> Option<&str> {
        self.app.toast.as_ref().map(|t| t.text.as_str())
    }

    // ---------- reading the backend ----------

    /// A computer as the backend saved it.
    fn saved(&self, name: &str) -> Profile {
        let demo = self.demo.borrow();
        demo.library
            .computers
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .unwrap_or_else(|| panic!("{name} is not saved"))
    }
    fn saved_names(&self) -> Vec<String> {
        self.demo
            .borrow()
            .library
            .computers
            .iter()
            .map(|p| p.name.clone())
            .collect()
    }
    fn killed(&self) -> Vec<String> {
        self.demo.borrow().killed.clone()
    }
}

/// Every text the view draws, with where it is visible, bottom layer first: a dialog's
/// texts come after those of the window beneath it.
fn texts(ui: &mut Simulator<'_, Message>) -> Vec<(String, Option<Rectangle>)> {
    let mut texts = Vec::new();
    let _ = ui.find(|candidate: Candidate<'_>| -> Option<()> {
        if let Candidate::Text {
            content,
            visible_bounds,
            ..
        } = candidate
        {
            texts.push((content.to_owned(), visible_bounds));
        }
        None
    });
    texts
}
fn topmost(ui: &mut Simulator<'_, Message>, text: &str) -> Option<Rectangle> {
    texts(ui)
        .into_iter()
        .rev()
        .find_map(|(content, bounds)| if content == text { bounds } else { None })
}
/// Every text field with its id and what it shows, in drawing order.
fn fields(ui: &mut Simulator<'_, Message>) -> Vec<(Option<Id>, String)> {
    let mut fields = Vec::new();
    let _ = ui.find(|candidate: Candidate<'_>| -> Option<()> {
        if let Candidate::TextInput { id, state, .. } = candidate {
            fields.push((id.cloned(), state.text().to_owned()));
        }
        None
    });
    fields
}
fn press(ui: &mut Simulator<'_, Message>, at: Point) {
    ui.point_at(at);
    let _ = ui.simulate(click());
}
fn focus(ui: &mut Simulator<'_, Message>, field: &'static str) {
    if let Err(error) = ui.click(Id::new(field)) {
        panic!("cannot click the {field} field: {error}");
    }
}
/// Ctrl+A, then the new text (or Backspace to leave the field empty).
fn replace(ui: &mut Simulator<'_, Message>, text: &str) {
    let _ = ui.simulate([
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::COMMAND)),
        press_key(Key::Character("a".into()), None),
        release_key(Key::Character("a".into())),
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::empty())),
    ]);
    let _ = if text.is_empty() {
        ui.tap_key(Named::Backspace)
    } else {
        ui.typewrite(text)
    };
}

fn computer(name: &str, address: &str, username: &str) -> Profile {
    Profile {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        address: address.into(),
        username: username.into(),
        ..Profile::draft()
    }
}
fn two_computers() -> Demo {
    Demo::with(Library {
        computers: vec![
            computer("Office PC", "192.0.2.10", "alex"),
            computer("Lab", "192.0.2.20", "alex"),
        ],
        ..Library::default()
    })
}
fn udp_v2() -> Transport {
    Transport {
        transport: "udp".into(),
        udp_version: Some(2),
        label: "UDP v2".into(),
    }
}
fn refusal(session: &str) -> Ended {
    Ended {
        session: session.into(),
        code: Some(76),
        report: Some(Report {
            state: "failed".into(),
            reason: "credentials".into(),
            message: REFUSED.into(),
            detail: "CredSSP: LOGON_FAILURE".into(),
        }),
    }
}

// ---------- the simple window ----------

#[test]
fn startup_does_not_touch_inbound_sharing() {
    let launcher = Launcher::simple();
    assert_eq!(
        launcher.app.host.status, None,
        "no GNOME Remote Desktop command runs before Settings opens"
    );
    assert!(!launcher.app.host.busy);
}

#[test]
fn the_empty_computer_box_shows_an_ip_address_example() {
    let launcher = Launcher::simple();
    assert!(launcher.app.computer_box.is_empty());
    assert_eq!(launcher.field(COMPUTER_BOX), "10.0.0.7");
}

#[test]
fn a_partial_ip_is_a_new_address_not_the_saved_computer_it_begins() {
    let mut launcher = Launcher::simple();
    launcher.type_into(COMPUTER_BOX, "192.0.2.1");
    assert!(launcher.shows("New computer at 192.0.2.1. Enter asks for the account."));
    assert_eq!(
        launcher.app.chosen, None,
        "192.0.2.1 must not pick the saved 192.0.2.10"
    );
}

#[test]
fn enter_on_a_new_address_opens_the_computer_dialog_with_a_blank_name_and_that_address() {
    let mut launcher = Launcher::simple();
    launcher.enter(COMPUTER_BOX, "192.0.2.1");
    assert!(launcher.shows("Finish this computer"));
    assert_eq!(launcher.computer_form().name, "");
    assert_eq!(launcher.address_field(), "192.0.2.1");
    assert_eq!(
        launcher.saved_names(),
        ["Office PC", "Lab"],
        "nothing is saved before Save"
    );
}

#[test]
fn the_name_is_optional_and_defaults_to_the_address() {
    let mut launcher = Launcher::simple();
    launcher.enter(COMPUTER_BOX, "192.0.2.30");
    assert!(launcher.shows("Name (optional)"));
    launcher.type_into(FORM_USERNAME, "alex");
    let _ = launcher.click("Save");
    let saved = launcher
        .demo
        .borrow()
        .library
        .computers
        .last()
        .cloned()
        .expect("a saved computer");
    assert_eq!(
        (saved.name.as_str(), saved.address.as_str()),
        ("192.0.2.30", "192.0.2.30")
    );
}

#[test]
fn cancelling_the_computer_dialog_saves_nothing() {
    let mut launcher = Launcher::simple();
    launcher.enter(COMPUTER_BOX, "192.0.2.1");
    let _ = launcher.click("Cancel");
    assert!(launcher.app.dialogs.is_empty());
    assert_eq!(launcher.saved_names(), ["Office PC", "Lab"]);
    assert!(!launcher.shows("192.0.2.1"), "the Recent list gained nothing");
}

#[test]
fn unticked_options_carry_into_the_new_computer_and_are_saved() {
    let mut launcher = Launcher::simple();
    let _ = launcher.click("Show options");
    let _ = launcher.click("Share the clipboard");
    let _ = launcher.click("Play remote audio here");
    launcher.enter(COMPUTER_BOX, "192.0.2.30");
    let draft = launcher.computer_form().options;
    assert!(
        !draft.clipboard && !draft.audio,
        "the dialog starts from the window's options"
    );
    launcher.type_into(FORM_USERNAME, "alex");
    let _ = launcher.click("Save");
    let saved = launcher.saved("192.0.2.30");
    assert!(!saved.clipboard && !saved.audio);
}

#[test]
fn failing_to_save_connection_options_stops_sign_in_until_the_save_succeeds() {
    let mut launcher = Launcher::simple();
    let _ = launcher.click("Office PC");
    let _ = launcher.click("Show options");
    let _ = launcher.click("Share the clipboard");
    let _ = launcher.click("Play remote audio here");
    launcher.backend_errors.borrow_mut().save_profile = Some("The library could not be saved.".into());

    let _ = launcher.click("Connect");

    assert!(launcher.shows("The library could not be saved."));
    assert!(
        launcher.app.password_dialog().is_none(),
        "a failed save must not continue with the old clipboard and audio settings"
    );
    assert!(launcher.demo.borrow().launches.is_empty());
    assert!(launcher.saved("Office PC").clipboard);
    assert!(!launcher.app.options.clipboard && !launcher.app.options.audio);

    launcher.backend_errors.borrow_mut().save_profile = None;
    let _ = launcher.click("Connect");

    assert!(launcher.shows("Connect to Office PC"));
    let saved = launcher.saved("Office PC");
    assert!(!saved.clipboard && !saved.audio);
    launcher.type_into(PASSWORD, "dummy-test-only");
    let _ = launcher.click("Connect");
    assert_eq!(launcher.demo.borrow().launches.len(), 1);
}

#[test]
fn options_for_a_new_computer_survive_typing_past_a_saved_one() {
    let demo = Demo::with(Library {
        computers: vec![computer("Office PC", "192.0.2.10", "alex")],
        ..Library::default()
    });
    let mut launcher = Launcher::open(demo, Layout::Simple);
    let _ = launcher.click("Show options");
    let _ = launcher.click("Share the clipboard");
    // Typed one key at a time: "1" alone names the saved computer and brings its options.
    launcher.enter(COMPUTER_BOX, "192.0.2.30");
    assert!(
        !launcher.computer_form().options.clipboard,
        "the unticked clipboard comes back once the box stops naming Office PC"
    );
}

#[test]
fn one_click_after_typing_reaches_the_button_it_is_on() {
    let mut launcher = Launcher::simple();
    launcher.type_into(COMPUTER_BOX, "Office");
    assert!(launcher.app.combo_open, "typing opens the suggestions");
    let _ = launcher.click("Connect");
    assert!(launcher.shows("Connect to Office PC"));
    assert!(!launcher.app.combo_open);
}

#[test]
fn finishing_a_computer_continues_to_its_password_dialog() {
    let mut launcher = Launcher::simple();
    launcher.add_by_address("192.0.2.30", "alex");
    assert!(launcher.shows("Connect to 192.0.2.30"));
    assert!(launcher.shows("alex at 192.0.2.30"));
}

#[test]
fn connecting_launches_that_computer_and_drops_the_password() {
    let mut launcher = Launcher::simple();
    launcher.add_by_address("192.0.2.30", "alex");
    launcher.type_into(PASSWORD, "dummy-test-only");
    let _ = launcher.click("Connect");
    let id = launcher.saved("192.0.2.30").id;
    assert_eq!(
        launcher.demo.borrow().launches,
        [(id, "demo-session-1".to_owned(), false)]
    );
    assert!(
        launcher.app.password_dialog().is_none(),
        "the password went with its dialog"
    );
    assert!(!launcher.shows("Connect to 192.0.2.30"));
}

#[test]
fn launching_alone_does_not_stamp_last_opened() {
    let mut launcher = Launcher::simple();
    let _ = launcher.connect("Office PC", "dummy-test-only");
    // Still connecting: the session is running but has published no transport.
    launcher.report(Status::default());
    assert_eq!(launcher.saved("Office PC").last_connected, "");
}

#[test]
fn a_connected_session_stamps_last_opened_and_shows_its_transport() {
    let mut launcher = Launcher::simple();
    let session = launcher.connect("Office PC", "dummy-test-only");
    launcher.answer(&session);
    assert!(parse_iso(&launcher.saved("Office PC").last_connected).is_some());
    assert!(launcher.shows("UDP v2"));
}

#[test]
fn a_refused_sign_in_reopens_the_password_dialog_saying_why_with_an_empty_field() {
    let mut launcher = Launcher::simple();
    let session = launcher.connect("Office PC", "wrong password");
    launcher.refuse(&session);
    assert!(launcher.shows("Connect to Office PC"));
    assert!(launcher.shows(REFUSED));
    assert_eq!(launcher.field(PASSWORD), "");
    assert!(launcher.app.sessions.is_empty());
}

#[test]
fn a_refusal_for_another_session_does_not_repoint_the_open_password_dialog() {
    let mut launcher = Launcher::simple();
    let office = launcher.connect("Office PC", "wrong password");
    let lab = launcher.connect("Lab", "wrong password");
    launcher.refuse(&lab);
    launcher.refuse(&office);
    // The password being typed must still go to Lab; Office PC's refusal is only reported.
    assert!(launcher.shows("Connect to Lab"));
    assert!(!launcher.shows("Connect to Office PC"));
    assert_eq!(
        launcher.app.password_dialog().map(|f| f.profile.clone()),
        Some(launcher.saved("Lab").id)
    );
    assert_eq!(
        launcher.toast(),
        Some("Office PC: The user name or password is incorrect.")
    );
}

#[test]
fn typing_a_saved_name_restores_its_options() {
    let mut launcher = Launcher::simple();
    let _ = launcher.click("Show options");
    let _ = launcher.click("Share the clipboard");
    assert!(!launcher.app.options.clipboard);
    launcher.type_into(COMPUTER_BOX, "Office PC");
    assert!(launcher.app.options.clipboard, "Office PC shares the clipboard");
    assert!(launcher.shows("Signing in as "));
}

#[test]
fn editing_the_selected_computer_in_full_updates_the_next_simple_connection() {
    let mut launcher = Launcher::simple();
    let _ = launcher.click("Office PC");
    launcher.change_layout(Layout::Full);
    launcher.edit_in_full("Office PC");
    let _ = launcher.click("Share the clipboard");
    let _ = launcher.click("Save");
    assert!(!launcher.saved("Office PC").clipboard);

    launcher.change_layout(Layout::Simple);
    let _ = launcher.click("Connect");

    assert!(launcher.shows("Connect to Office PC"));
    assert!(
        !launcher.saved("Office PC").clipboard,
        "connecting must keep the options saved in the full workspace"
    );
    launcher.type_into(PASSWORD, "dummy-test-only");
    let _ = launcher.click("Connect");
    assert_eq!(launcher.demo.borrow().launches.len(), 1);
}

#[test]
fn editing_another_computer_in_full_preserves_simple_connection_drafts() {
    // Both an existing computer's unsaved options and a new computer's draft survive.
    for computer_box in ["Lab", "192.0.2.30"] {
        let mut launcher = Launcher::simple();
        launcher.type_into(COMPUTER_BOX, computer_box);
        let _ = launcher.click("Show options");
        let _ = launcher.click("Share the clipboard");
        let draft = launcher.app.options;
        let chosen = launcher.app.chosen.clone();

        launcher.change_layout(Layout::Full);
        launcher.edit_in_full("Office PC");
        let _ = launcher.click("Play remote audio here");
        let _ = launcher.click("Save");
        launcher.change_layout(Layout::Simple);

        assert_eq!(launcher.field(COMPUTER_BOX), computer_box);
        assert_eq!(launcher.app.chosen, chosen);
        assert_eq!(launcher.app.options, draft);
        assert!(launcher.saved("Lab").clipboard, "draft options are not saved early");
        assert!(!launcher.saved("Office PC").audio);
    }
}

#[test]
fn clearing_the_computer_box_disables_connect() {
    let mut launcher = Launcher::simple();
    launcher.type_into(COMPUTER_BOX, "Office PC");
    assert!(launcher.app.can_connect_simple());
    launcher.type_into(COMPUTER_BOX, "");
    launcher.press(Named::Escape);
    assert!(launcher.click("Connect").is_empty(), "a disabled Connect sends nothing");
    assert!(launcher.app.dialogs.is_empty());
    assert!(launcher.shows("Pick a computer, or type an address."));
}

#[test]
fn new_computer_keeps_the_typed_address_and_a_blank_name() {
    let mut launcher = Launcher::simple();
    launcher.type_into(COMPUTER_BOX, "192.0.2.40");
    let _ = launcher.click("Show options");
    let _ = launcher.click("New computer\u{2026}");
    assert!(launcher.shows("New computer"));
    assert_eq!(launcher.computer_form().name, "");
    assert_eq!(launcher.address_field(), "192.0.2.40");
}

#[test]
fn removing_a_computer_asks_first_then_removes_it() {
    let mut launcher = Launcher::simple();
    let _ = launcher.click("Lab");
    let _ = launcher.click("Show options");
    let _ = launcher.click("Edit\u{2026}");
    let _ = launcher.click("Remove");
    assert!(launcher.shows("Remove this computer?"));
    assert_eq!(
        launcher.saved_names(),
        ["Office PC", "Lab"],
        "nothing is removed before the user confirms"
    );
    let _ = launcher.click("Remove");
    assert_eq!(launcher.saved_names(), ["Office PC"]);
    assert!(!launcher.shows("Lab"));
}

#[test]
fn failing_to_remove_a_computer_keeps_it_selected() {
    let mut launcher = Launcher::simple();
    let _ = launcher.click("Lab");
    let selected = launcher.app.chosen.clone();
    let _ = launcher.click("Show options");
    let _ = launcher.click("Edit\u{2026}");
    let _ = launcher.click("Remove");
    launcher.backend_errors.borrow_mut().delete_profile = Some("The computer could not be removed.".into());

    let _ = launcher.click("Remove");

    assert!(launcher.shows("The computer could not be removed."));
    assert_eq!(launcher.saved_names(), ["Office PC", "Lab"]);
    assert_eq!(launcher.app.chosen, selected);
    assert_eq!(launcher.field(COMPUTER_BOX), "Lab");
}

#[test]
fn closing_the_window_with_nothing_open_does_not_ask() {
    let mut launcher = Launcher::simple();
    launcher.close_window();
    assert!(launcher.app.dialogs.is_empty());
}

// ---------- settings ----------

#[test]
fn choosing_the_full_layout_in_settings_saves_it() {
    let mut launcher = Launcher::simple();
    launcher.open_settings();
    launcher.click_choice("Full");
    assert_eq!(launcher.demo.borrow().library.preferences.layout, Layout::Full);
    let _ = launcher.click("Close");
    assert!(launcher.shows("Favourites"), "the window changed to the full layout");
}

#[test]
fn failing_to_save_a_layout_keeps_the_previous_window_and_preference() {
    let mut launcher = Launcher::simple();
    launcher.open_settings();
    launcher.backend_errors.borrow_mut().save_preferences = Some("The preferences could not be saved.".into());

    launcher.click_choice("Full");

    assert!(launcher.shows("The preferences could not be saved."));
    assert_eq!(launcher.app.layout, Layout::Simple);
    assert_eq!(launcher.app.library.preferences.layout, Layout::Simple);
    assert_eq!(launcher.demo.borrow().library.preferences.layout, Layout::Simple);
}

#[test]
fn settings_choices_answer_to_a_click_on_their_words() {
    let mut launcher = Launcher::simple();
    launcher.open_settings();
    let _ = launcher.click("Full");
    assert_eq!(launcher.demo.borrow().library.preferences.layout, Layout::Full);
    let _ = launcher.click("Dark appearance");
    assert!(launcher.demo.borrow().library.preferences.dark);
}

#[test]
fn the_dark_switch_saves_the_preference() {
    let mut launcher = Launcher::simple();
    launcher.open_settings();
    launcher.click_switch("Dark appearance");
    assert!(launcher.demo.borrow().library.preferences.dark);
    assert!(launcher.app.tokens().dark);
}

// ---------- the full workspace ----------

#[test]
fn search_enter_on_an_address_opens_the_computer_dialog_with_that_address() {
    let mut launcher = Launcher::full();
    launcher.enter(FILTER, "192.0.2.50");
    assert!(launcher.shows("Finish this computer"));
    assert_eq!(launcher.address_field(), "192.0.2.50");
    assert_eq!(launcher.computer_form().name, "");
}

#[test]
fn computer_names_are_shown_literally() {
    let mut launcher = Launcher::full();
    launcher.enter(FILTER, "192.0.2.50");
    launcher.type_into(FORM_NAME, "Test PC <script>");
    let _ = launcher.click("Save");
    launcher.type_into(FILTER, "");
    assert!(launcher.shows("Test PC <script>"));
}

#[test]
fn open_now_with_nothing_open_says_so() {
    let mut launcher = Launcher::full();
    let _ = launcher.click("Open now");
    assert!(launcher.shows("Nothing open right now"));
}

#[test]
fn disconnecting_a_connected_desktop_asks_first_then_ends_its_session() {
    let mut launcher = Launcher::full();
    let session = launcher.connect("Office PC", "dummy-test-only");
    launcher.answer(&session);
    let _ = launcher.click("Disconnect");
    assert!(launcher.shows("Disconnect from Office PC?"));
    assert!(launcher.killed().is_empty(), "nothing closes before the user confirms");
    let _ = launcher.click("Disconnect");
    assert_eq!(launcher.killed(), [session]);
    assert!(launcher.app.sessions.is_empty());
}

#[test]
fn disconnecting_a_desktop_still_connecting_does_not_ask() {
    let mut launcher = Launcher::full();
    let session = launcher.connect("Office PC", "dummy-test-only");
    let _ = launcher.click("Disconnect");
    assert!(launcher.app.dialogs.is_empty());
    assert_eq!(launcher.killed(), [session]);
}

#[test]
fn closing_the_window_with_desktops_open_asks_then_disconnects_them_all() {
    let mut launcher = Launcher::full();
    let mut open = vec![
        launcher.connect("Office PC", "dummy-test-only"),
        launcher.connect("Lab", "dummy-test-only"),
    ];
    launcher.close_window();
    assert!(launcher.shows("Close Win RDP?"));
    assert!(launcher.shows(
        "2 desktops are still open. The remote sessions stay signed in; every connection from this app closes."
    ));
    assert!(launcher.killed().is_empty(), "nothing closes before the user confirms");
    let _ = launcher.click("Close and disconnect");
    let mut killed = launcher.killed();
    killed.sort();
    open.sort();
    assert_eq!(killed, open);
}

// ---------- helpers ----------

#[test]
fn addresses_are_told_apart_from_computer_names() {
    for address in [
        "192.0.2.1",
        "pc.example.com",
        "office-pc:3390",
        "192.0.2.10:3389",
        "host_1.lan",
    ] {
        assert!(looks_like_address(address), "{address} is an address");
    }
    for other in [
        "",
        "Office PC",
        "office",
        "192.0.2.",
        "pc..lan",
        "host:",
        "host:rdp",
        ":3389",
        "Test PC <script>",
        ".example",
    ] {
        assert!(!looks_like_address(other), "{other:?} is not an address");
    }
}

#[test]
fn last_opened_reads_as_a_day_relative_to_now() {
    let now = parse_iso("2026-09-26T12:00:00.000Z").unwrap();
    let hours_ago = |hours: i64| format_iso(now - hours * 3_600_000);
    assert_eq!(relative_date(&hours_ago(1), now), "today");
    assert_eq!(relative_date(&hours_ago(24), now), "yesterday");
    assert_eq!(relative_date(&hours_ago(47), now), "yesterday");
    assert_eq!(relative_date(&hours_ago(72), now), "3 days ago");
    assert_eq!(relative_date(&hours_ago(29 * 24 + 23), now), "29 days ago");
    assert_eq!(relative_date("2026-08-01T09:30:00.000Z", now), "2026-08-01");
    assert_eq!(relative_date("not a date", now), "before");
}

// ---------- screenshots ----------

/// Redraws the launcher screenshots the README and the website show, from the sample
/// computers, in the light theme.
#[test]
#[ignore = "writes the published screenshots; run with `-- --ignored screenshots`"]
fn screenshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the repository root");
    let write = |path: &str, png: &[u8]| std::fs::write(root.join(path), png).unwrap_or_else(|e| panic!("{path}: {e}"));

    // Dark, as the website's guided tour shows it on the blue hero.
    let mut sample = Demo::sample();
    sample.library.preferences.dark = true;
    let simple = Launcher::open(sample, Layout::Simple);
    write("website/public/assets/launcher.png", &simple.png());

    let mut full = Launcher::open(Demo::sample(), Layout::Full);
    let session = full.connect("Office PC", "demo");
    full.answer(&session);
    let png = full.png();
    write("docs/screenshots/launcher-full.png", &png);
    write("website/public/assets/launcher-full.png", &png);
}

impl Launcher {
    /// The window as a PNG at 1x, drawn by tiny-skia, the renderer the window itself uses,
    /// with no pointer over it. (The simulator's own snapshots are fixed at 2x and add the
    /// renderer's name to the file name.)
    fn png(&self) -> Vec<u8> {
        use iced_test::core::clipboard;
        use iced_test::core::renderer::{Headless, Style};
        use iced_test::futures::futures::executor::block_on;
        use iced_test::runtime::user_interface::{Cache, UserInterface};

        let size = window_size(self.app.layout).1;
        let theme = self.app.tokens().theme();
        let base = theme.base();
        let mut renderer = block_on(<iced::Renderer as Headless>::new(style::ui_font(), 13.into(), None))
            .expect("the tiny-skia renderer");
        let mut ui = UserInterface::build(view::view(&self.app), size, Cache::default(), &mut renderer);
        let cursor = mouse::Cursor::Unavailable;
        let redraw = Event::Window(window::Event::RedrawRequested(iced::time::Instant::now()));
        let _ = ui.update(&[redraw], cursor, &mut renderer, &mut clipboard::Null, &mut Vec::new());
        ui.draw(
            &mut renderer,
            &theme,
            &Style {
                text_color: base.text_color,
            },
            cursor,
        );
        let (width, height) = (size.width as u32, size.height as u32);
        let rgba = renderer.screenshot(Size::new(width, height), 1.0, base.background_color);
        let rgb: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[r, g, b, _]| [r, g, b])
            .collect();

        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::High);
        let mut writer = encoder.write_header().expect("a PNG header");
        writer.write_image_data(&rgb).expect("PNG image data");
        writer.finish().expect("a finished PNG");
        png
    }
}
