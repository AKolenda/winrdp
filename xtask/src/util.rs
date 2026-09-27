//! Process, file and workspace helpers shared by the commands.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The repository root: xtask lives one directory below it.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is inside the repository")
        .to_path_buf()
}

/// The cargo that started xtask, so a toolchain override also applies to the builds it runs.
pub fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// A command as a reader would type it, for error messages.
pub fn describe(command: &Command) -> String {
    let words = std::iter::once(command.get_program()).chain(command.get_args());
    words.map(OsStr::to_string_lossy).collect::<Vec<_>>().join(" ")
}

/// Runs `command` and returns its standard output; a failure carries its standard error.
pub fn capture(command: &mut Command) -> Result<Vec<u8>> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Could not run {}", describe(command)))?;
    if !output.status.success() {
        bail!(
            "Command failed ({}): {}\n{}",
            output.status,
            describe(command),
            String::from_utf8_lossy(&output.stderr).trim_end()
        );
    }
    Ok(output.stdout)
}

pub fn output(command: &mut Command) -> Result<String> {
    capture(command).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Runs `command` with the terminal attached.
pub fn status(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("Could not run {}", describe(command)))?;
    if !status.success() {
        bail!("Command failed ({status}): {}", describe(command));
    }
    Ok(())
}

/// Finds a program on PATH, or in the sbin directories a normal user's PATH may lack.
pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(["/usr/sbin", "/sbin"].map(PathBuf::from))
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

/// Whether this process runs as root: `/proc/self` belongs to the effective user.
pub fn is_root() -> Result<bool> {
    let proc = fs::metadata("/proc/self").context("Could not read /proc/self")?;
    Ok(proc.uid() == 0)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

/// Copies a directory tree; symbolic links are copied as the files they point to.
pub fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).with_context(|| format!("Could not create {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("Could not read {}", from.display()))? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if source.is_dir() {
            copy_dir(&source, &target)?;
        } else {
            fs::copy(&source, &target)
                .with_context(|| format!("Could not copy {} to {}", source.display(), target.display()))?;
        }
    }
    Ok(())
}

/// A private scratch directory, removed when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(prefix: &str) -> Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        // create_dir, not create_dir_all: never reuse a directory someone else prepared.
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .with_context(|| format!("Could not create {}", path.display()))?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The parts of `cargo metadata` the commands read.
#[derive(Deserialize)]
pub struct Metadata {
    pub packages: Vec<MetadataPackage>,
    pub target_directory: PathBuf,
}

#[derive(Deserialize)]
pub struct MetadataPackage {
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    pub manifest_path: PathBuf,
}

/// Runs `cargo metadata --locked` with `args` and parses the result.
pub fn metadata(root: &Path, args: &[&str]) -> Result<Metadata> {
    let mut cargo = cargo();
    cargo
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--locked"])
        .args(args);
    serde_json::from_slice(&capture(&mut cargo)?).context("Could not parse cargo metadata output")
}

/// Where cargo puts build output, honouring CARGO_TARGET_DIR and cargo configuration.
pub fn target_dir(root: &Path) -> Result<PathBuf> {
    Ok(metadata(root, &["--no-deps"])?.target_directory)
}
