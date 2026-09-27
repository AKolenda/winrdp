//! Fetch missing notice texts from the revision recorded in a crate archive.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{NoticeSource, SourceFile, is_notice_name};
use crate::util;

#[derive(Deserialize)]
struct VcsInfo {
    git: GitInfo,
    #[serde(default)]
    path_in_vcs: String,
}

#[derive(Deserialize)]
struct GitInfo {
    sha1: String,
}

#[derive(Deserialize)]
struct ContentEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

/// Texts for a crate whose archive (unpacked at `source`) ships none, from its
/// GitHub repository at the commit the archive was published from: the crate's own
/// directory first, then each parent up to the repository root.
pub(super) fn fetch_upstream(source: &Path, repository: Option<&str>, dest: &Path) -> Result<NoticeSource> {
    let vcs_info = fs::read(source.join(".cargo_vcs_info.json")).context("The archive has no .cargo_vcs_info.json")?;
    let vcs: VcsInfo = serde_json::from_slice(&vcs_info).context("Could not parse .cargo_vcs_info.json")?;
    let (owner, repo) = repository
        .and_then(github_repository)
        .context("The crate names no GitHub repository to collect licence texts from")?;
    let commit = vcs.git.sha1;
    let mut dir = vcs.path_in_vcs.trim_matches('/').to_owned();
    loop {
        let listing = fetch(&format!(
            "https://api.github.com/repos/{owner}/{repo}/contents/{dir}?ref={commit}"
        ))?;
        let entries: Vec<ContentEntry> = serde_json::from_slice(&listing).context("Unexpected GitHub API response")?;
        let mut names: Vec<_> = entries
            .into_iter()
            .filter(|entry| entry.kind == "file" && is_notice_name(&entry.name))
            .map(|entry| entry.name)
            .collect();
        names.sort();
        if !names.is_empty() {
            fs::create_dir_all(dest)?;
            let mut files = Vec::new();
            for name in names {
                let path_in_repo = if dir.is_empty() {
                    name.clone()
                } else {
                    format!("{dir}/{name}")
                };
                let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/{commit}/{path_in_repo}");
                fs::write(dest.join(&name), fetch(&url)?)?;
                files.push(SourceFile {
                    file: name,
                    path_in_repo: Some(path_in_repo),
                    url,
                });
            }
            return Ok(NoticeSource {
                origin: "upstream repository".to_owned(),
                repository: format!("https://github.com/{owner}/{repo}"),
                commit,
                commit_source: ".cargo_vcs_info.json in the published crate archive".to_owned(),
                files,
                note: None,
            });
        }
        if dir.is_empty() {
            bail!("github.com/{owner}/{repo} has no licence file at {commit}");
        }
        dir = dir
            .rsplit_once('/')
            .map_or_else(String::new, |(parent, _)| parent.to_owned());
    }
}

/// The owner and name of a GitHub repository from a URL into it.
fn github_repository(url: &str) -> Option<(String, String)> {
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let mut parts = path.split('/');
    let owner = parts.next().filter(|owner| !owner.is_empty())?;
    let repo = parts
        .next()
        .map(|repo| repo.trim_end_matches(".git"))
        .filter(|repo| !repo.is_empty())?;
    Some((owner.to_owned(), repo.to_owned()))
}

fn fetch(url: &str) -> Result<Vec<u8>> {
    util::capture(Command::new("curl").args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--proto",
        "=https",
        url,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_repository_in_github_urls() {
        let expected = Some(("etemesi254".to_owned(), "zune-image".to_owned()));
        assert_eq!(
            github_repository("https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg"),
            expected
        );
        assert_eq!(
            github_repository("https://github.com/etemesi254/zune-image.git"),
            expected
        );
        assert_eq!(github_repository("https://gitlab.com/owner/repo"), None);
    }
}
