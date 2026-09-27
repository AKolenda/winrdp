//! Resolve the shipped dependency graph and its Cargo registry checksums.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::{CRATES_IO, Dependency, IRONRDP, Key, Package, TARGET};
use crate::util;

/// The shipped packages, and the name each goes by in `used_by`.
const ROOTS: [(&str, &str); 2] = [("winrdp-next", "launcher"), ("winrdp-session", "session")];

/// Every package the launcher and session build, sorted by name and version.
pub(super) fn resolve(root: &Path) -> Result<Vec<Dependency>> {
    if !root.join(IRONRDP).join("Cargo.toml").is_file() {
        bail!("The IronRDP fork is not checked out; run git submodule update --init --recursive");
    }
    let metadata = util::metadata(root, &["--filter-platform", TARGET])?;
    let checksums = lock_checksums(&fs::read_to_string(root.join("Cargo.lock")).context("Could not read Cargo.lock")?);
    let mut used_by: BTreeMap<Key, Vec<String>> = BTreeMap::new();
    for (package, label) in ROOTS {
        for key in tree(root, &[package])?.into_keys() {
            used_by.entry(key).or_default().push(label.to_owned());
        }
    }
    let ironrdp = root.join(IRONRDP);
    let mut dependencies = Vec::new();
    for ((name, version), features) in tree(root, &ROOTS.map(|(package, _)| package))? {
        if ROOTS.iter().any(|(package, _)| *package == name) {
            continue;
        }
        let mut matching = metadata
            .packages
            .iter()
            .filter(|package| package.name == name && package.version == version);
        let (Some(package), None) = (matching.next(), matching.next()) else {
            bail!("cargo metadata does not describe exactly one {name} {version}");
        };
        let dir = package
            .manifest_path
            .parent()
            .expect("a manifest is inside its package")
            .to_path_buf();
        let (source, download_url) = match &package.source {
            Some(source) if source == CRATES_IO => (
                source.clone(),
                Some(format!("https://crates.io/api/v1/crates/{name}/{version}/download")),
            ),
            Some(source) => (source.clone(), None),
            None => {
                let relative = dir
                    .strip_prefix(root)
                    .with_context(|| format!("{name} is outside the repository"))?;
                (format!("path:{}", relative.display()), None)
            }
        };
        let key = (name, version);
        dependencies.push(Dependency {
            record: Package {
                registry_checksum: checksums.get(&key).cloned(),
                used_by: used_by.remove(&key).unwrap_or_default(),
                name: key.0,
                version: key.1,
                source,
                download_url,
                license_expression: package.license.clone(),
                repository: package.repository.clone(),
                features: features.into_iter().collect(),
                notice_files: Vec::new(),
                inherits_ironrdp_licenses: dir.starts_with(&ironrdp),
                status: String::new(),
                notice_source: None,
            },
            homepage: package.homepage.clone(),
            dir,
        });
    }
    Ok(dependencies)
}

/// The packages and enabled features of the normal and build dependency graph of
/// `packages`, resolved as a build of just those packages resolves them.
fn tree(root: &Path, packages: &[&str]) -> Result<BTreeMap<Key, BTreeSet<String>>> {
    let mut cargo = util::cargo();
    // CI can force colored output even through a pipe. Keep Cargo's repeated-node
    // marker plain so ANSI escapes cannot become part of a recorded feature name.
    cargo.current_dir(root).args([
        "tree",
        "--color",
        "never",
        "--locked",
        "--edges",
        "normal,build",
        "--target",
        TARGET,
        "--prefix",
        "none",
        "--format",
        "{p}|{f}",
    ]);
    for package in packages {
        cargo.args(["--package", package]);
    }
    let mut graph: BTreeMap<_, BTreeSet<String>> = BTreeMap::new();
    for line in util::output(&mut cargo)?.lines() {
        // Blank lines separate the trees of several packages.
        let Some((package, features)) = line.split_once('|') else {
            continue;
        };
        let mut words = package.split_whitespace();
        let (Some(name), Some(version)) = (words.next(), words.next().and_then(|word| word.strip_prefix('v'))) else {
            bail!("Unexpected cargo tree line: {line}");
        };
        // A package built both for the host and the target appears twice, with each set of features.
        let enabled = graph.entry((name.to_owned(), version.to_owned())).or_default();
        let features = features.trim_end_matches(" (*)");
        enabled.extend(
            features
                .split(',')
                .filter(|feature| !feature.is_empty())
                .map(str::to_owned),
        );
    }
    Ok(graph)
}

/// Registry checksums by package name and version, from a Cargo.lock.
fn lock_checksums(lock: &str) -> BTreeMap<Key, String> {
    lock.split("[[package]]")
        .filter_map(|block| {
            let field = |key: &str| {
                let prefix = format!("{key} = \"");
                block
                    .lines()
                    .find_map(|line| Some(line.strip_prefix(&prefix)?.strip_suffix('"')?.to_owned()))
            };
            Some(((field("name")?, field("version")?), field("checksum")?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_checksums_from_the_lock_file() {
        let lock = "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"1.0.0\"\nsource = \"registry+x\"\n\
                    checksum = \"abc\"\n\n[[package]]\nname = \"local\"\nversion = \"0.1.0\"\ndependencies = [\n \"a\",\n]\n";
        let checksums = lock_checksums(lock);
        assert_eq!(checksums.len(), 1);
        assert_eq!(checksums[&("a".to_owned(), "1.0.0".to_owned())], "abc");
    }
}
