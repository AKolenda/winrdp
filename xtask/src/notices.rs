//! The Rust dependency notice inventory: docs/rust-dependency-notices.json and the
//! licence texts under docs/licenses/rust/.
//!
//! The inventory covers the packages a build of the launcher and the session
//! compiles for Linux x86_64: normal and build dependencies with the features that
//! build enables, never xtask's own dependencies or dev-only edges.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

mod graph;
mod upstream;

use self::graph::resolve;
use self::upstream::fetch_upstream;
use crate::util;

const MANIFEST: &str = "docs/rust-dependency-notices.json";
/// Where the texts live, relative to docs/, as the manifest's paths are.
const LICENSES: &str = "licenses/rust";
const IRONRDP: &str = "third_party/ironrdp";
const TARGET: &str = "x86_64-unknown-linux-gnu";
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

const SCOPE: &str = "Cargo default-feature dependency resolution for launcher and session, including build \
    dependencies and excluding dev-only dependency edges; this is a conservative source inventory, not proof of \
    code linked in each binary.";
const LIMITATIONS: [&str; 3] = [
    "Native dependencies compiled or linked by build scripts may require additional source notices.",
    "License expressions and collected notices have not been independently audited for legal completeness.",
    "Upstream source download URLs and Cargo registry checksums identify exact crate archives; an offline \
     vendor-source archive is not included.",
];

#[derive(Serialize, Deserialize)]
struct Manifest {
    target: String,
    scope: String,
    collection: String,
    ironrdp_revision: String,
    limitations: Vec<String>,
    package_count: usize,
    missing_license_text: Vec<Missing>,
    packages: Vec<Package>,
    notice_exceptions: Vec<NoticeException>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Package {
    name: String,
    version: String,
    source: String,
    download_url: Option<String>,
    registry_checksum: Option<String>,
    license_expression: Option<String>,
    repository: Option<String>,
    used_by: Vec<String>,
    features: Vec<String>,
    notice_files: Vec<NoticeFile>,
    inherits_ironrdp_licenses: bool,
    status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notice_source: Option<NoticeSource>,
}

#[derive(Clone, Serialize, Deserialize)]
struct NoticeFile {
    path: String,
    sha256: String,
}

/// Where texts came from when the crate's archive has none.
#[derive(Clone, Serialize, Deserialize)]
struct NoticeSource {
    origin: String,
    repository: String,
    commit: String,
    commit_source: String,
    files: Vec<SourceFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct SourceFile {
    file: String,
    path_in_repo: Option<String>,
    url: String,
}

#[derive(Serialize, Deserialize)]
struct Missing {
    name: String,
    version: String,
    license_expression: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct NoticeException {
    name: String,
    version: String,
    license_expression: String,
    issue: String,
    resolution: String,
}

/// A package of the resolved graph: its manifest record without notices yet, and
/// where its source is.
struct Dependency {
    record: Package,
    dir: PathBuf,
    homepage: Option<String>,
}

type Key = (String, String);

impl Manifest {
    fn read(root: &Path) -> Result<Self> {
        let bytes = fs::read(root.join(MANIFEST)).with_context(|| format!("Could not read {MANIFEST}"))?;
        serde_json::from_slice(&bytes).with_context(|| format!("Could not parse {MANIFEST}"))
    }
}

impl Package {
    fn key(&self) -> Key {
        (self.name.clone(), self.version.clone())
    }

    fn label(&self) -> String {
        format!("{} {}", self.name, self.version)
    }

