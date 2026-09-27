//! Check the static site and copy it to website/dist, the directory Wrangler deploys.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::util;

const LATEST_RELEASE: &str = "https://github.com/AKolenda/winrdp/releases/latest";

pub fn run(root: &Path) -> Result<()> {
    let site = root.join("website");
    let index = site.join("public/index.html");
    let html = fs::read_to_string(&index).with_context(|| format!("Could not read {}", index.display()))?;
    check(&html)?;
    // Only public/ is deployed: never publish source, credentials, or design prototypes.
    let dist = site.join("dist");
    if dist.exists() {
        fs::remove_dir_all(&dist).with_context(|| format!("Could not remove {}", dist.display()))?;
    }
    util::copy_dir(&site.join("public"), &dist)?;
    println!("Static site built. Downloads link directly to GitHub Releases.");
    Ok(())
}

fn check(html: &str) -> Result<()> {
    // Downloads go to GitHub's latest release, never a pinned version: a release then
    // needs no website deploy, and winrdp.app can never advertise an old build.
    if !html.contains(LATEST_RELEASE) {
        bail!("Download link does not point at the latest release");
    }
    let pinned = html
        .match_indices("Download v")
        .any(|(at, text)| html[at + text.len()..].starts_with(|next: char| next.is_ascii_digit()));
    if pinned || html.contains("/releases/tag/") {
        bail!("The site names a specific release; link to /releases/latest instead");
    }
    if !html.contains("cursor-connect") {
        bail!("Missing guided product tour");
    }
    let lower = html.to_ascii_lowercase();
    if links(&lower).any(|link| [".deb", ".zip", ".appimage"].iter().any(|kind| link.ends_with(kind))) {
        bail!("Downloads must remain on GitHub Releases");
    }
    Ok(())
}

/// Every quoted href value in lower-cased `html`.
fn links(html: &str) -> impl Iterator<Item = &str> {
    html.match_indices("href=").filter_map(|(at, text)| {
        let value = html[at + text.len()..].strip_prefix(['"', '\''])?;
        value.get(..value.find(['"', '\''])?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(body: &str) -> String {
        format!("<a href=\"{LATEST_RELEASE}\">Download</a><div class=\"cursor-connect\"></div>{body}")
    }

    #[test]
    fn accepts_the_latest_release_link() {
        check(&page("")).unwrap();
    }

    #[test]
    fn refuses_pinned_releases_and_hosted_installers() {
        assert!(check("<a href=\"/download\">Download</a> cursor-connect").is_err());
        assert!(check(&page("Download v0.7.7")).is_err());
        assert!(
            check(&page(
                "<a href='https://github.com/AKolenda/winrdp/releases/tag/v0.7.7'>"
            ))
            .is_err()
        );
        assert!(check(&page("<a href='/winrdp-next_0.7.7_amd64.DEB'>")).is_err());
        assert!(check(&page("<a HREF=\"/Win-RDP.AppImage\">")).is_err());
        assert!(check(&page("<a href=\"/winrdp.zip\">")).is_err());
        assert!(check(&page("Download version notes")).is_ok());
    }
}
