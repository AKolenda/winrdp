//! Pixel and text drawing shared by the connection bar and modal dialogs.
//!
//! Pixels use softbuffer's `0x00RRGGBB` layout. Rectangle operations require a
//! region inside the buffer. Text uses a system font or the built-in bitmap glyphs.

use ab_glyph::{Font as _, FontVec, PxScale, ScaleFont as _, point};
use winit::dpi::PhysicalSize;

const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
];

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
    let clip_width = (size.width as usize).min(stride);
    let clip_height = (size.height as usize).min(pixels.len().checked_div(stride).unwrap_or(0));
    let Some(font) = font else {
        return bitmap::draw(
            pixels,
            stride,
            (clip_width, clip_height),
            x0,
            top + height.saturating_sub(bitmap::HEIGHT) / 2,
            text,
            color,
        );
    };
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    let text_height = scaled.ascent() - scaled.descent();
    let baseline = top as f32 + (height as f32 - text_height) / 2.0 + scaled.ascent();
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
                    if px >= 0 && py >= 0 && (px as usize) < clip_width && (py as usize) < clip_height {
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
pub(crate) fn blend_rect(
    pixels: &mut [u32],
    stride: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: u32,
    coverage: f32,
) {
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
pub(crate) fn rounded_fill(
    pixels: &mut [u32],
    stride: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    radius: f32,
    color: u32,
) {
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

    pub fn draw(
        pixels: &mut [u32],
        stride: usize,
        clip: (usize, usize),
        x0: usize,
        top: usize,
        text: &str,
        color: u32,
    ) -> usize {
        let mut pen = x0;
        for c in text.chars() {
            for (row, bits) in glyph(c).iter().enumerate() {
                for col in 0..GLYPH_W {
                    if bits & (0b10000 >> col) == 0 {
                        continue;
                    }
                    for dy in 0..SCALE {
                        for dx in 0..SCALE {
                            let x = pen + col * SCALE + dx;
                            let y = top + row * SCALE + dy;
                            if x < clip.0 && y < clip.1 {
                                pixels[y * stride + x] = color;
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

#[cfg(test)]
mod tests {
    use winit::dpi::PhysicalSize;

    use super::{draw_text, text_width};

    #[test]
    fn fallback_glyphs_clip_at_the_right_edge_without_wrapping_to_the_next_row() {
        let size = PhysicalSize::new(8, 16);
        let mut pixels = [0; 8 * 16];
        let advance = draw_text(None, &mut pixels, 8, size, 6, 0, 14, 13.5, "AB", 0xFF_FF_FF, false);

        assert_eq!(advance, text_width(None, 13.5, "AB"));
        for row in pixels.as_chunks::<8>().0 {
            assert_eq!(&row[..6], &[0; 6]);
        }
        assert!(pixels.iter().any(|&pixel| pixel != 0));
    }

    #[test]
    fn fallback_glyphs_clip_to_both_the_window_and_the_backing_buffer() {
        let mut pixels = [0; 8 * 3];
        draw_text(
            None,
            &mut pixels,
            8,
            PhysicalSize::new(4, 20),
            0,
            1,
            14,
            13.5,
            "A",
            0xFF_FF_FF,
            false,
        );

        assert_eq!(&pixels[..8], &[0; 8]);
        for row in pixels.as_chunks::<8>().0 {
            assert_eq!(&row[4..], &[0; 4]);
        }
        assert!(pixels.iter().any(|&pixel| pixel != 0));
    }

    #[test]
    fn an_empty_surface_still_reports_the_text_advance() {
        let advance = draw_text(
            None,
            &mut [],
            0,
            PhysicalSize::new(0, 0),
            0,
            0,
            14,
            13.5,
            "A",
            0xFF_FF_FF,
            false,
        );
        assert_eq!(advance, text_width(None, 13.5, "A"));
    }
}
