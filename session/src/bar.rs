//! The connection bar at the top of a full-screen session window, after mstsc's.
//!
//! Pinned, it stays visible. Unpinned, it slides away shortly after the pointer
//! leaves it and comes back when the pointer touches the top edge of the screen.
//! It shows the computer name and the transport, a pin toggle, a restore button
//! that leaves full screen, and a close button that disconnects.
//!
//! The window has no text renderer of its own, so the bar rasterizes a system
//! font with `ab_glyph`, falling back to built-in 5x7 glyphs when none is found.

use std::time::{Duration, Instant};

use ab_glyph::{Font as _, FontVec, PxScale, ScaleFont as _, point};
use winit::dpi::PhysicalSize;

/// Bar height in pixels.
pub const HEIGHT: usize = 32;
const BUTTON: usize = 44;
const TEXT_PAD: usize = 12;
const TEXT_GAP: usize = 10;
/// Corner radius of the bar's bottom corners and of the button hover fills.
const RADIUS: f32 = 8.0;
const BUTTON_RADIUS: f32 = 4.0;
const BUTTON_INSET: usize = 4;
/// Pointer within this many pixels of the top edge reveals an unpinned bar.
const REVEAL_ZONE: f64 = 2.0;
const HIDE_DELAY: Duration = Duration::from_millis(1200);
const TEXT_PX: f32 = 13.5;

// Windows 11 dark title-bar palette.
const BACKGROUND: u32 = 0x00_27_27_27;
const BORDER: u32 = 0x00_3f_3f_3f;
const HOVER: u32 = 0x00_39_39_39;
const CLOSE_HOVER: u32 = 0x00_c4_2b_1c;
const INK: u32 = 0x00_ff_ff_ff;
const MUTED: u32 = 0x00_9d_9d_9d;
const ACCENT: u32 = 0x00_60_cd_ff;

const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

/// What the pointer is over.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Pin,
    Restore,
    Close,
    Body,
}

pub struct ConnectionBar {
    font: Option<FontVec>,
    /// The computer name, as the launcher passed it.
    title: String,
    /// Transport label, `UDP v2` / `TCP`, once the session reports it.
    transport: String,
    pinned: bool,
    /// Whether an unpinned bar is currently out.
    revealed: bool,
    /// When an unpinned bar slides away, if the pointer stays off it.
    hide_at: Option<Instant>,
    hover: Option<Hit>,
}

impl ConnectionBar {
    pub fn new(title: String) -> Self {
        let font = load_font();
        Self {
            font,
            title,
            transport: String::new(),
            pinned: true,
            revealed: true,
            hide_at: None,
            hover: None,
        }
    }

