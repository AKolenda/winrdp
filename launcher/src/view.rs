// SPDX-License-Identifier: AGPL-3.0-only
//! Composes the launcher window, dialog layers, notifications and resize edges.
mod dialogs;
mod full;
mod settings;
mod simple;
mod widgets;

use iced::alignment::{Horizontal, Vertical};
use iced::mouse::Interaction;
use iced::widget::{Space, button, column, container, mouse_area, opaque, row, stack};
use iced::{Color, Fill, Length, Padding, window};

use crate::app::{App, Dialog, Message};
use crate::library::Layout;
use crate::style;

use widgets::label;

type Element<'a> = iced::Element<'a, Message>;

pub fn view(app: &App) -> Element<'_> {
    let t = app.tokens();
    let base = match app.layout {
        Layout::Simple => simple::view(app, t),
        Layout::Full => full::view(app, t),
    };
    let mut layers: Vec<Element<'_>> = vec![
        container(base)
            .width(Fill)
            .height(Fill)
            .style(style::background(t))
            .into(),
    ];
    if app.layout == Layout::Simple && app.combo_open && app.dialogs.is_empty() {
        let items = app.combo_items();
        if !items.is_empty() {
            layers.push(simple::combo_list(t, items));
        }
    }
    if app.layout == Layout::Full && undecorated() {
        layers.push(resize_edges());
    }
    for dialog in &app.dialogs {
        let body = match dialog {
            Dialog::Computer(form) => dialogs::computer(t, form),
            Dialog::Password(form) => dialogs::password(t, form),
            Dialog::Confirm(confirm) => dialogs::confirm(t, confirm),
            Dialog::Settings => settings::view(app, t),
        };
        layers.push(opaque(container(body).center(Fill).padding(12).style(style::backdrop)));
    }
    if let Some(toast) = &app.toast {
        let dismiss = button(label("Dismiss", 13.0, t.base))
            .padding([4, 10])
            .on_press(Message::DismissToast)
            .style(move |_, s| button::Style {
                background: matches!(s, button::Status::Hovered).then(|| Color { a: 0.12, ..t.base }.into()),
                text_color: t.base,
                border: iced::Border {
                    color: Color { a: 0.2, ..t.base },
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..button::Style::default()
            });
        // The text takes what the button leaves, so a long message wraps instead of pushing Dismiss out.
        let card = container(
            row![label(toast.text.as_str(), 13.0, t.base).width(Fill), dismiss]
                .spacing(12)
                .align_y(Vertical::Center),
        )
        .padding(Padding {
            top: 10.0,
            right: 12.0,
            bottom: 10.0,
            left: 16.0,
        })
        .max_width(560)
        .style(style::toast(t));
        layers.push(
            container(card)
                .width(Fill)
                .height(Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Bottom)
                .padding(16)
                .into(),
        );
    }
    stack(layers).width(Fill).height(Fill).into()
}
/// The system frame is off unless WINRDP_SYSTEM_FRAME is set; then the window draws its own edges.
pub fn undecorated() -> bool {
    std::env::var_os("WINRDP_SYSTEM_FRAME").is_none()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Thin edges and corners that resize the undecorated full window.
fn resize_edges<'a>() -> Element<'a> {
    use window::Direction as D;
    let edge = |w: Length, h: Length, direction: D, cursor: Interaction| -> Element<'a> {
        mouse_area(Space::new().width(w).height(h))
            .on_press(Message::Resize(direction))
            .interaction(cursor)
            .into()
    };
    let (n, c) = (Length::Fixed(4.0), Length::Fixed(8.0));
    column![
        row![
            edge(c, c, D::NorthWest, Interaction::ResizingDiagonallyDown),
            edge(Fill, n, D::North, Interaction::ResizingVertically),
            edge(c, c, D::NorthEast, Interaction::ResizingDiagonallyUp)
        ]
        .height(c),
        row![
            edge(n, Fill, D::West, Interaction::ResizingHorizontally),
            Space::new().width(Fill).height(Fill),
            edge(n, Fill, D::East, Interaction::ResizingHorizontally)
        ]
        .height(Fill),
        row![
            edge(c, c, D::SouthWest, Interaction::ResizingDiagonallyUp),
            column![
                Space::new().height(4),
                edge(Fill, n, D::South, Interaction::ResizingVertically)
            ],
            edge(c, c, D::SouthEast, Interaction::ResizingDiagonallyDown)
        ]
        .height(c),
    ]
    .into()
}
