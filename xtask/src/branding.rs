//! Render the logo masters in docs/branding to every icon the app, package and
//! website use.
//!
//! The hand-drawn SVG masters:
//!   logo.svg            full detail, used for 48 px and up
//!   logo-32.svg         pixel-aligned for 24 and 32 px
//!   logo-16.svg         pixel-aligned for 16 px
//!   social-preview.svg  the 1280 x 640 repository and link preview

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use resvg::usvg::fontdb::{Family, Query};
use resvg::{tiny_skia, usvg};

const HICOLOR_SIZES: [u16; 8] = [16, 24, 32, 48, 64, 128, 256, 512];
const FAVICON_SIZES: [u16; 7] = [16, 24, 32, 48, 64, 128, 256];
/// The social preview's typeface; any other face would reflow its lines.
const PREVIEW_FONT: &str = "DejaVu Sans";

pub fn run(root: &Path) -> Result<()> {
    let masters = root.join("docs/branding");
    for copy in ["launcher/assets/winrdp.svg", "website/public/assets/winrdp.svg"] {
        fs::copy(masters.join("logo.svg"), root.join(copy)).with_context(|| format!("Could not write {copy}"))?;
        println!("  {copy}");
    }
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let save = |relative: &str, bytes: &[u8]| {
        fs::write(root.join(relative), bytes).with_context(|| format!("Could not write {relative}"))?;
        println!("  {relative}");
        anyhow::Ok(())
    };

    // The README logo.
    save(
        "docs/branding/app-icon.png",
        &render(&masters.join("logo.svg"), 1024, 1024, &options)?,
    )?;
    // Fixed hicolor sizes for the package and the window icons; small sizes come
    // from the pixel-aligned masters.
    let mut favicon = Vec::new();
    for size in HICOLOR_SIZES {
        let master = if size <= 16 {
            "logo-16.svg"
        } else if size <= 32 {
            "logo-32.svg"
        } else {
            "logo.svg"
        };
        let png = render(&masters.join(master), size, size, &options)?;
        save(&format!("packaging/icons/{size}.png"), &png)?;
        if FAVICON_SIZES.contains(&size) {
            favicon.push((size, png));
        }
    }
    save("website/public/favicon.ico", &ico(&favicon)?)?;

    let family = [Family::Name(PREVIEW_FONT)];
    if options
        .fontdb
        .query(&Query {
            families: &family,
            ..Query::default()
        })
        .is_none()
    {
        bail!("The social preview is set in {PREVIEW_FONT}, which is not installed. Install fonts-dejavu-core.");
    }
    let preview = render(&masters.join("social-preview.svg"), 1280, 640, &options)?;
    save("docs/branding/social-preview.png", &preview)?;
    save("website/public/assets/social-preview.png", &preview)
}

/// Renders `svg` scaled to `width` x `height` and encodes it as PNG.
fn render(svg: &Path, width: u16, height: u16, options: &usvg::Options) -> Result<Vec<u8>> {
    let data = fs::read(svg).with_context(|| format!("Could not read {}", svg.display()))?;
    let tree = usvg::Tree::from_data(&data, options).with_context(|| format!("Could not parse {}", svg.display()))?;
    let mut pixmap = tiny_skia::Pixmap::new(u32::from(width), u32::from(height)).context("An image needs a size")?;
    let size = tree.size();
    let scale = tiny_skia::Transform::from_scale(f32::from(width) / size.width(), f32::from(height) / size.height());
    resvg::render(&tree, scale, &mut pixmap.as_mut());
    // PNG stores straight alpha; the pixmap holds premultiplied colour.
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let colour = pixel.demultiply();
            [colour.red(), colour.green(), colour.blue(), colour.alpha()]
        })
        .collect();
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, u32::from(width), u32::from(height));
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // These files are committed and shipped: worth a slower, smaller encoding.
    encoder.set_compression(png::Compression::High);
    encoder.write_header()?.write_image_data(&rgba)?;
    Ok(png)
}

/// A Windows icon file whose images are stored as PNG, as browsers read them.
fn ico(images: &[(u16, Vec<u8>)]) -> Result<Vec<u8>> {
    const HEADER: u32 = 6;
    const ENTRY: u32 = 16;
    let count = u16::try_from(images.len()).context("Too many images for an ICO directory")?;
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&count.to_le_bytes());
    let mut offset = HEADER + ENTRY * u32::from(count);
    for (size, png) in images {
        if !(1..=256).contains(size) {
            bail!("ICO image dimensions must be between 1 and 256 pixels");
        }
        // Width and height are one byte each; 0 means 256.
        let side = u8::try_from(*size).unwrap_or(0);
        ico.extend_from_slice(&[side, side, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        let length = u32::try_from(png.len()).context("ICO image data exceeds the format's size limit")?;
        ico.extend_from_slice(&length.to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset = offset
            .checked_add(length)
            .context("ICO file exceeds the format's size limit")?;
    }
    for (_, png) in images {
        ico.extend_from_slice(png);
    }
    Ok(ico)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_directory_points_at_each_png() {
        let ico = ico(&[(16, vec![1; 5]), (256, vec![2; 7])]).unwrap();
        assert_eq!(ico[..6], [0, 0, 1, 0, 2, 0]);
        assert_eq!(ico[6..8], [16, 16]);
        assert_eq!(ico[22..24], [0, 0], "256 px is stored as 0");
        assert_eq!(u32::from_le_bytes(ico[18..22].try_into().unwrap()), 38);
        assert_eq!(u32::from_le_bytes(ico[34..38].try_into().unwrap()), 43);
        assert_eq!(ico[38..43], [1; 5]);
        assert_eq!(ico[43..], [2; 7]);
    }

    #[test]
    fn icon_dimensions_cannot_silently_wrap_to_256() {
        assert!(ico(&[(0, vec![])]).is_err());
        assert!(ico(&[(257, vec![])]).is_err());
    }
}
