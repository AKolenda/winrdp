// SPDX-License-Identifier: AGPL-3.0-only
//! Win RDP's launcher: pick a computer, and each desktop opens in its own
//! `winrdp-session` window.
// Test fixtures and simulator assertions should fail immediately with their context.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

#[cfg(not(target_os = "linux"))]
compile_error!("Win RDP targets Linux only.");

mod app;
mod backend;
mod endpoint;
mod host;
mod library;
mod style;
#[cfg(test)]
mod tests;
mod view;

use iced::{Task, window};

use crate::app::{App, Message};
use crate::library::Layout;

fn main() -> iced::Result {
    if std::env::args().any(|a| a == "--version") {
        println!("Win RDP {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let layout = backend::Store::layout_hint();
    let (min, size) = app::window_size(layout);
    iced::application(boot, App::update, view::view)
        .title(App::title)
        .theme(|app: &App| app.tokens().theme())
        .subscription(App::subscription)
        .settings(iced::Settings {
            id: Some("winrdp-next".into()),
            default_font: style::ui_font(),
            default_text_size: 13.into(),
            ..iced::Settings::default()
        })
        .window(window::Settings {
            size,
            min_size: Some(min),
            resizable: layout == Layout::Full,
            decorations: !view::undecorated(),
            exit_on_close_request: false,
            icon: window_icon(),
            // Matches StartupWMClass in io.winrdp.Next.desktop, so the dock shows the app icon.
            platform_specific: window::settings::PlatformSpecific {
                application_id: "winrdp-next".into(),
                ..Default::default()
            },
            ..window::Settings::default()
        })
        .centered()
        .run()
}

fn boot() -> (App, Task<Message>) {
    let (mut app, error) = App::new(Box::new(backend::Real::new()));
    let opened = window::oldest().map(Message::WindowOpened);
    let notice = error.map_or_else(Task::none, |e| app.update(Message::Notify(e)));
    (app, Task::batch([opened, notice]))
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
