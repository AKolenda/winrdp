//! Check this host without installing packages or changing anything.

use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};

use crate::util;

/// Compilers for the C code some crates build (aws-lc, opus), and the dpkg tools
/// the packaging step runs.
const TOOLS: &[&str] = &[
    "cargo",
    "rustc",
    "cc",
    "cmake",
    "pkg-config",
    "dpkg",
    "dpkg-deb",
    "dpkg-query",
    "dpkg-shlibdeps",
    "ldd",
    "ldconfig",
];

/// ALSA and zlib are linked. The rest are opened at run time, and the packaging
/// step can only declare a dependency on a library that is installed here.
const LIBRARIES: &[&str] = &[
    "alsa",
    "zlib",
    "wayland-client",
    "xkbcommon",
    "xkbcommon-x11",
    "x11",
    "x11-xcb",
    "xcb",
    "xcursor",
    "xi",
];

const SOURCES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "launcher/Cargo.toml",
    "session/Cargo.toml",
    "third_party/ironrdp/crates/ironrdp/Cargo.toml",
];

pub fn run(root: &Path) -> Result<()> {
    println!("Win RDP build preflight");
    let mut errors = Vec::new();
    if std::env::consts::OS != "linux" {
        errors.push("Build on Linux, on the oldest distribution the package should install on.".to_owned());
    }
    for tool in TOOLS {
        if util::which(tool).is_none() {
            errors.push(format!("Missing tool: {tool}"));
        }
    }
    if util::which("pkg-config").is_some() {
        for name in LIBRARIES {
            match util::output(Command::new("pkg-config").args(["--modversion", name])) {
                Ok(version) => println!("  {name}: {}", version.trim()),
                Err(_) => errors.push(format!("Missing development package: {name}")),
            }
        }
    }
    for relative in SOURCES {
        if !root.join(relative).is_file() {
            errors.push(format!("Source file missing: {relative}"));
        }
    }
    if !errors.is_empty() {
        bail!(
            "\nBuild has not started:\n  {}\n\nRun cargo xtask install-deps, and git submodule update --init --recursive \
             for the IronRDP fork (see BUILDING.md).",
            errors.join("\n  ")
        );
    }
    println!("\nPreflight passed. Compilation and live testing have NOT run yet.");
    Ok(())
}
