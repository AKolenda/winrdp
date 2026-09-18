//! The "close this connection?" dialog, drawn over the desktop in the
//! Windows 11 message-box style: a light panel with the question, a grey
//! footer strip, and Disconnect / Cancel buttons. Enter disconnects, Escape
//! cancels, and the remote desktop receives no input while it is up.

use ab_glyph::FontVec;
use winit::dpi::PhysicalSize;

use crate::bar::{blend_rect, draw_text, fill, load_font, outline, rounded_fill, text_width};

/// What the user chose.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Disconnect,
    Cancel,
}

const WIDTH: usize = 440;
const BODY_HEIGHT: usize = 118;
const FOOTER_HEIGHT: usize = 62;
const PAD: usize = 22;
const BUTTON_W: usize = 118;
const BUTTON_H: usize = 32;
const BUTTON_GAP: usize = 8;
const RADIUS: f32 = 8.0;
const TITLE_PX: f32 = 14.5;
const TEXT_PX: f32 = 13.5;

// Windows 11 light dialog palette.
const DIM: u32 = 0x00_00_00_00;
const PANEL: u32 = 0x00_f9_f9_f9;
const FOOTER: u32 = 0x00_f0_f0_f0;
const BORDER: u32 = 0x00_d6_d6_d6;
const INK: u32 = 0x00_1b_1b_1b;
const MUTED: u32 = 0x00_5e_5e_5e;
const ACCENT: u32 = 0x00_00_67_c0;
const ACCENT_HOVER: u32 = 0x00_19_75_c5;
const BUTTON: u32 = 0x00_fb_fb_fb;
const BUTTON_HOVER: u32 = 0x00_f2_f2_f2;
const BUTTON_BORDER: u32 = 0x00_d1_d1_d1;

pub struct CloseDialog {
    font: Option<FontVec>,
    /// The computer name, for "Disconnect from Office PC?".
    computer: String,
    hover: Option<Choice>,
}

impl CloseDialog {
    pub fn new(computer: String) -> Self {
        Self {
            font: load_font(),
            computer,
            hover: None,
        }
    }

    /// Top-left corner of the panel, centred in `size`.
    fn origin(size: PhysicalSize<u32>) -> (usize, usize) {
        let (w, h) = (size.width as usize, size.height as usize);
        (w.saturating_sub(WIDTH) / 2, h.saturating_sub(BODY_HEIGHT + FOOTER_HEIGHT) / 2)
    }

    /// Button rectangles as (choice, x0, y0, x1, y1).
    fn buttons(size: PhysicalSize<u32>) -> [(Choice, usize, usize, usize, usize); 2] {
        let (left, top) = Self::origin(size);
        let y0 = top + BODY_HEIGHT + (FOOTER_HEIGHT - BUTTON_H) / 2;
        let cancel_x1 = left + WIDTH - PAD;
        let cancel_x0 = cancel_x1 - BUTTON_W;
        let ok_x1 = cancel_x0 - BUTTON_GAP;
        let ok_x0 = ok_x1 - BUTTON_W;
        [
            (Choice::Disconnect, ok_x0, y0, ok_x1, y0 + BUTTON_H),
            (Choice::Cancel, cancel_x0, y0, cancel_x1, y0 + BUTTON_H),
        ]
    }

    /// Whether the window is big enough to draw the panel in.
    ///
    /// Paint and hit-testing share this so a window too small to show the dialog
    /// never enters a modal state it cannot render: the buttons would still be
    /// live at their unclamped coordinates while nothing was on screen.
    pub fn fits(size: PhysicalSize<u32>) -> bool {
        size.width as usize >= WIDTH + 8 && size.height as usize >= BODY_HEIGHT + FOOTER_HEIGHT + 8
    }

    fn hit(size: PhysicalSize<u32>, x: f64, y: f64) -> Option<Choice> {
        if !Self::fits(size) {
            return None;
        }
        let (x, y) = (x.max(0.0) as usize, y.max(0.0) as usize);
        Self::buttons(size)
            .into_iter()
            .find(|(_, x0, y0, x1, y1)| x >= *x0 && x < *x1 && y >= *y0 && y < *y1)
            .map(|(choice, ..)| choice)
    }

    /// Returns whether the dialog needs repainting.
    pub fn pointer_moved(&mut self, size: PhysicalSize<u32>, x: f64, y: f64) -> bool {
        let hit = Self::hit(size, x, y);
        let changed = hit != self.hover;
        self.hover = hit;
        changed
    }

    pub fn click(&self, size: PhysicalSize<u32>, x: f64, y: f64) -> Option<Choice> {
        Self::hit(size, x, y)
    }