    /// The directory under licenses/rust/ that holds this package's texts.
    fn notice_dir(&self) -> String {
        format!("{}-{}", self.name, self.version)
    }
}

/// Regenerates the manifest and the licence texts from Cargo.lock and the crate sources.
pub fn generate(root: &Path) -> Result<()> {
    let previous = if root.join(MANIFEST).exists() {
        Some(Manifest::read(root)?)
    } else {
        None
    };
    // Texts collected from upstream (or, exceptionally, by hand) are kept as they are.
    let upstream: BTreeMap<Key, NoticeSource> = previous
        .iter()
        .flat_map(|manifest| &manifest.packages)
        .filter_map(|package| Some((package.key(), package.notice_source.clone()?)))
        .collect();
    let docs = root.join("docs");
    let mut packages = Vec::new();
    let mut fetched = 0;
    for Dependency {
        mut record,
        dir: source,
        homepage,
    } in resolve(root)?
    {
        record.notice_source = upstream.get(&record.key()).cloned();
        if record.notice_source.is_none() {
            let repository = record.repository.as_deref().or(homepage.as_deref());
            record.notice_source = collect_texts(root, &record, &source, repository)?;
            fetched += usize::from(record.notice_source.is_some());
        }
        record.notice_files = hashed(&docs, &record)?;
        let collected = !record.notice_files.is_empty() || record.inherits_ironrdp_licenses;
        record.status = if collected { "collected" } else { "missing" }.to_owned();
        packages.push(record);
    }
    remove_stale_notices(&docs.join(LICENSES), &packages)?;

    let revision = util::output(
        Command::new("git")
            .args(["-C", IRONRDP, "rev-parse", "HEAD"])
            .current_dir(root),
    )?;
    let present: BTreeSet<Key> = packages.iter().map(Package::key).collect();
    let manifest = Manifest {
        target: TARGET.to_owned(),
        scope: SCOPE.to_owned(),
        collection: collection(
            packages
                .iter()
                .filter(|package| package.notice_source.is_some())
                .count(),
        ),
        ironrdp_revision: revision.trim().to_owned(),
        limitations: LIMITATIONS.map(str::to_owned).to_vec(),
        package_count: packages.len(),
        missing_license_text: packages
            .iter()
            .filter(|package| package.status != "collected")
            .map(|package| Missing {
                name: package.name.clone(),
                version: package.version.clone(),
                license_expression: package.license_expression.clone(),
            })
            .collect(),
        notice_exceptions: previous
            .map(|manifest| manifest.notice_exceptions)
            .unwrap_or_default()
            .into_iter()
            .filter(|exception| present.contains(&(exception.name.clone(), exception.version.clone())))
            .collect(),
        packages,
    };
    let json = serde_json::to_string_pretty(&manifest)? + "\n";
    fs::write(root.join(MANIFEST), json).with_context(|| format!("Could not write {MANIFEST}"))?;
    let files: usize = manifest.packages.iter().map(|package| package.notice_files.len()).sum();
    println!(
        "Wrote {MANIFEST}: {} packages, {files} licence files, {fetched} newly collected from upstream repositories.",
        manifest.package_count
    );
    if !manifest.missing_license_text.is_empty() {
        let names: Vec<_> = manifest
            .missing_license_text
            .iter()
            .map(|missing| format!("{} {}", missing.name, missing.version))
            .collect();
        bail!(
            "No licence text found for: {}.\nPut the texts in docs/{LICENSES}/<name>-<version>/, record where they \
             came from under \"notice_source\" and explain under \"notice_exceptions\", then run this again.",
            names.join(", ")
        );
    }
    Ok(())
}

/// Replaces `record`'s notice directory with the licence texts in its source at
/// `source`. A crate of the fork with none of its own gets the fork's; a registry
/// crate with none gets its upstream repository's, and the returned provenance.
fn collect_texts(
    root: &Path,
    record: &Package,
    source: &Path,
    repository: Option<&str>,
) -> Result<Option<NoticeSource>> {
    let dir = root.join("docs").join(LICENSES).join(record.notice_dir());
    let mut from = source.to_path_buf();
    let mut files = notice_files(source)?;
    if files.is_empty() && record.inherits_ironrdp_licenses {
        from = root.join(IRONRDP);
        files = ["LICENSE-APACHE", "LICENSE-MIT"].map(PathBuf::from).to_vec();
    }
    copy_notices(&from, &files, &dir)?;
    if !files.is_empty() || record.source != CRATES_IO {
        return Ok(None);
    }
    match fetch_upstream(source, repository, &dir) {
        Ok(notice_source) => Ok(Some(notice_source)),
        Err(error) => {
            eprintln!("{}: {error:#}", record.label());
            // Never keep part of a download as if it were the whole notice.
            if dir.exists() {
                fs::remove_dir_all(&dir)?;
            }
            Ok(None)
        }
    }
}

/// Replaces `dir` with `files` (relative to `source`); no directory at all if there
/// are none.
fn copy_notices(source: &Path, files: &[PathBuf], dir: &Path) -> Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("Could not remove {}", dir.display()))?;
    }
    for relative in files {
        let target = dir.join(relative);
        fs::create_dir_all(target.parent().expect("notice files are inside a directory"))?;
        // Contents only: an executable bit on a licence text is an accident upstream.
        fs::write(&target, notice_text(&source.join(relative))?)
            .with_context(|| format!("Could not write {}", target.display()))?;
    }
    Ok(())
}