    pub fn set_transport(&mut self, label: &str) {
        self.transport = label.to_owned();
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    /// Whether the pointer is over the bar.
    pub fn hovered(&self) -> bool {
        self.hover.is_some()
    }

    /// Whether the bar is on screen right now.
    pub fn is_shown(&self) -> bool {
        self.pinned || self.revealed
    }

    /// The next moment `tick` has work to do.
    pub fn deadline(&self) -> Option<Instant> {
        self.hide_at
    }

    /// Reset to the pinned, visible state (entering full screen).
    pub fn reset(&mut self) {
        self.revealed = true;
        self.hide_at = None;
        self.hover = None;
    }

    /// Run the hide timer; `true` when the bar changed and needs a redraw.
    pub fn tick(&mut self, now: Instant) -> bool {
        match self.hide_at {
            Some(at) if at <= now => {
                self.hide_at = None;
                if !self.pinned && self.hover.is_none() {
                    self.revealed = false;
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Track the pointer. Returns `(captured, redraw)`: `captured` means the bar
    /// owns the pointer and the remote desktop must not see the movement.
    pub fn pointer_moved(&mut self, size: PhysicalSize<u32>, x: f64, y: f64, now: Instant) -> (bool, bool) {
        let mut redraw = false;
        if !self.is_shown() {
            if y > REVEAL_ZONE {
                return (false, false);
            }
            self.revealed = true;
            redraw = true;
        }
        let hit = self.hit_at(size, x, y);
        if hit != self.hover {
            self.hover = hit;
            redraw = true;
        }
        if hit.is_some() {
            self.hide_at = None;
            return (true, redraw);
        }
        if !self.pinned && self.hide_at.is_none() {
            self.hide_at = Some(now + HIDE_DELAY);
        }
        (false, redraw)
    }

    /// The pointer left the window.
    pub fn pointer_left(&mut self, now: Instant) -> bool {
        let redraw = self.hover.take().is_some();
        if !self.pinned && self.hide_at.is_none() {
            self.hide_at = Some(now + HIDE_DELAY);
        }
        redraw
    }

    /// A left click landed while the bar owns the pointer; what was hit.
    pub fn click(&mut self, now: Instant) -> Option<Hit> {
        let hit = self.hover?;
        if hit == Hit::Pin {
            self.pinned = !self.pinned;
            self.hide_at = (!self.pinned).then(|| now + HIDE_DELAY);
        }
        Some(hit)
    }

    /// `(left, top, width, height)` of the bar in a window of `size`.
    fn rect(&self, size: PhysicalSize<u32>) -> (usize, usize, usize, usize) {
        let window_width = size.width as usize;
        let text = self.measure(&self.title) + if self.transport.is_empty() { 0 } else { TEXT_GAP + self.measure(&self.transport) };
        let width = (BUTTON + TEXT_PAD + text + TEXT_PAD + 2 * BUTTON).min(window_width);
        ((window_width - width) / 2, 0, width, HEIGHT.min(size.height as usize))
    }

    fn hit_at(&self, size: PhysicalSize<u32>, x: f64, y: f64) -> Option<Hit> {
        let (left, top, width, height) = self.rect(size);
        let (left, top, right, bottom) = (left as f64, top as f64, (left + width) as f64, (top + height) as f64);
        if x < left || x >= right || y < top || y >= bottom {
            return None;
        }
        Some(if x < left + BUTTON as f64 {
            Hit::Pin
        } else if x >= right - BUTTON as f64 {
            Hit::Close
        } else if x >= right - 2.0 * BUTTON as f64 {
            Hit::Restore
        } else {
            Hit::Body
        })
    }

    /// Paint the bar over `pixels`, a `size`-sized 0RGB buffer.
    pub fn paint(&self, pixels: &mut [u32], size: PhysicalSize<u32>) {
        let stride = size.width as usize;
        let (left, top, width, height) = self.rect(size);
        if width < 3 * BUTTON || height < 8 {
            return;
        }
        let right = left + width;
        let bottom = top + height;

        // Body: a flat panel hanging from the top edge with anti-aliased rounded
        // bottom corners and a hairline border, like a Windows 11 title bar.
        let (fl, fr, fb) = (left as f32, right as f32, bottom as f32);
        for y in top..bottom {
            for x in left..right {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                // Distance outside the rounded body (0 inside).
                let corner_x = if px < fl + RADIUS {
                    Some(fl + RADIUS)
                } else if px > fr - RADIUS {
                    Some(fr - RADIUS)
                } else {
                    None
                };
                let (body, border) = match corner_x {
                    Some(cx) if py > fb - RADIUS => {
                        let d = ((px - cx).powi(2) + (py - (fb - RADIUS)).powi(2)).sqrt();
                        ((RADIUS - d + 0.5).clamp(0.0, 1.0), (1.0 - (d - (RADIUS - 0.5)).abs()).clamp(0.0, 1.0))
                    }
                    _ => {
                        let edge = x == left || x + 1 == right || y + 1 == bottom;
                        (1.0, if edge { 1.0 } else { 0.0 })
                    }
                };
                let index = y * stride + x;
                pixels[index] = blend(pixels[index], BACKGROUND, body);
                pixels[index] = blend(pixels[index], BORDER, border);
            }
        }

        // Button hover fills: rounded rectangles inset from the button cell.
        let buttons = [
            (Hit::Pin, left, left + BUTTON),
            (Hit::Restore, right - 2 * BUTTON, right - BUTTON),
            (Hit::Close, right - BUTTON, right),
        ];
        for (hit, x0, x1) in buttons {
            if self.hover == Some(hit) {
                let color = if hit == Hit::Close { CLOSE_HOVER } else { HOVER };
                rounded_fill(
                    pixels,
                    stride,
                    x0 + BUTTON_INSET,
                    top + BUTTON_INSET,
                    x1 - BUTTON_INSET,
                    bottom - BUTTON_INSET,
                    BUTTON_RADIUS,
                    color,
                );
            }
        }

        let cy = top + height / 2;
        self.draw_pin(pixels, stride, left + BUTTON / 2, cy);
        self.draw_restore(pixels, stride, right - BUTTON - BUTTON / 2, cy);
        self.draw_close(pixels, stride, right - BUTTON / 2, cy);

        // Text: title, then the transport in colour.
        let mut x = left + BUTTON + TEXT_PAD;
        x += self.draw_text(pixels, stride, size, x, top, height, &self.title, INK);
        if !self.transport.is_empty() {
            x += TEXT_GAP;
            let color = if self.transport.starts_with("UDP") { ACCENT } else { MUTED };
            self.draw_text(pixels, stride, size, x, top, height, &self.transport, color);
        }
    }

    fn draw_pin(&self, pixels: &mut [u32], stride: usize, cx: usize, cy: usize) {
        let color = if self.pinned { INK } else { MUTED };
        if self.pinned {
            // Upright: wide head, short neck, needle pointing down.
            fill(pixels, stride, cx - 4, cy - 8, 9, 5, color);
            fill(pixels, stride, cx - 2, cy - 3, 5, 3, color);
            fill(pixels, stride, cx - 6, cy, 13, 1, color);
            fill(pixels, stride, cx, cy + 1, 1, 6, color);
        } else {
            // On its side: needle pointing left, head to the right.
            fill(pixels, stride, cx - 8, cy, 6, 1, color);
            fill(pixels, stride, cx - 2, cy - 6, 1, 13, color);
            fill(pixels, stride, cx - 1, cy - 2, 3, 5, color);
            fill(pixels, stride, cx + 2, cy - 4, 5, 9, color);
        }
    }

    fn draw_restore(&self, pixels: &mut [u32], stride: usize, cx: usize, cy: usize) {
        // Two overlapping windows: the back one up-right, the front one down-left.
        let under = pixels[cy * stride + cx];
        outline(pixels, stride, cx - 2, cy - 6, 9, 9, INK);
        fill(pixels, stride, cx - 5, cy - 3, 9, 9, under);
        outline(pixels, stride, cx - 5, cy - 3, 9, 9, INK);
    }

    fn draw_close(&self, pixels: &mut [u32], stride: usize, cx: usize, cy: usize) {
        for i in 0..10usize {
            let (x, y) = (cx - 5 + i, cy - 5 + i);
            pixels[y * stride + x] = INK;
            pixels[y * stride + (cx + 4 - i)] = INK;
        }
    }

    /// Advance width of `text` in pixels.
    fn measure(&self, text: &str) -> usize {
        text_width(self.font.as_ref(), TEXT_PX, text)
    }

    /// Draw `text` vertically centred in the bar; returns its advance.
    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &self,
        pixels: &mut [u32],
        stride: usize,
        size: PhysicalSize<u32>,
        x0: usize,
        top: usize,
        height: usize,
        text: &str,
        color: u32,
    ) -> usize {
        draw_text(self.font.as_ref(), pixels, stride, size, x0, top, height, TEXT_PX, text, color, false)
    }
}

/// The system font the bar and the dialogs rasterize with, if one is installed.
pub(crate) fn load_font() -> Option<FontVec> {
    let font = FONT_CANDIDATES
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .and_then(|bytes| FontVec::try_from_vec(bytes).ok());
    if font.is_none() {
        tracing::warn!("No system font found; using the built-in glyphs");
    }
    font
}

/// Advance width of `text` at `px` pixels.
pub(crate) fn text_width(font: Option<&FontVec>, px: f32, text: &str) -> usize {
    match font {
        Some(font) => {
            let scaled = font.as_scaled(PxScale::from(px));
            let mut width = 0.0;
            let mut prev = None;
            for ch in text.chars() {
                let id = scaled.glyph_id(ch);
                if let Some(prev) = prev {
                    width += scaled.kern(prev, id);
                }
                width += scaled.h_advance(id);
                prev = Some(id);
            }
            width.ceil() as usize
        }
        None => text.chars().count() * bitmap::ADVANCE,
    }
}

/// Draw `text` vertically centred in a `height`-tall band starting at `top`;
/// returns its advance. `bold` draws a second pass offset by a fraction of a
/// pixel, since only a regular weight is loaded.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text(
    font: Option<&FontVec>,
    pixels: &mut [u32],
    stride: usize,
    size: PhysicalSize<u32>,
    x0: usize,
    top: usize,
    height: usize,
    px: f32,
    text: &str,
    color: u32,
    bold: bool,
) -> usize {
    let Some(font) = font else {
        return bitmap::draw(pixels, stride, x0, top + height.saturating_sub(bitmap::HEIGHT) / 2, text, color);
    };
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    let text_height = scaled.ascent() - scaled.descent();
    let baseline = top as f32 + (height as f32 - text_height) / 2.0 + scaled.ascent();
    let (max_x, max_y) = (size.width as i32, size.height as i32);
    let passes: &[f32] = if bold { &[0.0, 0.7] } else { &[0.0] };
    let mut advance = 0.0f32;
    for offset in passes {
        let mut x = x0 as f32 + offset;
        let mut prev = None;
        for ch in text.chars() {
            let id = scaled.glyph_id(ch);
            if let Some(prev) = prev {
                x += scaled.kern(prev, id);
            }
            let glyph = id.with_scale_and_position(scale, point(x, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    if px >= 0 && py >= 0 && px < max_x && py < max_y {
                        let index = py as usize * stride + px as usize;
                        pixels[index] = blend(pixels[index], color, coverage);
                    }
                });
            }
            x += scaled.h_advance(id);
            prev = Some(id);
        }
        advance = x - x0 as f32 - offset;
    }
    advance.ceil() as usize
}

/// Mix `color` over the rectangle at `coverage` (0..=1).
#[allow(clippy::too_many_arguments)]
pub(crate) fn blend_rect(pixels: &mut [u32], stride: usize, x: usize, y: usize, w: usize, h: usize, color: u32, coverage: f32) {
    for yy in y..y + h {
        for xx in x..x + w {
            let index = yy * stride + xx;
            pixels[index] = blend(pixels[index], color, coverage);
        }
    }
}

pub(crate) fn fill(pixels: &mut [u32], stride: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    for yy in y..y + h {
        for xx in x..x + w {
            pixels[yy * stride + xx] = color;
        }
    }
}

/// Fill the rectangle `[x0, x1) x [y0, y1)` with corners rounded by `radius`, anti-aliased.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rounded_fill(pixels: &mut [u32], stride: usize, x0: usize, y0: usize, x1: usize, y1: usize, radius: f32, color: u32) {
    let (l, t, r, b) = (x0 as f32, y0 as f32, x1 as f32, y1 as f32);
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Signed distance to the rounded rectangle.
            let cx = px.clamp(l + radius, r - radius);
            let cy = py.clamp(t + radius, b - radius);
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            let coverage = (radius - d + 0.5).clamp(0.0, 1.0);
            let index = y * stride + x;
            pixels[index] = blend(pixels[index], color, coverage);
        }
    }
}

