// SPDX-License-Identifier: AGPL-3.0-only
//! The launcher's look: one token set for light and dark, in the Windows 11 manner.
use std::sync::{LazyLock, OnceLock};

use iced::overlay::menu;
use iced::widget::{button, checkbox, container, pick_list, radio, scrollable, svg, text_input, toggler};
use iced::{Background, Border, Color, Font, Shadow, Theme, border, color, font};

#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub base: Color,
    pub panel: Color,
    pub ink: Color,
    pub mute: Color,
    pub line: Color,
    pub hover: Color,
    pub selected: Color,
    pub accent: Color,
    pub accent_ink: Color,
    pub live: Color,
    pub danger: Color,
    pub field: Color,
    pub field_line: Color,
    pub dark: bool,
}
pub const LIGHT: Tokens = Tokens {
    base: color!(0xF3F4F6),
    panel: color!(0xFFFFFF),
    ink: color!(0x1A1D21),
    mute: color!(0x5E6470),
    line: color!(0xE1E5EB),
    hover: color!(0xEBEEF3),
    selected: color!(0xE3ECF7),
    accent: color!(0x1E5FD8),
    accent_ink: color!(0xFFFFFF),
    live: color!(0x1E9E5A),
    danger: color!(0xC42B1C),
    field: color!(0xFFFFFF),
    field_line: color!(0x8B929D),
    dark: false,
};
pub const DARK: Tokens = Tokens {
    base: color!(0x202124),
    panel: color!(0x2A2C30),
    ink: color!(0xF1F2F4),
    mute: color!(0xA6ACB8),
    line: color!(0x3C4047),
    hover: color!(0x34373D),
    selected: color!(0x2A3D57),
    accent: color!(0x6FB1FF),
    accent_ink: color!(0x0A1F3A),
    live: color!(0x4CCB86),
    danger: color!(0xFF8A80),
    field: color!(0x303338),
    field_line: color!(0x6B7280),
    dark: true,
};
impl Tokens {
    pub fn theme(self) -> Theme {
        Theme::custom(
            if self.dark { "Win RDP dark" } else { "Win RDP" },
            iced::theme::Palette {
                background: self.base,
                text: self.ink,
                primary: self.accent,
                success: self.live,
                warning: color!(0xB7791F),
                danger: self.danger,
            },
        )
    }
}

/// The desktop's interface font (GNOME's `font-name`, else fontconfig's sans-serif),
/// the way the webview's `system-ui` resolved it.
pub fn ui_font() -> Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    *FONT.get_or_init(|| {
        system_font_family().map_or(Font::DEFAULT, |name| Font::with_name(Box::leak(name.into_boxed_str())))
    })
}
fn system_font_family() -> Option<String> {
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    // 'Inter 10' -> Inter; the trailing number is the size.
    let gnome = run("gsettings", &["get", "org.gnome.desktop.interface", "font-name"]).map(|s| {
        let s = s.trim_matches('\'');
        s.rsplit_once(' ')
            .filter(|(_, size)| size.parse::<f32>().is_ok())
            .map_or(s, |(name, _)| name)
            .to_owned()
    });
    gnome
        .filter(|s| !s.is_empty())
        .or_else(|| run("fc-match", &["-f", "%{family[0]}", "sans-serif"]).filter(|s| !s.is_empty()))
}
pub fn bold() -> Font {
    Font {
        weight: font::Weight::Semibold,
        ..ui_font()
    }
}

