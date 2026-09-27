//! Install the Debian/Ubuntu packages the build and the packaging step need.

use std::process::Command;

use anyhow::{Result, bail};

use crate::util;

/// A C toolchain and CMake for the aws-lc and opus sources Rust crates compile,
/// pkg-config and the ALSA and zlib headers for what the binaries link, dpkg-dev for
/// dpkg-shlibdeps, and curl for `cargo xtask notices`. The X11, Wayland and
/// xkbcommon libraries are never linked, winit opens them at run time, but the
/// package can only declare the ones installed here. DejaVu Sans is the session
/// bar's fallback font and the social preview's typeface. Nothing needs GTK or
/// WebKitGTK any more.
const PACKAGES: &[&str] = &[
    "build-essential",
    "cmake",
    "pkg-config",
    "curl",
    "ca-certificates",
    "dpkg-dev",
    "libasound2-dev",
    "zlib1g-dev",
    "libwayland-dev",
    "libxkbcommon-dev",
    "libxkbcommon-x11-dev",
    "libx11-dev",
    "libx11-xcb-dev",
    "libxcb1-dev",
    "libxcursor-dev",
    "libxi-dev",
    "fonts-dejavu-core",
];

pub fn run(yes: bool) -> Result<()> {
    if util::is_root()? {
        bail!("Run as your normal user; this command calls sudo only for apt.");
    }
    if util::which("apt-get").is_none() {
        bail!("This helper targets Zorin, Ubuntu and Debian, which use apt-get.");
    }
    println!(
        "This installs build tools and the ALSA, zlib, X11 and Wayland development packages from your configured repositories."
    );
    println!("It does not change graphics drivers, RDP host services or firewall rules.");
    util::status(Command::new("sudo").args(["apt-get", "update"]))?;
    let mut install = Command::new("sudo");
    install.args(["apt-get", "install"]);
    if yes {
        install.arg("--yes");
    }
    util::status(install.args(PACKAGES))?;
    println!("Rust itself comes from rustup; see BUILDING.md.");
    Ok(())
}
