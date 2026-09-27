//! Repainting only what changed.
//!
//! Copying and presenting the whole desktop for every graphics update costs 15 MB per
//! update at 2560x1440 even when only a caret blinked. The window instead copies the
//! changed area into the softbuffer buffer and presents just that, which is only sound
//! while the buffer still holds what was presented before: [`History`] tracks what recent
//! presents changed so an older buffer can be brought up to date, and says when it cannot.

use std::collections::VecDeque;

use ironrdp::pdu::geometry::InclusiveRectangle;
use smallvec::{SmallVec, smallvec};
use winit::dpi::PhysicalSize;

/// A rectangle of window pixels, `[x, x + width) x [y, y + height)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// The whole of a window of `size`.
    pub fn covering(size: PhysicalSize<u32>) -> Self {
        Self {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        }
    }

    /// The part of `self` inside a window of `size`, if any.
    pub fn clip(self, size: PhysicalSize<u32>) -> Option<Self> {
        let right = self.x.saturating_add(self.width).min(size.width);
        let bottom = self.y.saturating_add(self.height).min(size.height);
        (self.x < right && self.y < bottom).then(|| Self {
            x: self.x,
            y: self.y,
            width: right - self.x,
            height: bottom - self.y,
        })
    }

    fn covers(self, size: PhysicalSize<u32>) -> bool {
        self.x == 0 && self.y == 0 && self.width >= size.width && self.height >= size.height
    }

    pub fn to_softbuffer(self) -> Option<softbuffer::Rect> {
        Some(softbuffer::Rect {
            x: self.x,
            y: self.y,
            width: self.width.try_into().ok()?,
            height: self.height.try_into().ok()?,
        })
    }
}

impl From<&InclusiveRectangle> for Rect {
    /// The frame is drawn 1:1 from the window's top-left corner, so frame and window
    /// coordinates are the same.
    fn from(rect: &InclusiveRectangle) -> Self {
        Self {
            x: u32::from(rect.left),
            y: u32::from(rect.top),
            width: (u32::from(rect.right) + 1).saturating_sub(u32::from(rect.left)),
            height: (u32::from(rect.bottom) + 1).saturating_sub(u32::from(rect.top)),
        }
    }
}

/// A handful of rectangles; overlapping ones are fine, just copied twice.
pub type Damage = SmallVec<[Rect; 4]>;

/// Everything a buffer's contents depend on besides the pixels: a buffer drawn for another
/// window size or another desktop size has to be repainted whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub window: PhysicalSize<u32>,
    pub desktop: (u16, u16),
}

/// What one draw copies from the frame into the buffer, and what it presents.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub copy: Damage,
    pub present: Damage,
}

/// What the last few presents changed, newest first.
#[derive(Debug, Default)]
pub struct History {
    presents: VecDeque<(Layout, Damage)>,
}

impl History {
    /// Oldest buffer age worth tracking: softbuffer uses one buffer on X11 and two on Wayland.
    const DEPTH: usize = 3;

    /// Plans a draw into a buffer of softbuffer `age` for `layout`.
    ///
    /// `changed` is what differs from the last present: the frame's dirty area and every
    /// overlay painted now or last time. `everything` presents the whole window even so,
    /// for repaints the window system asked for: X11 reports an exposed window only as a
    /// redraw request, indistinguishable from the window's own.
    pub fn plan(&self, age: u8, layout: Layout, changed: &[Rect], everything: bool) -> Plan {
        let whole = Rect::covering(layout.window);
        let Some(stale) = self.stale(age, layout) else {
            return Plan {
                copy: smallvec![whole],
                present: smallvec![whole],
            };
        };
        let copy = stale.into_iter().chain(changed.iter().copied()).collect();
        let present = if everything {
            smallvec![whole]
        } else {
            changed.iter().copied().collect()
        };
        Plan {
            copy: simplify(copy, layout.window),
            present: simplify(present, layout.window),
        }
    }

    /// Remembers what a present changed.
    pub fn record(&mut self, layout: Layout, presented: Damage) {
        self.presents.push_front((layout, presented));
        self.presents.truncate(Self::DEPTH);
    }

    /// The areas a buffer of `age` is missing, or `None` when its contents cannot be used.
    ///
    /// A buffer of age `n` shows what the `n`th most recent present left on screen, so it
    /// lacks what the `n - 1` presents since then changed. Age 0 is a new buffer.
    fn stale(&self, age: u8, layout: Layout) -> Option<Damage> {
        let age = usize::from(age);
        if age == 0 || age > self.presents.len() {
            return None;
        }
        // Softbuffer keeps a Wayland buffer's age when it resizes the buffer, so compare
        // layouts rather than trusting the age alone.
        if self
            .presents
            .iter()
            .take(age)
            .any(|(presented, _)| *presented != layout)
        {
            return None;
        }
        Some(
            self.presents
                .iter()
                .take(age - 1)
                .flat_map(|(_, damage)| damage.iter().copied())
                .collect(),
        )
    }
}