// ---------- icons ----------
fn stroke_icon(view_box: u32, body: &str) -> svg::Handle {
    svg::Handle::from_memory(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {view_box} {view_box}" fill="none" stroke="#000" stroke-width="1.45" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"##
    ).into_bytes())
}
pub struct Icons {
    pub logo: svg::Handle,
    pub gear: svg::Handle,
    pub minimize: svg::Handle,
    pub maximize: svg::Handle,
    pub close: svg::Handle,
    pub expand: svg::Handle,
    pub compact: svg::Handle,
    pub tab_close: svg::Handle,
    pub chevron_down: svg::Handle,
    pub chevron_right: svg::Handle,
    pub search: svg::Handle,
    pub monitor: svg::Handle,
    pub more: svg::Handle,
    pub add_computer: svg::Handle,
}
pub static ICONS: LazyLock<Icons> = LazyLock::new(|| {
    Icons {
    logo: svg::Handle::from_memory(include_bytes!("../assets/winrdp.svg").as_slice()),
    gear: stroke_icon(16, r#"<circle cx="8" cy="8" r="2.2"/><path d="M8 1.8v1.6M8 12.6v1.6M1.8 8h1.6M12.6 8h1.6M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M3.6 12.4l1.1-1.1M11.3 4.7l1.1-1.1"/>"#),
    minimize: stroke_icon(16, r#"<path d="M3 8h10"/>"#),
    maximize: stroke_icon(16, r#"<rect x="3.5" y="3.5" width="9" height="9"/>"#),
    close: stroke_icon(16, r#"<path d="m4 4 8 8m0-8-8 8"/>"#),
    // Arrows out to the corners: switch to the full window. Arrows in: back to the simple one.
    expand: stroke_icon(16, r#"<path d="M9.5 2.5h4v4m0-4L9 7M6.5 13.5h-4v-4m0 4L7 9"/>"#),
    compact: stroke_icon(16, r#"<path d="M13.5 2.5 9 7m0-3.5V7h3.5M2.5 13.5 7 9m0 3.5V9H3.5"/>"#),
    tab_close: stroke_icon(16, r#"<path d="m4.6 4.6 6.8 6.8m0-6.8-6.8 6.8"/>"#),
    chevron_down: stroke_icon(16, r#"<path d="m4 6 4 4 4-4"/>"#),
    chevron_right: stroke_icon(16, r#"<path d="m6 4 4 4-4 4"/>"#),
    search: stroke_icon(20, r#"<circle cx="8.5" cy="8.5" r="5.5"/><path d="m13 13 4 4"/>"#),
    monitor: stroke_icon(16, r#"<rect x="1.6" y="2.6" width="12.8" height="9" rx="1.4"/><path d="M6 13.7h4"/>"#),
    more: svg::Handle::from_memory(br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="#000"><circle cx="3" cy="8" r="1.1"/><circle cx="8" cy="8" r="1.1"/><circle cx="13" cy="8" r="1.1"/></svg>"##.as_slice()),
    add_computer: stroke_icon(48, r#"<rect x="7" y="9" width="34" height="24" rx="3"/><path d="M24 33v6m-9 0h18m-9-24v12m-6-6h12"/>"#),
}
});
/// A monochrome icon drawn in `tint`.
pub fn icon<'a>(handle: &svg::Handle, size: f32, tint: Color) -> svg::Svg<'a> {
    svg(handle.clone())
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(tint) })
}

// ---------- shared shapes ----------
fn radius(r: f32) -> border::Radius {
    r.into()
}
fn outline(color: Color, width: f32, r: f32) -> Border {
    Border {
        color,
        width,
        radius: radius(r),
    }
}
/// The edge of a surface above the page (dialogs, the dropdown). No blurred shadow: with the
/// tiny-skia renderer a blur paints over its own surface and outside the repainted region.
fn raised_line(t: Tokens) -> Color {
    if t.dark { t.field_line } else { color!(0xC4CAD3) }
}
fn faded(c: Color) -> Color {
    Color { a: c.a * 0.45, ..c }
}
fn darker(c: Color) -> Color {
    Color {
        r: c.r * 0.94,
        g: c.g * 0.94,
        b: c.b * 0.94,
        a: c.a,
    }
}

// ---------- buttons ----------
fn button_base(
    status: button::Status,
    background: Option<Color>,
    hover: Option<Color>,
    text: Color,
    line: Color,
) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let fill = if hovered { hover.or(background) } else { background };
    let style = button::Style {
        background: fill.map(Background::Color),
        text_color: text,
        border: outline(line, if line.a > 0.0 { 1.0 } else { 0.0 }, 6.0),
        shadow: Shadow::default(),
        snap: true,
    };
    if status == button::Status::Disabled {
        return button::Style {
            background: style.background.map(|b| match b {
                Background::Color(c) => Background::Color(faded(c)),
                other => other,
            }),
            text_color: faded(text),
            border: Border {
                color: faded(line),
                ..style.border
            },
            ..style
        };
    }
    style
}
/// The ordinary bordered button.
pub fn plain(t: Tokens) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| button_base(s, Some(t.panel), Some(t.hover), t.ink, t.line)
}
pub fn primary(t: Tokens) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        button_base(
            s,
            Some(t.accent),
            Some(darker(t.accent)),
            t.accent_ink,
            Color::TRANSPARENT,
        )
    }
}
/// Borderless until hovered.
pub fn quiet(t: Tokens) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| button_base(s, None, Some(t.hover), t.ink, Color::TRANSPARENT)
}
pub fn quiet_mute(t: Tokens) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        let ink = if matches!(s, button::Status::Hovered | button::Status::Pressed) {
            t.ink
        } else {
            t.mute
        };
        button_base(s, None, Some(t.hover), ink, Color::TRANSPARENT)
    }
}
pub fn danger_text(t: Tokens) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| button_base(s, None, Some(t.hover), t.danger, Color::TRANSPARENT)
}
pub fn link(t: Tokens) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        let style = button_base(s, None, None, t.accent, Color::TRANSPARENT);
        button::Style {
            border: Border::default(),
            ..style
        }
    }
}
/// Minimize, maximize and close in the caption; close turns red.
pub fn caption(t: Tokens, close: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        let hovered = matches!(s, button::Status::Hovered | button::Status::Pressed);
        let (fill, ink) = match (hovered, close) {
            (true, true) => (Some(color!(0xC42B1C)), Color::WHITE),
            (true, false) => (Some(t.hover), t.ink),
            _ => (None, t.mute),
        };
        button::Style {
            background: fill.map(Background::Color),
            text_color: ink,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}
pub fn nav(t: Tokens, active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        let hovered = matches!(s, button::Status::Hovered | button::Status::Pressed);
        let fill = if active {
            Some(t.panel)
        } else if hovered {
            Some(t.hover)
        } else {
            None
        };
        button::Style {
            background: fill.map(Background::Color),
            text_color: t.ink,
            border: outline(Color::TRANSPARENT, 0.0, 7.0),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}
/// A whole list row that acts as one button.
pub fn row(t: Tokens, selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        let hovered = matches!(s, button::Status::Hovered | button::Status::Pressed);
        let fill = if selected {
            Some(t.selected)
        } else if hovered {
            Some(t.hover)
        } else {
            None
        };
        button::Style {
            background: fill.map(Background::Color),
            text_color: t.ink,
            border: outline(Color::TRANSPARENT, 0.0, 3.0),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}
/// Connect / Disconnect in a list row.
pub fn go(t: Tokens, live: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, s| {
        let hovered = matches!(s, button::Status::Hovered | button::Status::Pressed);
        let tone = if live { t.danger } else { t.accent };
        if hovered {
            button_base(
                s,
                Some(tone),
                None,
                if live { Color::WHITE } else { t.accent_ink },
                Color::TRANSPARENT,
            )
        } else {
            button_base(s, Some(t.panel), None, tone, t.line)
        }
    }
}

/// Connect / Disconnect filled while its row is hovered, as the whole row is one target.
pub fn go_hovered(t: Tokens, live: bool, row_hovered: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, s| go(t, live)(theme, if row_hovered { button::Status::Hovered } else { s })
}

// ---------- containers ----------
pub fn background(t: Tokens) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(t.base.into()),
        text_color: Some(t.ink),
        ..container::Style::default()
    }
}
pub fn card(t: Tokens, r: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(t.panel.into()),
        border: outline(t.line, 1.0, r),
        ..container::Style::default()
    }
}
pub fn inset(t: Tokens) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(t.base.into()),
        border: outline(t.line, 1.0, 8.0),
        ..container::Style::default()
    }
}
pub fn dialog(t: Tokens) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(t.panel.into()),
        text_color: Some(t.ink),
        border: outline(raised_line(t), 1.0, 8.0),
        shadow: Shadow::default(),
        snap: true,
    }
}
pub fn dialog_footer(t: Tokens) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(t.base.into()),
        border: Border {
            color: t.line,
            width: 0.0,
            radius: border::Radius::default().bottom(8.0),
        },
        ..container::Style::default()
    }
}
pub fn backdrop(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgba8(0, 0, 0, 0.4).into()),
        ..container::Style::default()
    }
}
pub fn toast(t: Tokens) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(t.ink.into()),
        text_color: Some(t.base),
        border: outline(Color::TRANSPARENT, 0.0, 8.0),
        shadow: Shadow::default(),
        snap: true,
    }
}
pub fn fill(c: Color, r: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(c.into()),
        border: outline(Color::TRANSPARENT, 0.0, r),
        ..container::Style::default()
    }
}
/// The one-pixel line between rows and panels.
pub fn rule(t: Tokens) -> impl Fn(&Theme) -> container::Style {
    fill(t.line, 0.0)
}
pub fn popup(t: Tokens) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(t.panel.into()),
        border: outline(raised_line(t), 1.0, 6.0),
        ..container::Style::default()
    }
}