/// The text of a notice file. A file holding nothing but a relative path to another
/// file stands for that file: ironrdp-dvc-pipe-proxy commits its licences as the
/// targets of symbolic links (`../../LICENSE-MIT`) in plain files.
fn notice_text(path: &Path) -> Result<Vec<u8>> {
    let text = fs::read(path).with_context(|| format!("Could not read {}", path.display()))?;
    let pointer = std::str::from_utf8(&text).map(str::trim).ok();
    let pointer = pointer.filter(|pointer| pointer.starts_with("../") && !pointer.contains(char::is_whitespace));
    match pointer
        .and_then(|pointer| Some(path.parent()?.join(pointer)))
        .filter(|target| target.is_file())
    {
        Some(target) => fs::read(&target).with_context(|| format!("Could not read {}", target.display())),
        None => Ok(text),
    }
}

/// Removes the notice directories of packages that are no longer dependencies.
fn remove_stale_notices(licenses: &Path, packages: &[Package]) -> Result<()> {
    let present: BTreeSet<String> = packages.iter().map(Package::notice_dir).collect();
    for entry in fs::read_dir(licenses).with_context(|| format!("Could not read {}", licenses.display()))? {
        let entry = entry?;
        if !present.contains(&*entry.file_name().to_string_lossy()) {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

/// Verifies the manifest against the dependency graph and the licence texts on disk.
pub fn check(root: &Path) -> Result<()> {
    let manifest = Manifest::read(root)?;
    let mut failures = coverage(&manifest, resolve(root)?);
    let referenced = check_notices(&manifest, &root.join("docs"), &mut failures)?;
    // The committed pointer, which is what a clone of this revision builds; a source
    // tree outside git has none to compare.
    let pointer = util::output(Command::new("git").args(["ls-tree", "HEAD", IRONRDP]).current_dir(root));
    if let Some(revision) = pointer.unwrap_or_default().split_whitespace().nth(2)
        && revision != manifest.ironrdp_revision
    {
        failures.push(format!(
            "ironrdp_revision {} disagrees with the submodule pointer {revision}",
            manifest.ironrdp_revision
        ));
    }
    if !failures.is_empty() {
        bail!(
            "Dependency notice inventory problems:\n- {}\n\nRun cargo xtask notices to regenerate the inventory.",
            failures.join("\n- ")
        );
    }
    println!(
        "Dependency notices agree: {} packages, {referenced} licence files",
        manifest.packages.len()
    );
    Ok(())
}

/// Where the manifest's package records and the dependency graph differ.
fn coverage(manifest: &Manifest, graph: Vec<Dependency>) -> Vec<String> {
    let expected: BTreeMap<Key, Package> = graph
        .into_iter()
        .map(|dependency| (dependency.record.key(), dependency.record))
        .collect();
    let recorded: BTreeMap<Key, &Package> = manifest
        .packages
        .iter()
        .map(|package| (package.key(), package))
        .collect();
    let mut failures = Vec::new();
    for (key, package) in &expected {
        if !recorded.contains_key(key) {
            failures.push(format!("{} is a dependency but has no record", package.label()));
        }
    }
    for (key, package) in &recorded {
        match expected.get(key) {
            None => failures.push(format!("{} is recorded but no longer a dependency", package.label())),
            Some(graph) => {
                let differing = differences(package, graph);
                if !differing.is_empty() {
                    failures.push(format!(
                        "{} does not match the dependency graph in {}",
                        package.label(),
                        differing.join(", ")
                    ));
                }
            }
        }
    }
    if manifest.package_count != manifest.packages.len() {
        failures.push(format!(
            "package_count {} disagrees with {} package records",
            manifest.package_count,
            manifest.packages.len()
        ));
    }
    failures
}

/// Checks every recorded text against the files on disk and returns how many there are.
fn check_notices(manifest: &Manifest, docs: &Path, failures: &mut Vec<String>) -> Result<usize> {
    let mut referenced = BTreeSet::new();
    for package in &manifest.packages {
        let label = package.label();
        for notice in &package.notice_files {
            let path = docs.join(&notice.path);
            match fs::read(&path) {
                Err(_) => failures.push(format!("{label} references a missing notice file: {}", notice.path)),
                Ok(bytes) => {
                    let digest = util::sha256_hex(&bytes);
                    if digest != notice.sha256 {
                        failures.push(format!(
                            "{label} notice {} has sha256 {digest}, manifest says {}",
                            notice.path, notice.sha256
                        ));
                    }
                }
            }
            referenced.insert(path);
        }
        if package.status != "collected" {
            failures.push(format!("{label} is recorded as \"{}\"", package.status));
        }
        if package.notice_files.is_empty() && !package.inherits_ironrdp_licenses {
            failures.push(format!(
                "{label} has no notice file and does not inherit the IronRDP licences"
            ));
        }
        if let Some(source) = &package.notice_source {
            let commit = source.commit.as_str();
            if commit.len() != 40 || !commit.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')) {
                failures.push(format!(
                    "{label} notice_source commit is not a full revision: {commit:?}"
                ));
            }
            for entry in &source.files {
                if !entry.url.starts_with("https://") {
                    failures.push(format!("{label} notice_source url is not https: {}", entry.url));
                }
                if entry.url.contains("raw.githubusercontent.com") && !entry.url.contains(commit) {
                    failures.push(format!(
                        "{label} notice_source url is not pinned to {commit}: {}",
                        entry.url
                    ));
                }
            }
        }
    }
    for path in walk_files(&docs.join(LICENSES))? {
        if !referenced.contains(&path) {
            let relative = path.strip_prefix(docs)?.display();
            failures.push(format!("{relative} is on disk but no package references it"));
        }
    }
    let explained: BTreeSet<Key> = manifest
        .notice_exceptions
        .iter()
        .map(|exception| (exception.name.clone(), exception.version.clone()))
        .collect();
    for missing in &manifest.missing_license_text {
        if !explained.contains(&(missing.name.clone(), missing.version.clone())) {
            failures.push(format!(
                "{} {} has no licence text and no recorded exception",
                missing.name, missing.version
            ));
        }
    }
    Ok(referenced.len())
}

/// The fields of `recorded` that are not what the dependency graph says.
fn differences(recorded: &Package, graph: &Package) -> Vec<&'static str> {
    [
        ("source", recorded.source == graph.source),
        ("download_url", recorded.download_url == graph.download_url),
        (
            "registry_checksum",
            recorded.registry_checksum == graph.registry_checksum,
        ),
        (
            "license_expression",
            recorded.license_expression == graph.license_expression,
        ),
        ("repository", recorded.repository == graph.repository),
        ("used_by", recorded.used_by == graph.used_by),
        ("features", recorded.features == graph.features),
        (
            "inherits_ironrdp_licenses",
            recorded.inherits_ironrdp_licenses == graph.inherits_ironrdp_licenses,
        ),
    ]
    .into_iter()
    .filter_map(|(field, same)| (!same).then_some(field))
    .collect()
}

fn collection(upstream: usize) -> String {
    format!(
        "Generated by cargo xtask notices from cargo tree --edges normal,build --target {TARGET} for the \
         launcher and session packages, with package details from cargo metadata --locked --filter-platform \
         {TARGET} and checksums from Cargo.lock. Non-Linux targets were not inspected. License texts retain their \
         original upstream content. For the {upstream} crates that publish no licence file inside their crates.io \
         archive, the texts were collected verbatim from the crate's own upstream repository, pinned to the commit \
         recorded in the archive's .cargo_vcs_info.json, and each such package records that provenance under \
         \"notice_source\"."
    )
}

/// Licence and notice files in a crate's source, relative to `dir` and sorted.
fn notice_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    collect_notices(dir, Path::new(""), false, &mut found)?;
    found.sort();
    Ok(found)
}

