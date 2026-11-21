// SPDX-License-Identifier: AGPL-3.0-only
//! Appearance, layout and GNOME desktop-sharing settings: a page of the full window, a dialog
//! of the simple one. Every setting is one card row; longer explanations fold under a chevron.
use iced::alignment::Vertical;
use iced::mouse::Interaction;
use iced::widget::{Column, Space, button, column, container, mouse_area, radio, row, svg, toggler};
use iced::{Fill, Length, Padding, padding};

use crate::app::{App, Details, HOST_USER, Message};
use crate::host;
use crate::library::Layout;
use crate::style::{self, ICONS, Tokens, icon};

use super::Element;
use super::widgets::{dialog_frame, field_label, footer_button, hrule, input, label, scroll, strong};

/// Where a row's words start, so text under a setting lines up with its title.
const INDENT: f32 = 16.0 + 18.0 + 16.0;

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
        column![label(title, 14.0, t.ink), label(detail, 12.0, t.mute)].spacing(2)
    ]
    .spacing(12)
    .padding([8, 0]);
    mouse_area(choice)
        .on_press(chosen.clone())
        .interaction(Interaction::Pointer)
        .into()
}
/// One setting: an icon, its title and a one-line description, and its control on the right.
fn setting<'a>(
    t: Tokens,
    glyph: &svg::Handle,
    title: &'a str,
    detail: Option<String>,
    control: Option<Element<'a>>,
) -> iced::widget::Row<'a, Message> {
    let mut words = column![label(title, 14.0, t.ink)].width(Fill).spacing(2);
    if let Some(detail) = detail.filter(|d| !d.is_empty()) {
        words = words.push(label(detail, 12.0, t.mute));
    }
    let mut content = row![icon(glyph, 18.0, t.ink), words]
        .spacing(16)
        .align_y(Vertical::Center);
    if let Some(control) = control {
        content = content.push(control);
    }
    content
}
/// A setting with a switch; clicking anywhere on the row toggles it too.
fn switch_setting<'a>(
    t: Tokens,
    glyph: &svg::Handle,
    title: &'a str,
    detail: Option<String>,
    on: bool,
    message: Option<fn(bool) -> Message>,
) -> Element<'a> {
    let switch = row![
        label(if on { "On" } else { "Off" }, 14.0, t.ink),
        toggler(on).size(20).style(style::switch(t)).on_toggle_maybe(message)
    ]
    .spacing(12)
    .align_y(Vertical::Center);
    let content = container(setting(t, glyph, title, detail, Some(switch.into())))
        .padding([14, 16])
        .width(Fill);
    match message {
        Some(toggle) => mouse_area(content)
            .on_press(toggle(!on))
            .interaction(Interaction::Pointer)
            .into(),
        None => content.into(),
    }
}
/// A plain setting row inside a card.
fn setting_row<'a>(
    t: Tokens,
    glyph: &svg::Handle,
    title: &'a str,
    detail: Option<String>,
    control: Option<Element<'a>>,
) -> Element<'a> {
    container(setting(t, glyph, title, detail, control))
        .padding([14, 16])
        .width(Fill)
        .into()
}
/// Content under a setting's title rather than under its icon.
fn indented<'a>(content: impl Into<Element<'a>>) -> Element<'a> {
    container(content)
        .padding(Padding {
            top: 4.0,
            right: 16.0,
            bottom: 10.0,
            left: INDENT,
        })
        .width(Fill)
        .into()
}
/// A row that folds an explanation away; the chevron shows which way it is.
fn expander<'a>(app: &App, t: Tokens, which: Details, title: &'a str, body: &'a str) -> Vec<Element<'a>> {
    let open = app.details.contains(&which);
    let header = button(
        row![
            icon(&ICONS.info, 18.0, t.ink),
            label(title, 14.0, t.ink).width(Fill),
            icon(
                if open {
                    &ICONS.chevron_down
                } else {
                    &ICONS.chevron_right
                },
                16.0,
                t.mute
            ),
        ]
        .spacing(16)
        .align_y(Vertical::Center),
    )
    .width(Fill)
    .padding([14, 16])
    .style(style::quiet(t))
    .on_press(Message::ToggleDetails(which));
    let mut rows = vec![header.into()];
    if open {
        rows.push(indented(label(body, 13.0, t.mute)));
    }
    rows
}
/// Settings rows on one card, a hairline between them.
fn card<'a>(t: Tokens, rows: Vec<Element<'a>>) -> Element<'a> {
    let mut content = Column::new();
    for (i, r) in rows.into_iter().enumerate() {
        if i > 0 {
            content = content.push(hrule(t));
        }
        content = content.push(r);
    }
    container(content)
        .width(Fill)
        .style(style::card(t, style::SURFACE))
        .clip(true)
        .into()
}
fn group_title<'a>(t: Tokens, title: &'a str) -> Element<'a> {
    container(strong(title, 14.0, t.ink))
        .padding(padding::top(18).bottom(4))
        .into()
}
fn caption<'a>(t: Tokens, words: impl iced::widget::text::IntoFragment<'a>) -> Element<'a> {
    container(label(words, 12.0, t.mute)).padding([2, 4]).into()
}

