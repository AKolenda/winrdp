// SPDX-License-Identifier: AGPL-3.0-only
//! The compact launcher, its recent computers and address suggestions.
use iced::alignment::{Horizontal, Vertical};
use iced::widget::text::LineHeight;
use iced::widget::{Column, Space, button, column, container, hover, mouse_area, row, stack, svg, text_input};
use iced::{Color, Fill, Padding, padding};

use crate::app::{App, COMPUTER_BOX, Message, Session, looks_like_address};
use crate::library::{Layout, Profile};
use crate::style::{self, ICONS, Tokens, bold, icon};

use super::Element;
use super::widgets::{caption_button, connection_options, hrule, label, line, resolution_picker, scroll, strong};

pub(super) fn view(app: &App, t: Tokens) -> Element<'_> {
    let caption = row![
        Space::new().width(12),
        svg(ICONS.logo.clone()).width(16).height(16),
        Space::new().width(8),
        label("Win RDP", 12.0, t.ink),
        mouse_area(Space::new().width(Fill).height(Fill)).on_press(Message::Drag),
        caption_button(t, &ICONS.expand, Message::SetLayout(Layout::Full), false),
        caption_button(t, &ICONS.gear, Message::OpenSettings, false),
        caption_button(t, &ICONS.minimize, Message::Minimize, false),
        caption_button(t, &ICONS.close, Message::CloseRequested, true),
    ]
    .height(40)
    .align_y(Vertical::Center);

    let combo = stack![
        text_input("10.0.0.7", &app.computer_box)
            .id(COMPUTER_BOX)
            .on_input(Message::BoxInput)
            .on_submit(Message::BoxSubmit)
            .padding(Padding {
                top: 8.0,
                right: 38.0,
                bottom: 8.0,
                left: 10.0
            })
            .size(14)
            .style(style::field(t)),
        container(
            button(container(icon(&ICONS.chevron_down, 16.0, t.mute)).center(Fill))
                .width(34)
                .height(Fill)
                .padding(0)
                .style(style::quiet_mute(t))
                .on_press(Message::ComboToggle)
        )
        .width(Fill)
        .height(Fill)
        .align_x(Horizontal::Right)
        .padding(1),
    ]
    .height(36);

    let value = app.computer_box.trim();
    let signin: Element<'_> = match app.simple_match() {
        Some(p) => row![
            label("Signing in as ", 12.5, t.mute),
            strong(
                if p.username.is_empty() {
                    "the account you enter".to_owned()
                } else {
                    p.username.clone()
                },
                12.5,
                t.ink
            ),
            Space::new().width(Fill),
            button(label("Change", 12.5, t.accent))
                .padding(0)
                .style(style::link(t))
                .on_press(Message::EditChosen),
        ]
        .into(),
        None if looks_like_address(value) => label(
            format!("New computer at {value}. Enter asks for the account."),
            12.5,
            t.mute,
        )
        .into(),
        None => label("Pick a computer, or type an address.", 12.5, t.mute).into(),
    };

    let recent = app.recent();
    let list: Element<'_> = if recent.is_empty() {
        column![
            hrule(t),
            container(label(
                "No saved computers yet. Type an address above and press Enter.",
                13.0,
                t.mute
            ))
            .padding([14, 4])
        ]
        .height(Fill)
        .into()
    } else {
        column![
            hrule(t),
            scroll(
                t,
                Column::with_children(
                    recent
                        .into_iter()
                        .map(|p| { hover(recent_item(app, t, p, false), recent_item(app, t, p, true)) })
                )
            )
            .height(Fill)
        ]
        .height(Fill)
        .into()
    };

    let disclosure = button(
        row![
            icon(
                if app.options_open {
                    &ICONS.chevron_down
                } else {
                    &ICONS.chevron_right
                },
                16.0,
                t.ink
            ),
            label(
                if app.options_open {
                    "Hide options"
                } else {
                    "Show options"
                },
                13.0,
                t.ink
            )
        ]
        .spacing(6)
        .align_y(Vertical::Center),
    )
    .padding([6, 4])
    .style(style::quiet(t))
    .on_press(Message::OptionsToggle);
    let connect = button(label("Connect", 13.0, t.accent_ink).font(bold()))
        .padding([8, 16])
        .style(style::primary(t))
        .on_press_maybe(app.can_connect_simple().then_some(Message::ConnectChosen));
    let options: Element<'_> = if app.options_open {
        column![hrule(t)]
            .extend(connection_options(t, app.options, Message::SetConnectionOption))
            .push(resolution_picker(t, app.options.resolution, Message::SetResolution))
            .push(
                row![
                    button(label("New computer\u{2026}", 13.0, t.ink))
                        .padding([5, 8])
                        .style(style::quiet(t))
                        .on_press(Message::NewFromBox),
                    button(label("Edit\u{2026}", 13.0, t.ink))
                        .padding([5, 8])
                        .style(style::quiet(t))
                        .on_press_maybe(app.chosen.as_ref().map(|_| Message::EditChosen)),
                ]
                .spacing(6),
            )
            .spacing(8)
            .padding(padding::top(10).bottom(2))
            .into()
    } else {
        Space::new().into()
    };

    let body = column![
        Space::new().height(10),
        label("Computer", 12.0, t.mute).line_height(LineHeight::Absolute(17.0.into())),
        Space::new().height(6),
        combo,
        container(signin).padding(padding::top(8)),
        container(strong("Recent", 12.0, t.mute)).padding(padding::top(18).bottom(2)),
        list,
        container(row![disclosure, Space::new().width(Fill), connect].align_y(Vertical::Center))
            .padding(padding::top(14)),
        options,
    ]
    .padding(Padding {
        top: 4.0,
        right: 22.0,
        bottom: 18.0,
        left: 22.0,
    });
    column![caption, body].into()
}
fn recent_item<'a>(app: &'a App, t: Tokens, p: &'a Profile, hovered: bool) -> Element<'a> {
    let live = app.live_for(&p.id);
    let selected = app.chosen.as_deref() == Some(p.id.as_str());
    let tag = live
        .map(|s| {
            if s.label().is_empty() {
                "Open".to_owned()
            } else {
                s.label().to_owned()
            }
        })
        .unwrap_or_default();
    let tag_udp = live.is_some_and(Session::udp);
    let more: Element<'_> = if hovered || selected {
        button(container(icon(&ICONS.more, 16.0, t.mute)).center(Fill))
            .width(28)
            .height(26)
            .padding(0)
            .style(style::quiet_mute(t))
            .on_press(Message::Edit(p.id.clone()))
            .into()
    } else {
        Space::new().width(28).into()
    };
    let tag_text = if tag_udp {
        strong(tag, 11.0, t.accent)
    } else {
        label(tag, 11.0, t.mute)
    };
    let content = row![
        line(p.name.as_str(), 13.0, t.ink, Fill),
        label(p.address.as_str(), 13.0, t.mute),
        container(tag_text).width(60).align_x(Horizontal::Right),
        more,
    ]
    .spacing(10)
    .align_y(Vertical::Center);
    let fill = if selected {
        t.selected
    } else if hovered {
        t.hover
    } else {
        Color::TRANSPARENT
    };
    let item = column![
        container(content)
            .height(35)
            .padding(Padding {
                top: 0.0,
                right: 2.0,
                bottom: 0.0,
                left: 8.0
            })
            .align_y(Vertical::Center)
            .style(style::fill(fill, 3.0)),
        hrule(t)
    ];
    mouse_area(item)
        .on_press(Message::Choose(p.id.clone()))
        .on_double_click(Message::Activate(p.id.clone()))
        .into()
}
pub(super) fn combo_list<'a>(t: Tokens, items: Vec<&'a Profile>) -> Element<'a> {
    let rows = Column::with_children(items.into_iter().map(|p| {
        button(
            row![
                line(p.name.as_str(), 13.0, t.ink, Fill),
                label(p.address.as_str(), 13.0, t.mute)
            ]
            .spacing(10),
        )
        .width(Fill)
        .padding([7, 10])
        .style(style::row(t, false))
        .on_press(Message::ComboPick(p.id.clone()))
        .into()
    }));
    let popup = container(scroll(t, rows))
        .padding(4)
        .max_height(220)
        .width(Fill)
        .style(style::popup(t));
    // Below the Computer box: caption 40 + padding 4 + label block 33 + box 36 + gap 4.
    container(popup)
        .width(Fill)
        .padding(Padding {
            top: 117.0,
            right: 21.0,
            bottom: 0.0,
            left: 21.0,
        })
        .into()
}
