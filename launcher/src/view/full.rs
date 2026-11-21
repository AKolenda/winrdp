// SPDX-License-Identifier: AGPL-3.0-only
//! The full workspace: navigation, session tabs and the computer library.
use iced::alignment::{Horizontal, Vertical};
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Row, Space, button, column, container, hover, mouse_area, responsive, row, svg, text, text_input,
};
use iced::{Fill, Length, Padding, Shrink, padding};

use crate::app::{App, FILTER, MAIN_SCROLL, Message, Page, Session, SessionState, relative_date};
use crate::library::{Layout, Profile};
use crate::style::{self, ICONS, Tokens, icon};

use super::widgets::{caption_button, label, line, pill, scroll, strong};
use super::{Element, now_ms, settings};

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
        caption_button(t, &ICONS.compact, Message::SetLayout(Layout::Simple), false),
        caption_button(t, &ICONS.minimize, Message::Minimize, false),
        caption_button(t, &ICONS.maximize, Message::ToggleMaximize, false),
        caption_button(t, &ICONS.close, Message::CloseRequested, true),
    ]
    .height(40);
    let count = |n: usize| if n == 0 { String::new() } else { n.to_string() };
    // A navigation item: the selected one carries an accent mark at its left edge.
    let nav_item = |glyph: &svg::Handle, caption: &'static str, active: bool, n: usize, message: Message| {
        let mark: Element<'_> = if active {
            container(Space::new().width(3).height(16))
                .style(style::marker(t))
                .into()
        } else {
            Space::new().width(3).height(16).into()
        };
        button(
            row![
                mark,
                icon(glyph, 16.0, t.ink),
                if active {
                    strong(caption, 14.0, t.ink)
                } else {
                    label(caption, 14.0, t.ink)
                },
                Space::new().width(Fill),
                label(count(n), 12.0, t.mute)
            ]
            .spacing(12)
            .height(Fill)
            .align_y(Vertical::Center),
        )
        .width(Fill)
        .height(36)
        .padding(Padding {
            top: 0.0,
            right: 12.0,
            bottom: 0.0,
            left: 0.0,
        })
        .style(style::nav(t, active))
        .on_press(message)
    };
    let page_item = |glyph: &svg::Handle, caption: &'static str, page: Page, n: usize| {
        nav_item(glyph, caption, app.page == page, n, Message::ShowPage(page))
    };
    let sidebar = column![
        row![
            svg(ICONS.logo.clone()).width(20).height(20),
            strong("Win RDP", 14.0, t.ink)
        ]
        .spacing(12)
        .align_y(Vertical::Center)
        .padding(Padding {
            top: 2.0,
            right: 12.0,
            bottom: 18.0,
            left: 15.0
        }),
        page_item(
            &ICONS.monitor,
            "Computers",
            Page::Computers,
            app.library.computers.len()
        ),
        page_item(
            &ICONS.star,
            "Favourites",
            Page::Favourites,
            app.library.computers.iter().filter(|p| p.favorite).count()
        ),
        page_item(&ICONS.play, "Open now", Page::Open, app.sessions.len()),
        Space::new().height(Fill),
        nav_item(
            &ICONS.gear,
            "Settings",
            app.page == Page::Settings,
            0,
            Message::OpenSettings
        ),
        container(label(format!("Win RDP {}", app.version), 12.0, t.mute)).padding(Padding {
            top: 8.0,
            right: 12.0,
            bottom: 2.0,
            left: 15.0
        }),
    ]
    .spacing(4)
    .width(232)
    .padding(Padding {
        top: 8.0,
        right: 8.0,
        bottom: 12.0,
        left: 8.0,
    });

    let title = match app.page {
        Page::Favourites => "Favourites",
        Page::Open => "Open now",
        Page::Computers => "Computers",
        Page::Settings => "Settings",
    };
    let mut page = Column::new().spacing(16).push(strong(title, 28.0, t.ink));
    if app.page == Page::Settings {
        page = page.push(settings::page(app, t));
    } else {
        page = library(app, t, page);
    }
    let main = container(
        scroll(
            t,
            container(page.max_width(1040)).width(Fill).padding(Padding {
                top: 24.0,
                right: 32.0,
                bottom: 28.0,
                left: 32.0,
            }),
        )
        .id(MAIN_SCROLL)
        .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(style::layer(t));
    column![caption, row![sidebar, main].height(Fill)].into()
}
/// The command bar and the computers on the Computers, Favourites and Open now pages.
fn library<'a>(app: &'a App, t: Tokens, mut page: Column<'a, Message>) -> Column<'a, Message> {
    let search = container(
        row![
            icon(&ICONS.search, 16.0, t.mute),
            text_input("Find, or type an address and press Enter", &app.filter)
                .id(FILTER)
                .on_input(Message::FilterInput)
                .on_submit(Message::FilterSubmit)
                .padding(0)
                .size(14)
                .style(style::bare_field(t)),
        ]
        .spacing(10)
        .align_y(Vertical::Center),
    )
    .max_width(380)
    .width(Fill)
    .height(32)
    .padding([0, 10])
    .align_y(Vertical::Center)
    .style(style::card(t, style::CONTROL));
    let new_computer = button(
        row![
            icon(&ICONS.plus, 14.0, t.accent_ink),
            label("New computer", 14.0, t.accent_ink)
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .padding([7, 14])
    .style(style::primary(t))
    .on_press(Message::NewComputer);
    page = page.push(
        row![search, Space::new().width(Fill), new_computer]
            .spacing(12)
            .align_y(Vertical::Center),
    );

    let (open, saved) = app.library_rows();
    let empty = open.is_empty() && saved.is_empty();
    // A caption only where two groups share the page; the page title already names one.
    let both = !open.is_empty() && !saved.is_empty();
    if !open.is_empty() {
        page = page.push(section(app, t, both.then_some("Open now"), open.clone()));
    }
    if !saved.is_empty() {
        page = page.push(section(app, t, both.then_some("Saved"), saved));
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
                    strong(heading, 20.0, t.ink),
                    label(hint, 14.0, t.mute),
                    Space::new().height(4),
                    button(label("New computer", 14.0, t.accent_ink))
                        .padding([7, 16])
                        .style(style::primary(t))
                        .on_press(Message::NewComputer),
                ]
                .spacing(10)
                .align_x(Horizontal::Center),
            )
            .width(Fill)
            .padding([56, 20]),
        );
    }
    page
}
fn section<'a>(app: &'a App, t: Tokens, caption: Option<&'static str>, rows: Vec<&'a Profile>) -> Element<'a> {
    let mut list = Column::new().spacing(4);
    if let Some(caption) = caption {
        list = list.push(container(strong(caption, 14.0, t.ink)).padding(padding::bottom(4)));
    }
    for p in rows {
        list = list.push(hover(library_row(app, t, p, false), library_row(app, t, p, true)));
    }
    list.into()
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
    // What sets this computer apart; the usual window mode goes unsaid.
    let extras = [
        if p.fullscreen { "Full screen" } else { "" },
        if p.microphone { "Microphone" } else { "" },
        if p.printer { "Printer" } else { "" },
    ]
    .into_iter()
    .filter(|s| !s.is_empty());
    let detail = std::iter::once(p.address.as_str())
        .chain((!p.username.is_empty()).then_some(p.username.as_str()))
        .chain(extras)
        .collect::<Vec<_>>()
        .join("  \u{b7}  ");
    let state = match live {
        Some(s) => format!(
            "Connected \u{b7} {}",
            if s.label().is_empty() { "starting" } else { s.label() }
        ),
        None if p.last_connected.is_empty() => "Never opened".to_owned(),
        None => format!("Last opened {}", relative_date(&p.last_connected, now_ms())),
    };
    let go_message = match live {
        Some(s) => Message::Disconnect(s.id.clone()),
        None => Message::RowPressed(p.id.clone()),
    };
    let live = live.is_some();
    // The status steps aside when the window is narrow, so the name keeps room to be read.
    let content = responsive(move |size| {
        // No colour of its own: the label takes the button's, which changes with the row's hover.
        let go = button(text(if live { "Disconnect" } else { "Connect" }).size(14))
            .padding([6, 14])
            .style(style::go_hovered(t, live, hovered))
            .on_press(go_message.clone());
        let edit = button(
            row![icon(&ICONS.pencil, 14.0, t.mute), label("Edit", 13.0, t.mute)]
                .spacing(6)
                .align_y(Vertical::Center),
        )
        .padding([7, 10])
        .style(style::quiet(t))
        .on_press(Message::Edit(p.id.clone()));
        let tile = container(icon(&ICONS.monitor, 18.0, if live { t.live } else { t.ink }))
            .center(36)
            .style(style::tile(t, live.then_some(t.live)));
        let mut name = row![strong(p.name.as_str(), 14.0, t.ink).wrapping(Wrapping::None)]
            .spacing(6)
            .align_y(Vertical::Center);
        if p.favorite {
            name = name.push(icon(&ICONS.star_filled, 12.0, t.mute));
        }
        let mut content = row![
            tile,
            column![
                container(name).width(Fill).clip(true),
                line(detail.clone(), 12.0, t.mute, Fill)
            ]
            .spacing(2)
            .width(Fill),
        ]
        .spacing(16)
        .height(Fill)
        .align_y(Vertical::Center);
        if size.width >= 600.0 {
            content = content.push(line(state.clone(), 12.0, if live { t.live } else { t.mute }, 170));
        }
        content.push(row![go, edit].spacing(4).align_y(Vertical::Center)).into()
    });
    button(content)
        .width(Fill)
        .height(Length::Fixed(64.0))
        .padding([0, 14])
        .style(style::row_card(t))
        .on_press(Message::RowPressed(p.id.clone()))
        .into()
}
