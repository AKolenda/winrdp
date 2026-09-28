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
mod keyring;
mod library;
mod style;
#[cfg(test)]
mod tests;
mod view;

use iced::{Task, window};

use crate::app::{App, Message};

fn main() -> iced::Result {
    if std::env::args().any(|a| a == "--version") {
        println!("Win RDP {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let layout = backend::Store::layout_hint();
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
        .window(app::window_settings(layout))
        .run()
}

fn boot() -> (App, Task<Message>) {
    let (mut app, error) = App::new(Box::new(backend::Real::new()));
    let opened = window::oldest().map(Message::WindowOpened);
    let notice = error.map_or_else(Task::none, |e| app.update(Message::Notify(e)));
    (app, Task::batch([opened, notice]))
}
