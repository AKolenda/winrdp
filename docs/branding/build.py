#!/usr/bin/env python3
"""Render the Win RDP logo masters to every icon the app, package and website use.

Hand-drawn SVG masters in this directory:
  logo.svg            full detail, used for 48 px and up
  logo-32.svg         pixel-aligned for 24 and 32 px
  logo-16.svg         pixel-aligned for 16 px
  social-preview.svg  the 1280 x 640 repository and link preview

Edit the masters, then run: python3 docs/branding/build.py
Needs librsvg and pycairo (GObject bindings) and Pillow.
"""
import shutil
from pathlib import Path

import cairo
import gi

gi.require_version("Rsvg", "2.0")
from gi.repository import Rsvg  # noqa: E402
from PIL import Image  # noqa: E402

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
HICOLOR_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]


def master_for(size: int) -> Path:
    if size <= 16:
        return HERE / "logo-16.svg"
    if size <= 32:
        return HERE / "logo-32.svg"
    return HERE / "logo.svg"


def render(svg: Path, width: int, height: int, out: Path) -> None:
    handle = Rsvg.Handle.new_from_file(str(svg))
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, width, height)
    viewport = Rsvg.Rectangle()
    viewport.x, viewport.y, viewport.width, viewport.height = 0, 0, width, height
    handle.render_document(cairo.Context(surface), viewport)
    out.parent.mkdir(parents=True, exist_ok=True)
    surface.write_to_png(str(out))
    print(f"  {out.relative_to(REPO)}")


def main() -> None:
    for dest in ("frontend/assets/winrdp.svg", "website/public/assets/winrdp.svg"):
        shutil.copyfile(HERE / "logo.svg", REPO / dest)
        print(f"  {dest}")
    # Tauri embeds this as the launcher window icon; the session window embeds it too.
    render(HERE / "logo.svg", 1024, 1024, REPO / "src-tauri/icons/icon.png")
    shutil.copyfile(REPO / "src-tauri/icons/icon.png", HERE / "app-icon.png")
    print(f"  {(HERE / 'app-icon.png').relative_to(REPO)}")
    # Fixed hicolor sizes for the .deb; small sizes come from the pixel-aligned masters.
    for size in HICOLOR_SIZES:
        render(master_for(size), size, size, REPO / f"packaging/icons/{size}.png")
    frames = [Image.open(REPO / f"packaging/icons/{size}.png").convert("RGBA") for size in ICO_SIZES]
    ico = REPO / "website/public/favicon.ico"
    frames[-1].save(ico, format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=frames[:-1])
    print(f"  {ico.relative_to(REPO)}")
    render(HERE / "social-preview.svg", 1280, 640, HERE / "social-preview.png")
    shutil.copyfile(HERE / "social-preview.png", REPO / "website/public/assets/social-preview.png")
    print("  website/public/assets/social-preview.png")


if __name__ == "__main__":
    main()