pub(crate) fn outline(pixels: &mut [u32], stride: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    fill(pixels, stride, x, y, w, 1, color);
    fill(pixels, stride, x, y + h - 1, w, 1, color);
    fill(pixels, stride, x, y, 1, h, color);
    fill(pixels, stride, x + w - 1, y, 1, h, color);
}

/// Mix `fg` over `bg` by `coverage` (0..=1) per channel.
pub(crate) fn blend(bg: u32, fg: u32, coverage: f32) -> u32 {
    let mix = |shift: u32| {
        let b = ((bg >> shift) & 0xff) as f32;
        let f = ((fg >> shift) & 0xff) as f32;
        ((b + (f - b) * coverage).round() as u32) << shift
    };
    mix(16) | mix(8) | mix(0)
}

/// Built-in 5x7 capitals and digits, used only when no system font is available.
mod bitmap {
    const SCALE: usize = 2;
    const GLYPH_W: usize = 5;
    const GLYPH_H: usize = 7;
    pub const ADVANCE: usize = (GLYPH_W + 1) * SCALE;
    pub const HEIGHT: usize = GLYPH_H * SCALE;

    fn glyph(c: char) -> [u8; GLYPH_H] {
        match c.to_ascii_uppercase() {
            'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
            'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
            'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
            'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
            'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
            'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
            'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
            'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
            'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
            'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
            'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
            'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
            'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
            'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
            'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
            'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
            'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
            'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
            'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
            'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
            'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
            'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
            'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
            'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
            'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
            'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
            '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
            '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
            '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
            '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
            '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
            '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
            '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
            '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
            '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
            '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
            '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110],
            '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
            _ => [0; GLYPH_H],
        }
    }

    pub fn draw(pixels: &mut [u32], stride: usize, x0: usize, top: usize, text: &str, color: u32) -> usize {
        let mut pen = x0;
        for c in text.chars() {
            for (row, bits) in glyph(c).iter().enumerate() {
                for col in 0..GLYPH_W {
                    if bits & (0b10000 >> col) == 0 {
                        continue;
                    }
                    for dy in 0..SCALE {
                        for dx in 0..SCALE {
                            let index = (top + row * SCALE + dy) * stride + pen + col * SCALE + dx;
                            if index < pixels.len() {
                                pixels[index] = color;
                            }
                        }
                    }
                }
            }
            pen += ADVANCE;
        }
        pen - x0
    }
}
