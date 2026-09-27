//! Check version consistency without compiling or accessing credentials.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::{package, util};

pub fn run(root: &Path, tag: Option<&str>) -> Result<()> {
    let metadata = util::metadata(root, &["--no-deps"])?;
    let mut versions = BTreeMap::new();
    for (name, manifest) in [
        ("winrdp-next", "launcher/Cargo.toml"),
        ("winrdp-session", "session/Cargo.toml"),
    ] {
        let package = metadata.packages.iter().find(|package| package.name == name);
        let package = package.with_context(|| format!("The workspace has no {name} package"))?;
        versions.insert(manifest, package.version.clone());
    }
    versions.insert("xtask package VERSION", package::VERSION.to_owned());
    versions.insert("AppStream", appstream_release(root)?);
    let version = &versions["session/Cargo.toml"];
    if versions.values().any(|other| other != version) {
        let listed: Vec<_> = versions
            .iter()
            .map(|(place, version)| format!("{place}: {version}"))
            .collect();
        bail!("Release versions disagree:\n- {}", listed.join("\n- "));
    }

    // Text a reader acts on: an install command or a download link naming the wrong
    // release is as broken as a mismatched manifest, and nothing else checks these.
    let mentions = [
        ("README.md", format!("winrdp-next_{version}_amd64.deb")),
        ("BUILDING.md", format!("winrdp-next_{version}_amd64.deb")),
        (
            "docs/RELEASE-CHECKLIST.md",
            format!("cargo xtask check-release --tag v{version}"),
        ),
        ("docs/RELEASE-NOTES.md", format!("Win RDP {version}")),
    ];
    let mut missing = Vec::new();
    for (path, text) in mentions {
        let contents = fs::read_to_string(root.join(path)).with_context(|| format!("Could not read {path}"))?;
        if !contents.contains(&text) {
            missing.push(format!("{path} does not mention \"{text}\""));
        }
    }
    if !missing.is_empty() {
        bail!(
            "Release {version} is not reflected everywhere:\n- {}",
            missing.join("\n- ")
        );
    }

    if tag.is_some_and(|tag| tag != format!("v{version}")) {
        bail!("Release tag must be v{version}");
    }
    println!("Release metadata agrees: {version}");
    Ok(())
}

/// The newest release in the AppStream metadata, which software centres show.
fn appstream_release(root: &Path) -> Result<String> {
    let path = root.join("packaging/io.winrdp.Next.metainfo.xml");
    let text = fs::read_to_string(&path).with_context(|| format!("Could not read {}", path.display()))?;
    let document = roxmltree::Document::parse(&text).with_context(|| format!("{} is not valid XML", path.display()))?;
    let release = document
        .root_element()
        .children()
        .find(|node| node.has_tag_name("releases"))
        .and_then(|releases| releases.children().find(|node| node.has_tag_name("release")))
        .and_then(|release| release.attribute("version"));
    release
        .map(str::to_owned)
        .with_context(|| format!("{} has no <releases><release version>", path.display()))
}
