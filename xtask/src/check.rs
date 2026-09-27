//! The same source-quality checks used locally and in CI.

use std::path::Path;

use anyhow::{Context, Result};

use crate::util;

pub fn run(root: &Path) -> Result<()> {
    // Select our packages explicitly: --all would also format the IronRDP fork,
    // which has its own workspace, toolchain and formatting rules.
    util::status(util::cargo().current_dir(root).args([
        "fmt",
        "--check",
        "--package",
        "winrdp-next",
        "--package",
        "winrdp-session",
        "--package",
        "xtask",
    ]))
    .context("Formatting check failed; run cargo fmt for the three workspace packages")?;

    // cargo-machete distinguishes a Cargo subcommand from `cargo run` using
    // this variable. Do not let xtask's package name confuse its argument parser.
    util::status(
        util::cargo()
            .current_dir(root)
            .env_remove("CARGO_PKG_NAME")
            .args(["machete", "launcher", "session", "xtask"]),
    )
    .context("Dependency check failed; install the tool with cargo install cargo-machete --version 0.9.2 --locked")?;

    util::status(util::cargo().current_dir(root).args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ]))
    .context("Static analysis failed")?;

    let rustdoc_flags = format!("{} -D warnings", std::env::var("RUSTDOCFLAGS").unwrap_or_default());
    util::status(
        util::cargo()
            .current_dir(root)
            .env("RUSTDOCFLAGS", rustdoc_flags)
            .args([
                "doc",
                "--workspace",
                "--no-deps",
                "--document-private-items",
                "--locked",
            ]),
    )
    .context("Documentation check failed")?;

    util::status(
        util::cargo()
            .current_dir(root)
            .args(["test", "--workspace", "--locked"]),
    )
    .context("Workspace tests failed")?;
    Ok(())
}
