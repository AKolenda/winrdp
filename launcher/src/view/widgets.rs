// SPDX-License-Identifier: AGPL-3.0-only
//! Shared controls, typography and dialog framing for the launcher.
use iced::alignment::{Horizontal, Vertical};
use iced::widget::text::Wrapping;
use iced::widget::{Column, Space, button, checkbox, column, container, hover, row, scrollable, svg, text, text_input};
use iced::{Color, Fill, Length, Padding};

use crate::app::{ConnectionOption, ConnectionOptions, Message};
use crate::style::{self, Tokens, bold, icon};

use super::Element;

pub(super) fn label<'a>(content: impl text::IntoFragment<'a>, size: f32, color: Color) -> text::Text<'a> {
    text(content).size(size).color(color)
}
pub(super) fn strong<'a>(content: impl text::IntoFragment<'a>, size: f32, color: Color) -> text::Text<'a> {
    label(content, size, color).font(bold())
}
/// One line that never wraps; overflow is cut at the column edge.
pub(super) fn line<'a>(
    content: impl text::IntoFragment<'a>,
    size: f32,
    color: Color,
    width: impl Into<Length>,
) -> Element<'a> {
    container(label(content, size, color).wrapping(Wrapping::None))
        .width(width)
        .clip(true)
        .into()
}
/// Vertical scrolling with a thin scroller beside the content rather than over it.
pub(super) fn scroll<'a>(t: Tokens, content: impl Into<Element<'a>>) -> scrollable::Scrollable<'a, Message> {
    scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(6).scroller_width(6).margin(2),
        ))
        .spacing(2)
        .style(style::scrollbar(t))
}
pub(super) fn hrule<'a>(t: Tokens) -> Element<'a> {
    container(Space::new())
        .width(Fill)
        .height(1)
        .style(style::rule(t))
        .into()
}
pub(super) fn vrule<'a>(t: Tokens) -> Element<'a> {
    container(Space::new())
        .width(1)
        .height(Fill)
        .style(style::rule(t))
        .into()
}
pub(super) fn pill<'a>(t: Tokens, content: String, udp: bool) -> Element<'a> {
    let (bg, ink) = if udp {
        (t.accent, t.accent_ink)
    } else {
        (t.line, t.mute)
    };
    container(strong(content, 10.5, ink))
        .padding([1, 7])
        .style(style::fill(bg, 99.0))
        .into()
}

/// A button that swaps in a second rendering while hovered, so its icon can change colour too.
pub(super) fn caption_button<'a>(t: Tokens, handle: &svg::Handle, message: Message, close: bool) -> Element<'a> {
    let make = |tint: Color| {
        button(container(icon(handle, 16.0, tint)).center(Fill))
            .width(44)
            .height(Fill)
            .padding(0)
            .style(style::caption(t, close))
            .on_press(message.clone())
    };
    hover(make(t.mute), make(if close { Color::WHITE } else { t.ink }))
}
pub(super) fn check_row<'a>(
    t: Tokens,
    text_label: &'a str,
    checked: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a> {
    checkbox(checked)
        .label(text_label)
        .on_toggle(on_toggle)
        .size(16)
        .spacing(9)
        .text_size(13)
        .style(style::check(t))
        .into()
}

/// The same connection controls appear in the compact window and the computer editor.
/// Returning individual rows lets each form keep its own spacing and other fields.
pub(super) fn connection_options<'a>(
    tokens: Tokens,
    options: ConnectionOptions,
    on_change: fn(ConnectionOption, bool) -> Message,
) -> [Element<'a>; 5] {
    [
        ("Open full screen", ConnectionOption::Fullscreen, options.fullscreen),
        ("Share the clipboard", ConnectionOption::Clipboard, options.clipboard),
        ("Play remote audio here", ConnectionOption::Audio, options.audio),
        (
            "Send my microphone to the remote computer",
            ConnectionOption::Microphone,
            options.microphone,
        ),
        (
            "Offer my printer to the remote computer",
            ConnectionOption::Printer,
            options.printer,
        ),
    ]
    .map(|(label, option, enabled)| check_row(tokens, label, enabled, move |value| on_change(option, value)))
}
pub(super) fn input<'a>(
    t: Tokens,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .padding([8, 10])
        .size(13)
        .style(style::field(t))
}
pub(super) fn field_label<'a>(t: Tokens, caption: &'a str, field: impl Into<Element<'a>>) -> Element<'a> {
    column![label(caption, 12.0, t.mute), field.into()].spacing(5).into()
}

pub(super) fn dialog_frame<'a>(
    t: Tokens,
    width: f32,
    heading: Option<&'a str>,
    body: Element<'a>,
    footer: Element<'a>,
) -> Element<'a> {
    let mut content = Column::new();
    if let Some(title) = heading {
        content = content.push(
            row![
                strong(title, 15.0, t.ink).width(Fill),
                button(container(label("\u{00D7}", 18.0, t.mute)).center(Fill))
                    .width(30)
                    .height(30)
                    .padding(0)
                    .style(style::quiet_mute(t))
                    .on_press(Message::CloseDialog)
            ]
            .align_y(Vertical::Center)
            .padding(Padding {
                top: 16.0,
                right: 16.0,
                bottom: 0.0,
                left: 22.0,
            }),
        );
    }
    content = content.push(body).push(hrule(t)).push(
        container(footer)
            .width(Fill)
            .padding([12, 22])
            .style(style::dialog_footer(t)),
    );
    container(content).max_width(width).style(style::dialog(t)).into()
}
pub(super) fn footer_button<'a>(t: Tokens, caption: &'a str, primary: bool, message: Option<Message>) -> Element<'a> {
    let ink = if primary { t.accent_ink } else { t.ink };
    let text = if primary {
        strong(caption, 13.0, ink)
    } else {
        label(caption, 13.0, ink)
    };
    // At least 96 px wide, like the Windows dialog buttons, and wider when the label needs it.
    button(column![Space::new().width(64), text].align_x(Horizontal::Center))
        .padding([7, 16])
        .style(move |theme, status| {
            if primary {
                style::primary(t)(theme, status)
            } else {
                style::plain(t)(theme, status)
            }
        })
        .on_press_maybe(message)
        .into()
}
