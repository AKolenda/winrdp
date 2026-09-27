// SPDX-License-Identifier: AGPL-3.0-only
//! Appearance, layout and GNOME desktop-sharing settings.
use iced::alignment::Vertical;
use iced::mouse::Interaction;
use iced::widget::{Column, Space, button, column, container, mouse_area, radio, row, toggler};
use iced::{Fill, Length, Padding, padding};

use crate::app::{App, HOST_USER, Message};
use crate::host;
use crate::library::Layout;
use crate::style::{self, Tokens};

use super::Element;
use super::widgets::{dialog_frame, field_label, footer_button, hrule, input, label, scroll, strong};

/// A radio choice; clicking its words chooses it as well as clicking the circle.
fn choice_row<'a, V: Copy + Eq>(
    t: Tokens,
    title: &'a str,
    detail: &'a str,
    value: V,
    selected: Option<V>,
    on: impl FnOnce(V) -> Message,
) -> Element<'a> {
    let chosen = on(value);
    let choice = row![
        radio("", value, selected, |_| chosen.clone())
            .size(16)
            .spacing(0)
            .style(style::choice(t)),
        column![strong(title, 13.0, t.ink), label(detail, 12.0, t.mute)].spacing(2)
    ]
    .spacing(12)
    .padding([10, 0]);
    mouse_area(choice)
        .on_press(chosen.clone())
        .interaction(Interaction::Pointer)
        .into()
}
/// A switch with its label on the left; clicking the label toggles it too.
fn switch_row<'a>(
    t: Tokens,
    title: &'a str,
    detail: Option<String>,
    on: bool,
    message: Option<fn(bool) -> Message>,
) -> Element<'a> {
    let mut words = column![strong(title, 13.0, t.ink)].width(Fill).spacing(2);
    if let Some(detail) = detail {
        words = words.push(label(detail, 12.0, t.mute));
    }
    let content = row![
        words,
        toggler(on).size(20).style(style::switch(t)).on_toggle_maybe(message)
    ]
    .align_y(Vertical::Center);
    match message {
        Some(toggle) => mouse_area(content)
            .on_press(toggle(!on))
            .interaction(Interaction::Pointer)
            .into(),
        None => content.into(),
    }
}
fn panel<'a>(t: Tokens, rows: Vec<Element<'a>>) -> Element<'a> {
    let mut content = Column::new();
    for (i, r) in rows.into_iter().enumerate() {
        if i > 0 {
            content = content.push(hrule(t));
        }
        content = content.push(r);
    }
    container(content.padding([4, 12]))
        .width(Fill)
        .style(style::inset(t))
        .into()
}
fn section_title<'a>(t: Tokens, title: &'a str) -> Element<'a> {
    container(strong(title, 12.0, t.mute))
        .padding(padding::top(10).bottom(2))
        .into()
}
pub(super) fn view(app: &App, t: Tokens) -> Element<'_> {
    let host = &app.host;
    let status = host.status.clone().unwrap_or_default();
    let on = status.available && status.sharing;
    let (summary, explanation) = match &host.status {
        None => ("Checking\u{2026}".to_owned(), String::new()),
        Some(s) if !s.available => (
            "GNOME Remote Desktop is not installed.".to_owned(),
            "Install the gnome-remote-desktop package, then come back here.".to_owned(),
        ),
        Some(s) => (
            if on {
                format!(
                    "Sharing on port {}, {}.",
                    s.port,
                    if s.extend {
                        "virtual screen"
                    } else {
                        "showing your screen"
                    }
                )
            } else {
                "Off.".to_owned()
            },
            if on {
                "From Windows, open Remote Desktop Connection, enter this computer\u{2019}s address, and sign in with the sharing name and password.".to_owned()
            } else if s.headless {
                "Right now a separate-session mode (Remote Login) answers on this port, which gives Windows its own login instead of your desktop. Turning sharing on replaces it.".to_owned()
            } else if s.credentials {
                "Turn on sharing to let Windows connect to this desktop.".to_owned()
            } else {
                "Choose a sharing name and password, then turn sharing on. Windows signs in with these, not with your Linux account.".to_owned()
            },
        ),
    };
    let layout = Some(app.layout);
    let mut host_rows: Vec<Element<'_>> = vec![
        container(switch_row(
            t,
            "Accept connections from Windows",
            Some(summary),
            on,
            (status.available && !host.busy).then_some(Message::HostSwitch),
        ))
        .padding([10, 0])
        .into(),
    ];
    if !explanation.is_empty() {
        host_rows.push(container(label(explanation, 13.0, t.ink)).padding([10, 0]).into());
    }
    if status.available {
        host_rows.push(
            column![
                choice_row(
                    t,
                    "Show my screen",
                    "Windows sees exactly what is on this monitor. Needs the monitor connected and on.",
                    host::Mode::Mirror,
                    host.mode,
                    Message::HostMode
                ),
                choice_row(
                    t,
                    "Virtual screen",
                    "Same sign-in, on a screen of its own. Works with the monitor off or unplugged.",
                    host::Mode::Extend,
                    host.mode,
                    Message::HostMode
                ),
            ]
            .into(),
        );
    }
    if host.needs_credentials() {
        host_rows.push(
            row![
                field_label(
                    t,
                    "Sharing name",
                    input(t, "", &host.user, Message::HostUser).id(HOST_USER)
                ),
                field_label(
                    t,
                    "Sharing password",
                    input(t, "", host.pass.as_str(), |v| Message::HostPass(v.into()))
                        .secure(true)
                        .on_submit(Message::HostSubmit)
                ),
                button(strong("Turn on", 13.0, t.accent_ink))
                    .padding([8, 14])
                    .style(style::primary(t))
                    .on_press_maybe((!host.busy && !host.pass.is_empty()).then_some(Message::HostSubmit)),
            ]
            .spacing(8)
            .align_y(Vertical::Bottom)
            .padding(padding::top(4).bottom(10))
            .into(),
        );
    }
    host_rows.push(
        column![
            row![
                button(label("Open GNOME's Remote Desktop settings", 13.0, t.ink))
                    .padding([6, 8])
                    .style(style::quiet(t))
                    .on_press(Message::HostOpenSettings),
                button(label("Refresh", 13.0, t.ink))
                    .padding([6, 8])
                    .style(style::quiet(t))
                    .on_press_maybe((!host.busy).then_some(Message::HostRefresh)),
            ]
            .spacing(6),
            container(label(
                "Sharing mirrors the desktop you are signed in to, so Windows sees your screen, not a separate login. \
                 It runs at every sign-in whether or not Win RDP is open, over TCP; hosting over RDP-UDP is not built yet.",
                13.0,
                t.mute,
            ))
            .padding([10, 0]),
        ]
        .into(),
    );
    let body =
        column![
            section_title(t, "Layout"),
            panel(
                t,
                vec![
                    choice_row(
                        t,
                        "Simple",
                        "One small window: pick a computer, connect. Every desktop opens in its own window.",
                        Layout::Simple,
                        layout,
                        Message::SetLayout,
                    ),
                    choice_row(
                        t,
                        "Full",
                        "Sidebar, search, and a tab for every open desktop.",
                        Layout::Full,
                        layout,
                        Message::SetLayout,
                    ),
                ],
            ),
            section_title(t, "Appearance"),
            panel(
                t,
                vec![
                    container(switch_row(
                        t,
                        "Dark appearance",
                        None,
                        app.library.preferences.dark,
                        Some(Message::SetDark),
                    ))
                    .height(44)
                    .align_y(Vertical::Center)
                    .into()
                ],
            ),
            section_title(t, "Connecting to Windows"),
            panel(
                t,
                vec![container(label(
                "Desktops open over reliable RDP-UDP when the host allows it, using MS-RDPEUDP version 1 or 2 or \
                 MS-RDPEUDP2 (shown as UDP v1, v2 and v3), and fall back to TCP by themselves. Microphone and \
                 printer redirection are per computer, under Edit.",
                13.0,
                t.ink,
            ))
            .padding([10, 0])
            .into()],
            ),
            section_title(t, "Letting Windows connect to this computer"),
            panel(t, host_rows),
            container(label(
                format!(
                    "Win RDP {}. Every desktop opens in its own window, powered by the IronRDP engine.",
                    app.version,
                ),
                13.0,
                t.mute,
            ))
            .padding([10, 0]),
        ]
        .spacing(6)
        .padding(Padding {
            top: 14.0,
            right: 22.0,
            bottom: 18.0,
            left: 22.0,
        });
    let footer = row![
        Space::new().width(Fill),
        footer_button(t, "Close", false, Some(Message::CloseDialog))
    ];
    let height = if app.layout == Layout::Simple { 470.0 } else { 600.0 };
    dialog_frame(
        t,
        560.0,
        Some("Settings"),
        scroll(t, body).height(Length::Fixed(height)).into(),
        footer.into(),
    )
}
