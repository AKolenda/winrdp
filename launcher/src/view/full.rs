// SPDX-License-Identifier: AGPL-3.0-only
//! The full workspace: navigation, session tabs and the computer library.
use iced::alignment::{Horizontal, Vertical};
use iced::widget::text::Wrapping;
use iced::widget::{Column, Row, Space, button, column, container, hover, mouse_area, row, svg, text, text_input};
use iced::{Fill, Length, Padding, Shrink, padding};

use crate::app::{App, FILTER, Message, Page, Session, SessionState, relative_date};
use crate::library::Profile;
use crate::style::{self, ICONS, Tokens, bold, icon};

use super::widgets::{caption_button, hrule, label, line, pill, scroll, strong, vrule};
use super::{Element, now_ms};

pub(super) fn view(app: &App, t: Tokens) -> Element<'_> {
    let tabs = Row::with_children(app.sessions.iter().map(|s| tab(t, s)))
        .spacing(2)
        .align_y(Vertical::Bottom);
    let caption = row![
        container(tabs)
            .height(Fill)
            .align_y(Vertical::Bottom)
            .padding(padding::left(10)),
        mouse_area(Space::new().width(Fill).height(Fill))
            .on_press(Message::Drag)
            .on_double_click(Message::ToggleMaximize),
        caption_button(t, &ICONS.minimize, Message::Minimize, false),
        caption_button(t, &ICONS.maximize, Message::ToggleMaximize, false),
        caption_button(t, &ICONS.close, Message::CloseRequested, true),
    ]
    .height(40);
    let count = |n: usize| if n == 0 { String::new() } else { n.to_string() };
    let nav = |caption: &'static str, page: Page, n: usize| {
        let active = app.page == page;
        button(
            row![
                if active {
                    strong(caption, 13.0, t.ink)
                } else {
                    label(caption, 13.0, t.ink)
                },
                Space::new().width(Fill),
                label(count(n), 11.0, t.mute)
            ]
            .height(Fill)
            .align_y(Vertical::Center),
        )
        .width(Fill)
        .height(34)
        .padding([0, 10])
        .style(style::nav(t, active))
        .on_press(Message::ShowPage(page))
    };
    let sidebar = column![
        row![
            svg(ICONS.logo.clone()).width(22).height(22),
            strong("Win RDP", 14.0, t.ink)
        ]
        .spacing(10)
        .align_y(Vertical::Center)
        .padding(Padding {
            top: 4.0,
            right: 10.0,
            bottom: 18.0,
            left: 10.0
        }),
        nav("Computers", Page::Computers, app.library.computers.len()),
        nav(
            "Favourites",
            Page::Favourites,
            app.library.computers.iter().filter(|p| p.favorite).count()
        ),
        nav("Open now", Page::Open, app.sessions.len()),
        Space::new().height(Fill),
        button(
            container(label("Settings", 13.0, t.ink))
                .height(Fill)
                .align_y(Vertical::Center)
        )
        .width(Fill)
        .height(34)
        .padding([0, 10])
        .style(style::nav(t, false))
        .on_press(Message::OpenSettings),
        container(label(format!("Win RDP {}", app.version), 11.0, t.mute)).padding([6, 10]),
    ]
    .spacing(2)
    .width(212)
    .padding([16, 10]);

    let title = match app.page {
        Page::Favourites => "Favourites",
        Page::Open => "Open now",
        Page::Computers => "Computers",
    };
    let search = container(
        row![
            icon(&ICONS.search, 16.0, t.mute),
            text_input("Find, or type an address and press Enter", &app.filter)
                .id(FILTER)
                .on_input(Message::FilterInput)
                .on_submit(Message::FilterSubmit)
                .padding(0)
                .size(13)
                .style(style::bare_field(t)),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .width(320)
    .height(34)
    .padding([0, 10])
    .align_y(Vertical::Center)
    .style(style::card(t, 7.0));
    let heading = row![
        strong(title, 20.0, t.ink).width(Fill),
        search,
        button(label("New computer", 13.0, t.accent_ink).font(bold()))
            .padding([8, 14])
            .style(style::primary(t))
            .on_press(Message::NewComputer)
    ]
    .spacing(12)
    .align_y(Vertical::Center);

    let (open, saved) = app.library_rows();
    let mut page = Column::new().spacing(12).push(heading);
    let empty = open.is_empty() && saved.is_empty();
    if !open.is_empty() {
        page = page.push(section(app, t, "Open now", open.clone()));
    }
    if !saved.is_empty() {
        page = page.push(section(
            app,
            t,
            if open.is_empty() { "Computers" } else { "Saved" },
            saved,
        ));
    }
    if empty {
        let (heading, hint) = if app.library.computers.is_empty() {
            (
                "Add your first computer",
                "Save its address and username, or type an address in the search field and press Enter.",
            )
        } else if app.page == Page::Open {
            (
                "Nothing open right now",
                "A desktop is listed here while its window is open.",
            )
        } else {
            (
                "No matching computers",
                "Try another search, or type an address and press Enter.",
            )
        };
        page = page.push(
            container(
                column![
                    icon(&ICONS.add_computer, 48.0, t.mute),
                    strong(heading, 16.0, t.ink),
                    label(hint, 13.0, t.mute),
                    button(label("New computer", 13.0, t.ink))
                        .padding([7, 14])
                        .style(style::plain(t))
                        .on_press(Message::NewComputer),
                ]
                .spacing(10)
                .align_x(Horizontal::Center),
            )
            .width(Fill)
            .padding([56, 20]),
        );
    }
    let main = scroll(
        t,
        page.padding(Padding {
            top: 20.0,
            right: 26.0,
            bottom: 20.0,
            left: 18.0,
        }),
    )
    .height(Fill);
    column![caption, row![sidebar, vrule(t), main].height(Fill)].into()
}
fn section<'a>(app: &'a App, t: Tokens, caption: &'static str, rows: Vec<&'a Profile>) -> Element<'a> {
    let mut list = Column::new();
    for (i, p) in rows.into_iter().enumerate() {
        if i > 0 {
            list = list.push(hrule(t));
        }
        list = list.push(hover(library_row(app, t, p, false), library_row(app, t, p, true)));
    }
    column![
        container(label(caption, 12.0, t.mute)).padding([0, 4]),
        container(list).style(style::card(t, 10.0)).clip(true)
    ]
    .spacing(6)
    .into()
}
fn tab<'a>(t: Tokens, s: &'a Session) -> Element<'a> {
    let pill_text = if s.label().is_empty() {
        if s.state == SessionState::Connected {
            "Connected"
        } else {
            "Connecting"
        }
        .to_owned()
    } else {
        s.label().to_owned()
    };
    container(
        row![
            line(s.name.as_str(), 12.0, t.ink, Shrink),
            pill(t, pill_text, s.udp()),
            button(container(icon(&ICONS.tab_close, 14.0, t.mute)).center(Fill))
                .width(22)
                .height(22)
                .padding(0)
                .style(style::quiet_mute(t))
                .on_press(Message::Disconnect(s.id.clone())),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .height(32)
    .padding(Padding {
        top: 0.0,
        right: 6.0,
        bottom: 0.0,
        left: 11.0,
    })
    .align_y(Vertical::Center)
    .into()
}
fn library_row<'a>(app: &'a App, t: Tokens, p: &'a Profile, hovered: bool) -> Element<'a> {
    let live = app.live_for(&p.id);
    let extras = [
        if p.fullscreen { "Full screen" } else { "Window" },
        if p.favorite { "Favourite" } else { "" },
        if p.microphone { "Microphone" } else { "" },
        if p.printer { "Printer" } else { "" },
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(", ");
    let state = match live {
        Some(s) => format!(
            "Connected, {}",
            if s.label().is_empty() { "starting" } else { s.label() }
        ),
        None if p.last_connected.is_empty() => "Never opened".to_owned(),
        None => format!("Last opened {}", relative_date(&p.last_connected, now_ms())),
    };
    let go_message = match live {
        Some(s) => Message::Disconnect(s.id.clone()),
        None => Message::RowPressed(p.id.clone()),
    };
    // No colour of its own: the label takes the button's, which changes with the row's hover.
    let go = button(
        text(if live.is_some() { "Disconnect" } else { "Connect" })
            .size(13)
            .font(bold()),
    )
    .padding([6, 14])
    .style(style::go_hovered(t, live.is_some(), hovered))
    .on_press(go_message);
    let more = button(container(label("\u{2026}", 13.0, t.mute)).center(Fill))
        .width(30)
        .height(30)
        .padding(0)
        .style(style::plain(t))
        .on_press(Message::Edit(p.id.clone()));
    let content = row![
        container(icon(&ICONS.monitor, 18.0, t.mute)).width(24).center_x(24),
        column![
            container(strong(p.name.as_str(), 14.0, t.ink).wrapping(Wrapping::None))
                .width(Fill)
                .clip(true),
            line(extras, 12.0, t.mute, Fill)
        ]
        .width(Length::FillPortion(14)),
        line(p.address.as_str(), 13.0, t.mute, Length::FillPortion(10)),
        line(
            if p.username.is_empty() {
                "Ask at connection"
            } else {
                p.username.as_str()
            },
            13.0,
            t.mute,
            Length::FillPortion(10)
        ),
        line(state, 12.0, if live.is_some() { t.live } else { t.mute }, 170),
        container(row![go, more].spacing(6))
            .width(150)
            .align_x(Horizontal::Right),
    ]
    .spacing(14)
    .height(Fill)
    .align_y(Vertical::Center);
    button(content)
        .width(Fill)
        .height(Length::Fixed(54.0))
        .padding([0, 14])
        .style(style::row(t, false))
        .on_press(Message::RowPressed(p.id.clone()))
        .into()
}
