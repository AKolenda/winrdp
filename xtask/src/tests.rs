//! Repository checks that need no build: the packaging metadata, the packager's
//! refusals, and a Rust-only tree. None of them compiles or exercises RDP.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::process::Command;

use crate::util::{self, TempDir};
use crate::{package, util::root};

fn read(relative: &str) -> String {
    fs::read_to_string(root().join(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"))
}

#[test]
fn desktop_file_keeps_a_separate_identity() {
    let desktop = read("packaging/io.winrdp.Next.desktop");
    assert!(desktop.contains("Exec=winrdp-next\n"));
    assert!(desktop.contains("Keywords=RDP;"));
    assert!(!desktop.contains("Exec=winrdp\n"));
    // The package installs its icons under this name.
    assert!(desktop.contains("Icon=winrdp-next\n"));
}

#[test]
fn metainfo_launches_the_desktop_file() {
    let text = read("packaging/io.winrdp.Next.metainfo.xml");
    let document = roxmltree::Document::parse(&text).unwrap();
    let component = document.root_element();
    let child = |name| {
        component
            .children()
            .find(|node| node.has_tag_name(name))
            .and_then(|node| node.text())
    };
    assert_eq!(child("id"), Some("io.winrdp.Next"));
    assert_eq!(child("launchable"), Some("io.winrdp.Next.desktop"));
}

#[test]
fn no_package_from_a_file_that_is_not_a_program() {
    let temp = TempDir::new("winrdp-xtask-test").unwrap();
    let binary = temp.path().join("binary");
    fs::write(&binary, "not an executable").unwrap();
    let out = temp.path().join("dist");
    assert!(package::package(&root(), &binary, &binary, &out).is_err());
    assert!(!out.exists());
}

#[test]
fn no_package_without_the_binaries() {
    let temp = TempDir::new("winrdp-xtask-test").unwrap();
    let missing = temp.path().join("missing");
    let out = temp.path().join("out");
    assert!(package::package(&root(), &missing, &missing, &out).is_err());
    assert!(!out.exists());
}

#[test]
fn scratch_directories_are_private_and_removed_when_finished() {
    let path = {
        let scratch = TempDir::new("winrdp-xtask-private").unwrap();
        let path = scratch.path().to_path_buf();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o700);
        fs::write(path.join("partial-output"), "temporary").unwrap();
        path
    };
    assert!(!path.exists());
}

#[test]
fn rust_only() {
    // Every session runs in winrdp-session (IronRDP); nothing links a C/C++ RDP engine.
    let tracked = util::output(Command::new("git").arg("ls-files").current_dir(root())).unwrap();
    let native: Vec<_> = tracked
        .lines()
        .filter(|file| {
            [".c", ".cc", ".cpp", ".h", ".hpp"]
                .iter()
                .any(|extension| file.ends_with(extension))
        })
        .collect();
    assert_eq!(native, Vec::<&str>::new());
    for member in ["launcher", "session", "xtask"] {
        let build_script = root().join(member).join("build.rs");
        if build_script.exists() {
            assert!(
                !fs::read_to_string(build_script).unwrap().contains("rustc-link-lib"),
                "{member}"
            );
        }
    }
    for manifest in ["Cargo.lock", "launcher/Cargo.toml", "session/Cargo.toml"] {
        let text = read(manifest).to_lowercase();
        assert!(!text.contains("freerdp") && !text.contains("qt6"), "{manifest}");
    }
}

#[test]
fn source_files_are_present() {
    for name in [
        "launcher/src/main.rs",
        "launcher/src/endpoint.rs",
        "session/src/main.rs",
        "BUILDING.md",
        "PROVENANCE.md",
    ] {
        assert!(fs::metadata(root().join(name)).unwrap().len() > 100, "{name}");
    }
}