/// A thin scroller in the line colour, darker while it is used; no rail.
pub fn scrollbar(t: Tokens) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style {
    move |_, s| {
        let active = matches!(s, scrollable::Status::Dragged { .. })
            || matches!(
                s,
                scrollable::Status::Hovered {
                    is_vertical_scrollbar_hovered: true,
                    ..
                }
            );
        let rail = scrollable::Rail {
            background: None,
            border: Border::default(),
            scroller: scrollable::Scroller {
                background: (if active { t.field_line } else { t.line }).into(),
                border: outline(Color::TRANSPARENT, 0.0, 3.0),
            },
        };
        scrollable::Style {
            container: container::Style::default(),
            vertical_rail: rail,
            horizontal_rail: rail,
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: t.panel.into(),
                border: outline(t.line, 1.0, 99.0),
                shadow: Shadow::default(),
                icon: t.mute,
            },
        }
    }
}

// ---------- inputs ----------
pub fn field(t: Tokens) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, s| text_input::Style {
        background: t.field.into(),
        border: outline(
            if matches!(s, text_input::Status::Focused { .. }) {
                t.accent
            } else {
                t.line
            },
            1.0,
            5.0,
        ),
        icon: t.mute,
        placeholder: t.mute,
        value: t.ink,
        selection: Color { a: 0.35, ..t.accent },
    }
}
/// A drop-down list drawn like a text field.
pub fn pick(t: Tokens) -> impl Fn(&Theme, pick_list::Status) -> pick_list::Style {
    move |_, s| pick_list::Style {
        text_color: t.ink,
        placeholder_color: t.mute,
        handle_color: t.mute,
        background: t.field.into(),
        border: outline(
            if matches!(s, pick_list::Status::Opened { .. }) {
                t.accent
            } else {
                t.line
            },
            1.0,
            5.0,
        ),
    }
}
/// The open list of a [`pick`] drop-down.
pub fn pick_menu(t: Tokens) -> impl Fn(&Theme) -> menu::Style {
    move |_| menu::Style {
        background: t.panel.into(),
        border: outline(t.line, 1.0, 5.0),
        text_color: t.ink,
        selected_text_color: t.ink,
        selected_background: t.selected.into(),
        shadow: Shadow::default(),
    }
}
/// A borderless input inside a drawn frame (the Computer box, the search field).
pub fn bare_field(t: Tokens) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, _| text_input::Style {
        background: Color::TRANSPARENT.into(),
        border: Border::default(),
        icon: t.mute,
        placeholder: t.mute,
        value: t.ink,
        selection: Color { a: 0.35, ..t.accent },
    }
}
pub fn check(t: Tokens) -> impl Fn(&Theme, checkbox::Status) -> checkbox::Style {
    move |_, s| {
        let (checked, hovered) = match s {
            checkbox::Status::Active { is_checked } => (is_checked, false),
            checkbox::Status::Hovered { is_checked } => (is_checked, true),
            checkbox::Status::Disabled { is_checked } => (is_checked, false),
        };
        let fill = if checked {
            if hovered { darker(t.accent) } else { t.accent }
        } else if hovered {
            t.hover
        } else {
            t.field
        };
        checkbox::Style {
            background: fill.into(),
            icon_color: t.accent_ink,
            border: outline(if checked { Color::TRANSPARENT } else { t.field_line }, 1.0, 3.0),
            text_color: Some(t.ink),
        }
    }
}
pub fn choice(t: Tokens) -> impl Fn(&Theme, radio::Status) -> radio::Style {
    move |_, s| {
        let selected = match s {
            radio::Status::Active { is_selected } | radio::Status::Hovered { is_selected } => is_selected,
        };
        radio::Style {
            background: t.field.into(),
            dot_color: t.accent,
            border_width: 1.0,
            border_color: if selected { t.accent } else { t.field_line },
            text_color: Some(t.ink),
        }
    }
}
pub fn switch(t: Tokens) -> impl Fn(&Theme, toggler::Status) -> toggler::Style {
    move |_, s| {
        let on = match s {
            toggler::Status::Active { is_toggled }
            | toggler::Status::Hovered { is_toggled }
            | toggler::Status::Disabled { is_toggled } => is_toggled,
        };
        let disabled = matches!(s, toggler::Status::Disabled { .. });
        let style = toggler::Style {
            background: (if on { t.accent } else { t.field }).into(),
            background_border_width: if on { 0.0 } else { 1.0 },
            background_border_color: t.field_line,
            foreground: (if on { t.accent_ink } else { t.mute }).into(),
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
            text_color: Some(t.ink),
            border_radius: None,
            padding_ratio: 0.2,
        };
        if disabled {
            toggler::Style {
                background: Color {
                    a: 0.45,
                    ..if on { t.accent } else { t.field }
                }
                .into(),
                ..style
            }
        } else {
            style
        }
    }
}