fn sharing<'a>(app: &'a App, t: Tokens) -> Vec<Element<'a>> {
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
                    "On \u{b7} port {} \u{b7} {}",
                    s.port,
                    if s.extend {
                        "virtual screen"
                    } else {
                        "showing your screen"
                    }
                )
            } else {
                String::new()
            },
            if on {
                "From Windows, open Remote Desktop Connection, enter this computer\u{2019}s address, and sign in with the sharing name and password.".to_owned()
            } else if s.headless {
                "Right now a separate-session mode (Remote Login) answers on this port, which gives Windows its own login instead of your desktop. Turning sharing on replaces it.".to_owned()
            } else if s.credentials {
                String::new()
            } else {
                "Choose a sharing name and password, then turn sharing on. Windows signs in with these, not with your Linux account.".to_owned()
            },
        ),
    };
    let mut rows: Vec<Element<'_>> = vec![switch_setting(
        t,
        &ICONS.share,
        "Accept connections from Windows",
        Some(if host.busy {
            "Updating sharing settings\u{2026}".into()
        } else {
            summary
        }),
        on,
        (status.available && !host.busy).then_some(Message::HostSwitch),
    )];
    let mut inner = Column::new().spacing(2);
    let explained = !explanation.is_empty();
    if explained {
        inner = inner.push(container(label(explanation, 13.0, t.mute)).padding(padding::bottom(6)));
    }
    if status.available && !host.busy {
        inner = inner
            .push(choice_row(
                t,
                "Show my screen",
                "Windows sees exactly what is on this monitor. Needs the monitor connected and on.",
                host::Mode::Mirror,
                host.mode,
                Message::HostMode,
            ))
            .push(choice_row(
                t,
                "Virtual screen",
                "Same sign-in, on a screen of its own. Works with the monitor off or unplugged.",
                host::Mode::Extend,
                host.mode,
                Message::HostMode,
            ));
    }
    if host.needs_credentials() && !host.busy {
        inner = inner.push(
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
            .padding(padding::top(6)),
        );
    }
    if explained || (status.available && !host.busy) || host.needs_credentials() {
        rows.push(indented(inner));
    }
    let open = button(
        row![label("Open", 13.0, t.ink), icon(&ICONS.external, 14.0, t.ink)]
            .spacing(8)
            .align_y(Vertical::Center),
    )
    .padding([6, 12])
    .style(style::plain(t))
    .on_press(Message::HostOpenSettings);
    let refresh = button(
        row![icon(&ICONS.refresh, 14.0, t.ink), label("Refresh", 13.0, t.ink)]
            .spacing(8)
            .align_y(Vertical::Center),
    )
    .padding([6, 12])
    .style(style::plain(t))
    .on_press_maybe((!host.busy).then_some(Message::HostRefresh));
    rows.push(setting_row(
        t,
        &ICONS.gear,
        "GNOME Remote Desktop settings",
        None,
        Some(row![refresh, open].spacing(8).into()),
    ));
    rows.extend(expander(
        app,
        t,
        Details::Sharing,
        "About sharing",
        "Sharing mirrors the desktop you are signed in to, so Windows sees your screen, not a separate login. \
         It runs at every sign-in whether or not Win RDP is open, over TCP; hosting over RDP-UDP is not built yet.",
    ));
    rows
}

/// The settings themselves, the same in the page and the dialog.
fn body(app: &App, t: Tokens) -> Column<'_, Message> {
    let layout = Some(app.layout);
    let mut connecting = vec![setting_row(
        t,
        &ICONS.network,
        "UDP when the computer allows it",
        Some("Otherwise the desktop opens over TCP, by itself.".into()),
        Some(label("Automatic", 13.0, t.mute).into()),
    )];
    connecting.extend(expander(
        app,
        t,
        Details::Protocol,
        "Protocol details",
        "Desktops open over reliable RDP-UDP using MS-RDPEUDP version 1 or 2 or MS-RDPEUDP2, \
         shown on a desktop's tab as UDP v1, v2 and v3.",
    ));
    column![
        group_title(t, "Appearance"),
        card(
            t,
            vec![
                switch_setting(
                    t,
                    &ICONS.palette,
                    "Dark appearance",
                    None,
                    app.library.preferences.dark,
                    Some(Message::SetDark),
                ),
                column![
                    setting_row(
                        t,
                        &ICONS.layout,
                        "Window layout",
                        Some("How Win RDP opens. Every desktop still gets a window of its own.".into()),
                        None,
                    ),
                    indented(column![
                        choice_row(
                            t,
                            "Simple",
                            "One small window: pick a computer, connect.",
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
                    ]),
                ]
                .into(),
            ],
        ),
        group_title(t, "Connecting to Windows"),
        card(t, connecting),
        caption(t, "Microphone and printer are set per computer, under Edit."),
        group_title(t, "Sharing this computer"),
        card(t, sharing(app, t)),
        container(
            row![
                svg(ICONS.logo.clone()).width(16).height(16),
                label(
                    format!("Win RDP {} \u{b7} powered by the IronRDP engine", app.version),
                    12.0,
                    t.mute
                ),
            ]
            .spacing(10)
            .align_y(Vertical::Center)
        )
        .padding(padding::top(20).left(4)),
    ]
    .spacing(4)
}

/// Settings as the full window's page, in a column no wider than reads comfortably.
pub(super) fn page(app: &App, t: Tokens) -> Element<'_> {
    container(body(app, t).max_width(880)).width(Fill).into()
}

/// Settings as a dialog over the simple window.
pub(super) fn view(app: &App, t: Tokens) -> Element<'_> {
    let content = body(app, t).padding(Padding {
        top: 0.0,
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
        600.0,
        Some("Settings"),
        scroll(t, content).height(Length::Fixed(height)).into(),
        footer.into(),
    )
}
