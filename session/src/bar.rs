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

use ab_glyph::FontVec;
use winit::dpi::PhysicalSize;

use crate::drawing::{blend, draw_text, fill, load_font, outline, rounded_fill, text_width};

/// Bar height in pixels.
pub const HEIGHT: usize = 32;
/// The pin icon extends eight pixels above its vertical centre.
const MIN_HEIGHT: usize = 16;
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

    /// `(left, top, width, height)` of the bar in a window of `size`: all that `paint` draws on.
    pub fn rect(&self, size: PhysicalSize<u32>) -> (usize, usize, usize, usize) {
        let window_width = size.width as usize;
        let text = self.measure(&self.title)
            + if self.transport.is_empty() {
                0
            } else {
                TEXT_GAP + self.measure(&self.transport)
            };
        let width = (BUTTON + TEXT_PAD + text + TEXT_PAD + 2 * BUTTON).min(window_width);
        ((window_width - width) / 2, 0, width, HEIGHT.min(size.height as usize))
    }

    fn hit_at(&self, size: PhysicalSize<u32>, x: f64, y: f64) -> Option<Hit> {
        let (left, top, width, height) = self.rect(size);
        if width < 3 * BUTTON || height < MIN_HEIGHT {
            return None;
        }
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
        if width < 3 * BUTTON || height < MIN_HEIGHT {
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
                        (
                            (RADIUS - d + 0.5).clamp(0.0, 1.0),
                            (1.0 - (d - (RADIUS - 0.5)).abs()).clamp(0.0, 1.0),
                        )
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
            let color = if self.transport.starts_with("UDP") {
                ACCENT
            } else {
                MUTED
            };
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
        draw_text(
            self.font.as_ref(),
            pixels,
            stride,
            size,
            x0,
            top,
            height,
            TEXT_PX,
            text,
            color,
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use winit::dpi::PhysicalSize;

    use super::{BUTTON, ConnectionBar, Hit, MIN_HEIGHT};

    #[test]
    fn small_windows_never_draw_or_activate_controls_that_do_not_fit() {
        let mut bar = ConnectionBar::new("Office PC".to_owned());
        bar.font = None;
        bar.hover = Some(Hit::Pin);

        for width in [0, 1, 131, 132, 200] {
            for height in [0, 1, 7, 8, 15, 16, 31, 32] {
                let size = PhysicalSize::new(width, height);
                let mut pixels = vec![0; width as usize * height as usize];
                bar.paint(&mut pixels, size);

                if (width as usize) < 3 * BUTTON || (height as usize) < MIN_HEIGHT {
                    assert!(pixels.iter().all(|&pixel| pixel == 0), "painted into {size:?}");
                    assert_eq!(bar.hit_at(size, 1.0, 1.0), None);
                } else {
                    assert!(pixels.iter().any(|&pixel| pixel != 0), "did not paint into {size:?}");
                    assert_eq!(bar.hit_at(size, 1.0, 1.0), Some(Hit::Pin));
                }
            }
        }
    }
}