    pub fn paint(&self, pixels: &mut [u32], size: PhysicalSize<u32>) {
        let stride = size.width as usize;
        let (w, h) = (size.width as usize, size.height as usize);
        if !Self::fits(size) {
            return;
        }
        // Dim the desktop so the dialog reads as modal.
        blend_rect(pixels, stride, 0, 0, w, h, DIM, 0.42);

        let (left, top) = Self::origin(size);
        let (right, bottom) = (left + WIDTH, top + BODY_HEIGHT + FOOTER_HEIGHT);
        rounded_fill(pixels, stride, left, top, right, bottom, RADIUS, PANEL);
        fill(pixels, stride, left, top + BODY_HEIGHT, WIDTH, 1, BORDER);
        // Footer strip with rounded bottom corners (the fill's top corners are hidden under the body).
        rounded_fill(pixels, stride, left, top + BODY_HEIGHT + 1, right, bottom, RADIUS, FOOTER);
        fill(pixels, stride, left, top + BODY_HEIGHT + 1, WIDTH, RADIUS as usize, FOOTER);
        outline(pixels, stride, left, top, WIDTH, BODY_HEIGHT + FOOTER_HEIGHT, BORDER);

        let font = self.font.as_ref();
        let title = format!("Disconnect from {}?", self.computer);
        draw_text(font, pixels, stride, size, left + PAD, top + 18, 22, TITLE_PX, &title, INK, true);
        draw_text(
            font,
            pixels,
            stride,
            size,
            left + PAD,
            top + 52,
            20,
            TEXT_PX,
            "The remote session stays signed in on the computer.",
            MUTED,
            false,
        );
        draw_text(
            font,
            pixels,
            stride,
            size,
            left + PAD,
            top + 74,
            20,
            TEXT_PX,
            "You can connect again to pick up where you left off.",
            MUTED,
            false,
        );

        for (choice, x0, y0, x1, y1) in Self::buttons(size) {
            let hovered = self.hover == Some(choice);
            let (color, ink) = match choice {
                Choice::Disconnect => (if hovered { ACCENT_HOVER } else { ACCENT }, 0x00_ff_ff_ff),
                Choice::Cancel => (if hovered { BUTTON_HOVER } else { BUTTON }, INK),
            };
            rounded_fill(pixels, stride, x0, y0, x1, y1, 4.0, color);
            if choice == Choice::Cancel {
                outline(pixels, stride, x0, y0, x1 - x0, y1 - y0, BUTTON_BORDER);
            }
            let label = match choice {
                Choice::Disconnect => "Disconnect",
                Choice::Cancel => "Cancel",
            };
            let tw = text_width(font, TEXT_PX, label);
            let tx = x0 + (x1 - x0).saturating_sub(tw) / 2;
            draw_text(font, pixels, stride, size, tx, y0, y1 - y0, TEXT_PX, label, ink, choice == Choice::Disconnect);
        }
    }
}

#[cfg(test)]
mod tests {
    use winit::dpi::PhysicalSize;

    use super::{Choice, CloseDialog};

    fn size(width: u32, height: u32) -> PhysicalSize<u32> {
        PhysicalSize::new(width, height)
    }

    #[test]
    fn a_window_too_small_to_draw_the_panel_has_no_live_buttons() {
        let small = size(300, 400);
        assert!(!CloseDialog::fits(small));
        let dialog = CloseDialog::new("Office PC".to_owned());
        // Anywhere in the window, including where the unclamped buttons would be.
        for (x, y) in [(0.0, 0.0), (150.0, 200.0), (299.0, 399.0)] {
            assert_eq!(dialog.click(small, x, y), None);
        }
    }

    #[test]
    fn both_buttons_answer_where_they_are_drawn() {
        let full = size(1280, 720);
        assert!(CloseDialog::fits(full));
        let dialog = CloseDialog::new("Office PC".to_owned());
        let mut seen = Vec::new();
        for (choice, x0, y0, x1, y1) in CloseDialog::buttons(full) {
            let (cx, cy) = (((x0 + x1) / 2) as f64, ((y0 + y1) / 2) as f64);
            assert_eq!(dialog.click(full, cx, cy), Some(choice));
            // Just outside is not a hit.
            assert_eq!(dialog.click(full, (x1 + 1) as f64, cy), None);
            seen.push(choice);
        }
        assert!(seen.contains(&Choice::Disconnect) && seen.contains(&Choice::Cancel));
    }

    #[test]
    fn hovering_a_button_asks_for_one_repaint_not_one_per_motion() {
        let full = size(1280, 720);
        let mut dialog = CloseDialog::new("Office PC".to_owned());
        let (_, x0, y0, x1, y1) = CloseDialog::buttons(full)[0];
        let (cx, cy) = (((x0 + x1) / 2) as f64, ((y0 + y1) / 2) as f64);
        assert!(dialog.pointer_moved(full, cx, cy), "entering the button repaints");
        assert!(!dialog.pointer_moved(full, cx + 1.0, cy), "staying on it does not");
        assert!(dialog.pointer_moved(full, 1.0, 1.0), "leaving it repaints");
    }
}
