// SPDX-License-Identifier: AGPL-3.0-only
//! Host-operation ordering without running any GNOME commands. Tasks are inspected
//! and dropped: executing them would change the real desktop-sharing service.
use super::{App, Message};
use crate::{backend::Demo, host};

fn sharing_app() -> App {
    let (mut app, _) = App::new(Box::new(Demo::default()));
    app.host.status = Some(host::Status {
        available: true,
        sharing: true,
        credentials: true,
        ..host::Status::default()
    });
    app.host.mode = Some(host::Mode::Mirror);
    app
}

#[test]
fn a_pending_disable_cannot_be_reversed_by_another_host_action() {
    let mut app = sharing_app();
    let disable = app.update(Message::HostSwitch(false));
    assert_eq!(disable.units(), 1);
    assert!(app.host.busy);

    // The status still says sharing is on until the pending task returns.
    assert!(app.host.status.as_ref().unwrap().sharing);
    for message in [
        Message::HostMode(host::Mode::Extend),
        Message::HostSwitch(true),
        Message::HostSwitch(false),
        Message::HostSubmit,
        Message::HostRefresh,
    ] {
        assert_eq!(app.update(message).units(), 0);
    }
    assert_eq!(app.host.mode, Some(host::Mode::Mirror));
    assert!(app.host.busy);
}

#[test]
fn a_pending_status_request_cannot_overlap_a_host_mutation() {
    let mut app = sharing_app();
    let refresh = app.update(Message::HostRefresh);
    assert_eq!(refresh.units(), 1);
    assert!(app.host.busy);
    assert_eq!(app.update(Message::HostRefresh).units(), 0);
    assert_eq!(app.update(Message::HostSwitch(false)).units(), 0);
    assert_eq!(app.update(Message::OpenSettings).units(), 0);
}

#[test]
fn submitting_an_incomplete_sharing_form_does_not_start_hosting() {
    let mut app = sharing_app();
    app.host.status.as_mut().unwrap().sharing = false;
    app.host.status.as_mut().unwrap().credentials = false;
    app.host.pass = "dummy-password".to_owned().into();
    app.host.user = "  ".into();
    assert_eq!(app.update(Message::HostSubmit).units(), 0);
    assert!(!app.host.busy);
    assert!(!app.host.pass.is_empty());

    app.host.user = "audit-user".into();
    let enable = app.update(Message::HostSubmit);
    assert_eq!(enable.units(), 1);
    assert!(app.host.busy);
    assert!(app.host.pass.is_empty());
    assert_eq!(app.update(Message::HostSubmit).units(), 0);
}

#[test]
fn changing_mode_after_disable_completes_does_not_turn_sharing_back_on() {
    let mut app = sharing_app();
    let disable = app.update(Message::HostSwitch(false));
    assert_eq!(disable.units(), 1);
    let mut off = app.host.status.clone().unwrap();
    off.sharing = false;
    app.host_loaded(off);
    assert_eq!(app.update(Message::HostMode(host::Mode::Extend)).units(), 0);
    assert_eq!(app.host.mode, Some(host::Mode::Extend));
    assert!(!app.host.busy);
}

#[test]
fn shutdown_needs_a_confirmed_stopped_status_before_reporting_sharing_is_off() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let mut app = sharing_app();
    for (status, expected) in [
        (host::Status::default(), "Could not confirm that sharing is off."),
        (app.host.status.clone().unwrap(), "Sharing is still on."),
    ] {
        let _ = app.update(Message::HostDisabled(Ok(status)));
        assert!(app.toast.as_ref().unwrap().text.starts_with(expected));
    }
}
