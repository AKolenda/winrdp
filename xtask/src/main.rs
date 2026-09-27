//! Win RDP's build, packaging and release tasks: `cargo xtask <command>`.
//!
//! Nothing here changes an installed Win RDP, an RDP host service or the firewall;
//! `install-deps` is the only command that asks for sudo, and only for apt.
// Test assertions should fail immediately when a fixture cannot be constructed.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::panic))]

mod branding;
mod check;
mod deb;
mod deps;
mod notices;
mod package;
mod preflight;
mod release;
#[cfg(test)]
mod tests;
mod util;
mod website;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};

const USAGE: &str = "\
Usage: cargo xtask <command> [options]

Commands:
  check                      Check formatting, dependencies, lints, Rust docs and tests
                             (requires cargo-machete 0.9.2; see BUILDING.md)
  preflight                  Check this host for the build tools and libraries
  deb                        Preflight, test, build both binaries and package them in dist/
                             (WINRDP_JOBS sets the build jobs, default 4; logs in logs/)
  package                    Package binaries that are already built
      --launcher <path>      winrdp-next (default: the release target directory)
      --session <path>       winrdp-session (default: the release target directory)
      --out <dir>            Where the .deb goes (default: dist)
  check-release              Check that every version field and install command agrees
      --tag <vX.Y.Z>         Also require this release tag
  notices                    Regenerate docs/rust-dependency-notices.json and the licence texts
      --check                Only verify them against Cargo.lock and the files on disk
  branding                   Render the logo masters in docs/branding to every icon copy
  website                    Check website/public and copy it to website/dist
  install-deps               Install the Debian/Ubuntu build dependencies with apt
      --yes                  Do not ask before installing (for CI)
";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    match run(&command, Args(args.collect())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: &str, mut args: Args) -> Result<()> {
    let root = util::root();
    match command {
        "check" => {
            args.finish()?;
            check::run(&root)
        }
        "preflight" => {
            args.finish()?;
            preflight::run(&root)
        }
        "deb" => {
            args.finish()?;
            deb::run(&root)
        }
        "package" => {
            let launcher = args.value("--launcher")?.map(PathBuf::from);
            let session = args.value("--session")?.map(PathBuf::from);
            let out = args.value("--out")?.map_or_else(|| root.join("dist"), PathBuf::from);
            args.finish()?;
            let release = util::target_dir(&root)?.join("release");
            package::package(
                &root,
                &launcher.unwrap_or_else(|| release.join("winrdp-next")),
                &session.unwrap_or_else(|| release.join("winrdp-session")),
                &out,
            )
            .map(drop)
        }
        "check-release" => {
            let tag = args.value("--tag")?;
            args.finish()?;
            release::run(&root, tag.as_deref())
        }
        "notices" => {
            let check = args.flag("--check");
            args.finish()?;
            if check {
                notices::check(&root)
            } else {
                notices::generate(&root)
            }
        }
        "branding" => {
            args.finish()?;
            branding::run(&root)
        }
        "website" => {
            args.finish()?;
            website::run(&root)
        }
        "install-deps" => {
            let yes = args.flag("--yes");
            args.finish()?;
            deps::run(yes)
        }
        "" | "help" | "-h" | "--help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => bail!("Unknown command: {other}\n\n{USAGE}"),
    }
}

/// The options after the command name, removed as each is recognised.
struct Args(Vec<String>);

impl Args {
    fn flag(&mut self, name: &str) -> bool {
        let found = self.0.iter().position(|arg| arg == name);
        found.map(|index| self.0.remove(index)).is_some()
    }

    fn value(&mut self, name: &str) -> Result<Option<String>> {
        let Some(index) = self.0.iter().position(|arg| arg == name) else {
            return Ok(None);
        };
        if index + 1 == self.0.len() {
            bail!("{name} needs a value");
        }
        let value = self.0.remove(index + 1);
        self.0.remove(index);
        Ok(Some(value))
    }

    fn finish(self) -> Result<()> {
        match self.0.first() {
            Some(extra) => bail!("Unexpected argument: {extra}\n\n{USAGE}"),
            None => Ok(()),
        }
    }
}