/// One whole-window rectangle instead of a list that already contains it.
fn simplify(damage: Damage, window: PhysicalSize<u32>) -> Damage {
    if damage.iter().any(|rect| rect.covers(window)) {
        smallvec![Rect::covering(window)]
    } else {
        damage
    }
}

/// Copies the `rect` part of a `window_width`-wide buffer from `frame`, which is drawn
/// 1:1 from the top-left corner; what lies beyond the frame is black. That is also what
/// shows while a resize is pending and the window and the desktop disagree in size.
///
/// `rect` must lie within the buffer, and `frame` must hold `frame_size.0 * frame_size.1`
/// pixels.
pub fn blit(buffer: &mut [u32], window_width: usize, frame: &[u32], frame_size: (usize, usize), rect: Rect) {
    let (frame_width, frame_height) = frame_size;
    let (left, right) = (rect.x as usize, rect.x as usize + rect.width as usize);
    // Columns `left..left + from_frame` come from the frame, the rest are black.
    let from_frame = right.min(frame_width).saturating_sub(left);
    for y in rect.y as usize..rect.y as usize + rect.height as usize {
        let row = &mut buffer[y * window_width + left..y * window_width + right];
        if y < frame_height && from_frame != 0 {
            let source = y * frame_width + left;
            row[..from_frame].copy_from_slice(&frame[source..source + from_frame]);
            row[from_frame..].fill(0);
        } else {
            row.fill(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use winit::dpi::PhysicalSize;

    use super::{Damage, History, Layout, Plan, Rect, blit};

    fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect { x, y, width, height }
    }

    fn layout(width: u32, height: u32) -> Layout {
        Layout {
            window: PhysicalSize::new(width, height),
            desktop: (1280, 720),
        }
    }

    fn damage(rects: &[Rect]) -> Damage {
        rects.iter().copied().collect()
    }

    #[test]
    fn a_new_buffer_is_repainted_and_presented_whole() {
        let history = History::default();
        let plan = history.plan(0, layout(1280, 720), &[rect(10, 10, 5, 5)], false);
        assert_eq!(plan.copy.as_slice(), &[rect(0, 0, 1280, 720)]);
        assert_eq!(plan.present.as_slice(), &[rect(0, 0, 1280, 720)]);
    }

    #[test]
    fn the_buffer_just_presented_needs_only_what_changed() {
        let mut history = History::default();
        history.record(layout(1280, 720), damage(&[rect(0, 0, 1280, 720)]));
        let plan = history.plan(1, layout(1280, 720), &[rect(10, 10, 5, 5)], false);
        assert_eq!(
            plan,
            Plan {
                copy: damage(&[rect(10, 10, 5, 5)]),
                present: damage(&[rect(10, 10, 5, 5)]),
            }
        );
    }

    #[test]
    fn a_double_buffered_surface_also_catches_up_on_what_the_other_buffer_showed() {
        let mut history = History::default();
        history.record(layout(1280, 720), damage(&[rect(0, 0, 1280, 720)]));
        history.record(layout(1280, 720), damage(&[rect(100, 100, 10, 10)]));
        let plan = history.plan(2, layout(1280, 720), &[rect(10, 10, 5, 5)], false);
        assert_eq!(plan.copy.as_slice(), &[rect(100, 100, 10, 10), rect(10, 10, 5, 5)]);
        assert_eq!(plan.present.as_slice(), &[rect(10, 10, 5, 5)]);
    }

    #[test]
    fn a_buffer_older_than_the_history_is_repainted_whole() {
        let mut history = History::default();
        history.record(layout(1280, 720), damage(&[rect(0, 0, 1280, 720)]));
        let plan = history.plan(2, layout(1280, 720), &[rect(10, 10, 5, 5)], false);
        assert_eq!(plan.copy.as_slice(), &[rect(0, 0, 1280, 720)]);
    }

    #[test]
    fn a_buffer_last_presented_at_another_size_is_repainted_whole_whatever_its_age() {
        let mut history = History::default();
        history.record(layout(1280, 720), damage(&[rect(0, 0, 1280, 720)]));
        history.record(layout(1600, 900), damage(&[rect(0, 0, 1600, 900)]));
        let plan = history.plan(2, layout(1600, 900), &[], false);
        assert_eq!(plan.copy.as_slice(), &[rect(0, 0, 1600, 900)]);

        // The desktop changing size under an unchanged window counts too.
        let mut history = History::default();
        history.record(layout(1280, 720), damage(&[rect(0, 0, 1280, 720)]));
        let resized = Layout {
            desktop: (1024, 768),
            ..layout(1280, 720)
        };
        assert_eq!(
            history.plan(1, resized, &[], false).copy.as_slice(),
            &[rect(0, 0, 1280, 720)]
        );
    }

    #[test]
    fn a_requested_repaint_presents_everything_but_copies_only_what_changed() {
        let mut history = History::default();
        history.record(layout(1280, 720), damage(&[rect(0, 0, 1280, 720)]));
        let plan = history.plan(1, layout(1280, 720), &[rect(500, 0, 300, 32)], true);
        assert_eq!(plan.copy.as_slice(), &[rect(500, 0, 300, 32)]);
        assert_eq!(plan.present.as_slice(), &[rect(0, 0, 1280, 720)]);
    }

    #[test]
    fn a_list_containing_the_whole_window_collapses_to_it() {
        let mut history = History::default();
        history.record(layout(1280, 720), damage(&[rect(0, 0, 1280, 720)]));
        let plan = history.plan(
            1,
            layout(1280, 720),
            &[rect(10, 10, 5, 5), rect(0, 0, 1280, 720)],
            false,
        );
        assert_eq!(plan.copy.as_slice(), &[rect(0, 0, 1280, 720)]);
        assert_eq!(plan.present.as_slice(), &[rect(0, 0, 1280, 720)]);
    }

    #[test]
    fn rectangles_are_clipped_to_the_window() {
        let window = PhysicalSize::new(100, 50);
        assert_eq!(rect(90, 40, 20, 20).clip(window), Some(rect(90, 40, 10, 10)));
        assert_eq!(rect(100, 0, 5, 5).clip(window), None);
        assert_eq!(rect(0, 0, 0, 5).clip(window), None);
        let inclusive = ironrdp::pdu::geometry::InclusiveRectangle {
            left: 3,
            top: 4,
            right: 3,
            bottom: 9,
        };
        assert_eq!(Rect::from(&inclusive), rect(3, 4, 1, 6));
    }

    #[test]
    fn blit_copies_the_rectangle_and_blacks_out_what_lies_beyond_the_frame() {
        // A 3x2 frame in a 4x3 window.
        let frame = [1, 2, 3, 4, 5, 6];
        let mut buffer = [9; 12];
        blit(&mut buffer, 4, &frame, (3, 2), rect(1, 0, 3, 3));
        assert_eq!(buffer, [9, 2, 3, 0, 9, 5, 6, 0, 9, 0, 0, 0]);

        // Only the rectangle is touched.
        let mut buffer = [9; 12];
        blit(&mut buffer, 4, &frame, (3, 2), rect(0, 1, 2, 1));
        assert_eq!(buffer, [9, 9, 9, 9, 4, 5, 9, 9, 9, 9, 9, 9]);

        // A window smaller than the frame clips it.
        let mut buffer = [9; 2];
        blit(&mut buffer, 2, &frame, (3, 2), rect(0, 0, 2, 1));
        assert_eq!(buffer, [1, 2]);
    }

    #[test]
    fn blit_clears_damage_entirely_inside_the_right_padding() {
        let frame = [1, 2, 3, 4, 5, 6];
        let mut buffer = [9; 15];
        blit(&mut buffer, 5, &frame, (3, 2), rect(4, 0, 1, 3));
        assert_eq!(buffer, [9, 9, 9, 9, 0, 9, 9, 9, 9, 0, 9, 9, 9, 9, 0]);
    }

    #[test]
    fn an_old_overlay_is_removed_when_a_buffer_returns() {
        let layout = Layout {
            window: PhysicalSize::new(4, 1),
            desktop: (4, 1),
        };
        let desktop = [1, 2, 3, 4];
        let overlay = rect(1, 0, 2, 1);
        let mut history = History::default();
        history.record(layout, damage(&[Rect::covering(layout.window)]));

        // The first buffer still contains the overlay; the second removed it.
        let mut first_buffer = [1, 9, 9, 4];
        history.record(layout, damage(&[overlay]));
        let plan = history.plan(2, layout, &[], false);
        for rect in plan.copy {
            blit(&mut first_buffer, 4, &desktop, (4, 1), rect);
        }
        assert_eq!(first_buffer, desktop);
        assert!(plan.present.is_empty());
    }
}