fn collect_notices(dir: &Path, relative: &Path, in_licenses: bool, found: &mut Vec<PathBuf>) -> Result<()> {
    let here = dir.join(relative);
    for entry in fs::read_dir(&here).with_context(|| format!("Could not read {}", here.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = relative.join(&name);
        if entry.file_type()?.is_dir() {
            let licenses =
                in_licenses || name.eq_ignore_ascii_case("licenses") || name.eq_ignore_ascii_case("licences");
            collect_notices(dir, &path, licenses, found)?;
        } else if (in_licenses || is_notice_name(&name)) && dir.join(&path).is_file() {
            // is_file follows symbolic links: IronRDP's crates link to the workspace's licences.
            found.push(path);
        }
    }
    Ok(())
}

/// LICENSE, LICENCE, UNLICENSE, COPYING, COPYRIGHT, NOTICE(S) and
/// THIRD_PARTY_NOTICES files, alone or followed by `-`, `_` or `.` and more, in
/// any case. Rust sources named after licences (`license_exchange.rs`) are code.
fn is_notice_name(name: &str) -> bool {
    const STEMS: [&str; 8] = [
        "license",
        "licence",
        "unlicense",
        "copying",
        "copyright",
        "notice",
        "third_party_notice",
        "third-party-notice",
    ];
    let name = name.to_ascii_lowercase();
    let is_notice = STEMS.iter().any(|stem| {
        let rest = name
            .strip_prefix(stem)
            .map(|rest| rest.strip_prefix('s').unwrap_or(rest));
        rest.is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_', '.']))
    });
    is_notice && !name.ends_with(".rs")
}

/// The manifest records of the texts in `package`'s notice directory.
fn hashed(docs: &Path, package: &Package) -> Result<Vec<NoticeFile>> {
    let dir = docs.join(LICENSES).join(package.notice_dir());
    let mut files = Vec::new();
    for path in walk_files(&dir)? {
        let relative = path.strip_prefix(docs)?;
        let bytes = fs::read(&path)?;
        let path = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        files.push(NoticeFile {
            path,
            sha256: util::sha256_hex(&bytes),
        });
    }
    Ok(files)
}

/// The files below `dir`, sorted; nothing if it does not exist.
fn walk_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir).with_context(|| format!("Could not read {}", dir.display()))? {
            let path = entry?.path();
            if path.is_dir() {
                files.extend(walk_files(&path)?);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_licence_and_notice_files() {
        for name in [
            "LICENSE",
            "LICENSE-MIT",
            "LICENSE_APACHE-2.0",
            "License.txt",
            "LICENCE",
            "COPYING",
            "COPYING.LIB",
            "COPYRIGHT.md",
            "NOTICE",
            "NOTICES.md",
            "UNLICENSE",
            "THIRD_PARTY_NOTICES",
        ] {
            assert!(is_notice_name(name), "{name}");
        }
        for name in [
            "license_exchange.rs",
            "README.md",
            "AUTHORS",
            "licensed.txt",
            "Cargo.toml",
        ] {
            assert!(!is_notice_name(name), "{name}");
        }
    }
}
