//! Package compiled binaries into a .deb; never a source-only or fake package.
//!
//! Run on the oldest distribution the package should install on: dpkg-shlibdeps
//! derives Depends from this host's libraries. Both programs are Rust and the
//! package bundles no native library, not glibc, graphics drivers or fonts either.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use md5::{Digest, Md5};
use serde::Serialize;

use crate::util::{self, TempDir};

/// The version the package is built as; check-release keeps it equal to the
/// crates' versions and the AppStream release.
pub const VERSION: &str = "0.8.0";

const MAINTAINER: &str = "Win RDP project <build@localhost>";

const DESCRIPTION: &str = "Win RDP desktop client for Linux
 Rust-powered IronRDP sessions, each in its own window, from a native launcher.
 This package installs a client only; it does not enable an RDP host service.
";

const POSTINST: &str = "#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null; then update-desktop-database -q /usr/share/applications || true; fi
if command -v gtk-update-icon-cache >/dev/null; then gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true; fi
";

pub fn file_name(arch: &str) -> String {
    format!("winrdp-next_{VERSION}_{arch}.deb")
}

#[derive(Serialize)]
struct Report<'a> {
    version: &'a str,
    architecture: &'a str,
    depends: &'a str,
    sha256: &'a str,
    live_rdp_tested: bool,
}

/// Builds `out/winrdp-next_<version>_<arch>.deb` and its checksum and report.
/// Nothing is written to `out` unless both binaries are real, current builds.
pub fn package(root: &Path, launcher: &Path, session: &Path, out: &Path) -> Result<PathBuf> {
    let launcher = compiled(launcher, "A compiled ELF Win RDP launcher (winrdp-next)")?;
    let session = compiled(session, "A compiled ELF winrdp-session binary")?;
    check_version(&launcher, &format!("Win RDP {VERSION}"))?;
    check_version(&session, &format!("winrdp-session {VERSION}"))?;
    let arch = util::output(Command::new("dpkg").arg("--print-architecture"))?;
    let arch = arch.trim();
    fs::create_dir_all(out).with_context(|| format!("Could not create {}", out.display()))?;

    let temp = TempDir::new("winrdp-deb")?;
    let stage = Stage(temp.path().join("stage"));
    let bin = "usr/lib/winrdp-next/bin";
    let binaries = [
        stage.copy(&launcher, &format!("{bin}/winrdp-next"), 0o755)?,
        stage.copy(&session, &format!("{bin}/winrdp-session"), 0o755)?,
    ];
    for binary in &binaries {
        // --version opens no window and connects to no remote machine.
        util::output(system_command(binary).arg("--version"))?;
        for library in linked(binary)?.values() {
            if owning_package(library).is_none() {
                bail!(
                    "System library is not owned by an installed Debian package: {}. \
                     Use a matching, packaged build environment; not an arbitrary local library.",
                    library.display()
                );
            }
        }
    }
    stage_files(root, &stage)?;
    let depends = depends(temp.path(), &stage, &binaries, arch)?;

    let mut size = 0;
    let mut md5sums = String::new();
    for relative in stage.files()? {
        let contents = fs::read(stage.0.join(&relative))?;
        size += contents.len();
        let digest = util::hex(&Md5::digest(&contents));
        writeln!(md5sums, "{digest}  {}", relative.display()).expect("writing to a String cannot fail");
    }
    let control = format!(
        "Package: winrdp-next\nVersion: {VERSION}\nArchitecture: {arch}\nSection: net\nPriority: optional\n\
         Maintainer: {MAINTAINER}\nInstalled-Size: {}\nDepends: {depends}\nDescription: {DESCRIPTION}",
        size / 1024
    );
    stage.write("DEBIAN/control", control.as_bytes(), 0o644)?;
    stage.write("DEBIAN/postinst", POSTINST.as_bytes(), 0o755)?;
    stage.write("DEBIAN/md5sums", md5sums.as_bytes(), 0o644)?;
    stage.set_directory_modes()?;

    let name = file_name(arch);
    let destination = out.join(&name);
    util::output(
        Command::new("dpkg-deb")
            .args(["--root-owner-group", "--build"])
            .arg(&stage.0)
            .arg(&destination),
    )?;
    util::output(Command::new("dpkg-deb").arg("--info").arg(&destination))?;
    let sha256 = util::sha256_hex(&fs::read(&destination)?);
    let report = Report {
        version: VERSION,
        architecture: arch,
        depends: &depends,
        sha256: &sha256,
        live_rdp_tested: false,
    };
    fs::write(
        out.join("package-report.json"),
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    fs::write(out.join(format!("{name}.sha256")), format!("{sha256}  {name}\n"))?;
    println!("Created real binary package: {}", destination.display());
    println!("SHA256: {sha256}");
    println!("Host services and firewall were not modified.");
    Ok(destination)
}

/// Stages everything but the binaries: launchers on PATH, desktop integration, icons,
/// documentation and every licence text.
fn stage_files(root: &Path, stage: &Stage) -> Result<()> {
    for name in ["winrdp-next", "winrdp-session"] {
        let wrapper = format!("#!/bin/sh\nexec /usr/lib/winrdp-next/bin/{name} \"$@\"\n");
        stage.write(&format!("usr/bin/{name}"), wrapper.as_bytes(), 0o755)?;
    }
    let icons = "usr/share/icons/hicolor";
    let docs = "usr/share/doc/winrdp-next";
    for (source, target) in [
        (
            "packaging/io.winrdp.Next.desktop",
            "usr/share/applications/io.winrdp.Next.desktop",
        ),
        (
            "packaging/io.winrdp.Next.metainfo.xml",
            "usr/share/metainfo/io.winrdp.Next.metainfo.xml",
        ),
        (
            "launcher/assets/winrdp.svg",
            &format!("{icons}/scalable/apps/winrdp-next.svg"),
        ),
        ("README.md", &format!("{docs}/README.md")),
        ("BUILDING.md", &format!("{docs}/BUILDING.md")),
        ("LICENSE", &format!("{docs}/LICENSE")),
        ("PROVENANCE.md", &format!("{docs}/PROVENANCE.md")),
        ("docs/THIRD-PARTY.md", &format!("{docs}/THIRD-PARTY.md")),
        (
            "docs/rust-dependency-notices.json",
            &format!("{docs}/rust-dependency-notices.json"),
        ),
        (
            "third_party/ironrdp/LICENSE-MIT",
            &format!("{docs}/IronRDP-LICENSE-MIT"),
        ),
        (
            "third_party/ironrdp/LICENSE-APACHE",
            &format!("{docs}/IronRDP-LICENSE-APACHE"),
        ),
    ] {
        stage.copy(&root.join(source), target, 0o644)?;
    }
    // Fixed sizes win over scalable; 16-32 px are drawn on the pixel grid (cargo xtask branding).
    for (size, png) in icons_in(&root.join("packaging/icons"))? {
        stage.copy(&png, &format!("{icons}/{size}x{size}/apps/winrdp-next.png"), 0o644)?;
    }
    stage.copy_tree(&root.join("docs/licenses"), &format!("{docs}/licenses"))
}

/// The package's Depends: what dpkg-shlibdeps derives from the binaries' ELF
/// dependencies, the fallback font, and the libraries the binaries open at run time.
fn depends(temp: &Path, stage: &Stage, binaries: &[PathBuf], arch: &str) -> Result<String> {
    // dpkg-shlibdeps needs a source package's control file, and finds the package
    // build tree by the DEBIAN/control above the binaries. Every linked library was
    // checked for a dpkg owner before --ignore-missing-info is used.
    fs::create_dir(temp.join("debian"))?;
    fs::write(
        temp.join("debian/control"),
        format!(
            "Source: winrdp-next\nSection: net\nPriority: optional\nMaintainer: {MAINTAINER}\n\n\
             Package: winrdp-next\nArchitecture: any\nDescription: Win RDP desktop client\n"
        ),
    )?;
    let control = format!(
        "Package: winrdp-next\nVersion: {VERSION}\nArchitecture: {arch}\nMaintainer: {MAINTAINER}\n\
         Description: Win RDP desktop client\n"
    );
    stage.write("DEBIAN/control", control.as_bytes(), 0o644)?;
    let mut shlibdeps = system_command(Path::new("dpkg-shlibdeps"));
    shlibdeps.current_dir(temp).args(["-O", "--ignore-missing-info"]);
    for binary in binaries {
        let mut arg = OsString::from("-e");
        arg.push(binary);
        shlibdeps.arg(arg);
    }
    let result = util::output(&mut shlibdeps)?;
    let shlibs = result
        .lines()
        .find_map(|line| line.strip_prefix("shlibs:Depends="))
        .filter(|depends| depends.contains("libc6"))
        .with_context(|| {
            format!("Could not derive complete runtime dependencies; refusing to create .deb.\n{result}")
        })?;
    let declared: BTreeSet<&str> = shlibs
        .split(',')
        .filter_map(|clause| clause.split_whitespace().next())
        .collect();
    let opened = dlopen_dependencies(binaries)?;
    // The session's connection bar falls back to DejaVu Sans when no desktop font is found.
    let depends = [shlibs, "fonts-dejavu-core"].into_iter().chain(
        opened
            .keys()
            .map(String::as_str)
            .filter(|name| !declared.contains(name)),
    );
    Ok(depends.collect::<Vec<_>>().join(", "))
}

/// Reject a stale or unrelated executable before staging a package.
fn check_version(binary: &Path, expected: &str) -> Result<()> {
    let reported = util::output(system_command(binary).arg("--version"))?;
    if reported.trim() != expected {
        bail!(
            "{} reports {:?}, expected {expected:?}; rebuild it. No package was created.",
            binary.display(),
            reported.trim()
        );
    }
    Ok(())
}

/// The canonical path of `path` if it is an ELF file, so nothing else is ever packaged.
fn compiled(path: &Path, what: &str) -> Result<PathBuf> {
    let mut magic = [0; 4];
    let read = fs::File::open(path).and_then(|mut file| file.read_exact(&mut magic));
    if read.is_err() || magic != *b"\x7fELF" {
        bail!("{what} is required at {}. No package was created.", path.display());
    }
    Ok(path.canonicalize()?)
}

/// A command that finds shared libraries where an installed program does: without
/// the builder's LD_LIBRARY_PATH, so no library outside the system paths satisfies it.
fn system_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command.env_remove("LD_LIBRARY_PATH");
    command
}

/// The shared libraries `binary` links, by soname, as the dynamic loader resolves them.
fn linked(binary: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let output = util::output(system_command(Path::new("ldd")).arg(binary))?;
    if output.contains("not found") {
        bail!("Unresolved libraries for {}:\n{output}", binary.display());
    }
    Ok(output
        .lines()
        .filter_map(|line| {
            let (soname, rest) = line.trim().split_once(" => ")?;
            let path = rest.split_whitespace().next().filter(|path| path.starts_with('/'))?;
            Some((soname.to_owned(), PathBuf::from(path)))
        })
        .collect())
}

/// The installed Debian package that owns `library`, trying both sides of the /usr merge.
fn owning_package(library: &Path) -> Option<String> {
    let path = library.to_string_lossy();
    let mut candidates = BTreeSet::from([path.to_string()]);
    if let Ok(real) = library.canonicalize() {
        candidates.insert(real.to_string_lossy().into_owned());
    }
    if let Some(rest) = path.strip_prefix("/usr/lib/") {
        candidates.insert(format!("/lib/{rest}"));
    }
    if let Some(rest) = path.strip_prefix("/lib/") {
        candidates.insert(format!("/usr/lib/{rest}"));
    }
    candidates.iter().find_map(|candidate| {
        let owner = util::output(Command::new("dpkg-query").args(["-S", candidate])).ok()?;
        owner.split(':').next().map(|name| name.trim().to_owned())
    })
}

/// Packages for libraries the binaries load with dlopen.
///
/// dpkg-shlibdeps only reads ELF NEEDED entries, so the X11 and Wayland libraries
/// winit and xkbcommon open at run time produce no dependency at all. Without them
/// a clean installation opens no window.
fn dlopen_dependencies(binaries: &[PathBuf]) -> Result<BTreeMap<String, String>> {
    let cache = loader_cache()?;
    let mut packages = BTreeMap::new();
    for binary in binaries {
        let linked = linked(binary)?;
        for soname in sonames(&fs::read(binary)?) {
            if linked.contains_key(&soname) {
                continue;
            }
            let Some(path) = cache.get(&soname) else {
                println!(
                    "Note: {soname} is named in {} but not installed here, so it is not declared.",
                    binary.display()
                );
                continue;
            };
            let package = owning_package(path).with_context(|| {
                format!(
                    "{soname} is opened at run time but no installed package owns {}; \
                     refusing to create an under-declared .deb.",
                    path.display()
                )
            })?;
            packages.insert(package, soname);
        }
    }
    Ok(packages)
}

/// The x86-64 libraries in the dynamic loader's cache, by soname; the first entry wins,
/// as it does for the loader.
fn loader_cache() -> Result<BTreeMap<String, PathBuf>> {
    let ldconfig = util::which("ldconfig").context("ldconfig is missing; it comes with libc-bin")?;
    let listing = util::output(Command::new(ldconfig).arg("-p"))?;
    let mut cache = BTreeMap::new();
    for line in listing.lines() {
        let Some((entry, path)) = line.trim().split_once(" => ") else {
            continue;
        };
        let Some((soname, kind)) = entry.split_once(' ') else {
            continue;
        };
        if kind.starts_with("(libc6,x86-64") {
            cache.entry(soname.to_owned()).or_insert_with(|| PathBuf::from(path));
        }
    }
    Ok(cache)
}

/// Every `lib<name>.so.<major>` in a binary: the names it may pass to dlopen.
fn sonames(binary: &[u8]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = binary;
    while let Some(start) = rest.windows(3).position(|window| window == b"lib") {
        let candidate = &rest[start..];
        let length = soname_length(candidate);
        if let Some(length) = length {
            found.insert(String::from_utf8_lossy(&candidate[..length]).into_owned());
        }
        // Without a match, all of "lib" can be skipped: it cannot start again inside itself.
        rest = &candidate[length.unwrap_or(3)..];
    }
    found
}

/// The length of `lib[A-Za-z0-9_+-]+\.so\.[0-9]+` at the start of `bytes`, if it is there.
fn soname_length(bytes: &[u8]) -> Option<usize> {
    let name = |byte: &&u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-');
    let stem = 3 + bytes[3..].iter().take_while(name).count();
    if stem == 3 || !bytes[stem..].starts_with(b".so.") {
        return None;
    }
    let major = bytes[stem + 4..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    (major > 0).then_some(stem + 4 + major)
}

/// The hicolor PNGs in `dir`, named by their pixel size.
fn icons_in(dir: &Path) -> Result<BTreeMap<u32, PathBuf>> {
    let mut icons = BTreeMap::new();
    for entry in fs::read_dir(dir).with_context(|| format!("Could not read {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "png") {
            let size = path.file_stem().and_then(|stem| stem.to_str()?.parse().ok());
            let size = size.with_context(|| format!("{} is not named after its size", path.display()))?;
            icons.insert(size, path);
        }
    }
    Ok(icons)
}

/// The package's file system tree. Every mode is set here, never taken from the
/// umask or the checkout.
struct Stage(PathBuf);

impl Stage {
    fn write(&self, relative: &str, contents: &[u8], mode: u32) -> Result<PathBuf> {
        let path = self.0.join(relative);
        let parent = path.parent().expect("staged files are inside the stage");
        fs::create_dir_all(parent).with_context(|| format!("Could not create {}", parent.display()))?;
        fs::write(&path, contents).with_context(|| format!("Could not write {}", path.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
        Ok(path)
    }

    fn copy(&self, source: &Path, relative: &str, mode: u32) -> Result<PathBuf> {
        let contents = fs::read(source).with_context(|| format!("Could not read {}", source.display()))?;
        self.write(relative, &contents, mode)
    }

    fn copy_tree(&self, source: &Path, relative: &str) -> Result<()> {
        for entry in fs::read_dir(source).with_context(|| format!("Could not read {}", source.display()))? {
            let path = entry?.path();
            let name = path
                .file_name()
                .expect("directory entries have names")
                .to_string_lossy();
            let target = format!("{relative}/{name}");
            if path.is_dir() {
                self.copy_tree(&path, &target)?;
            } else {
                self.copy(&path, &target, 0o644)?;
            }
        }
        Ok(())
    }

    /// The installed files, relative to the stage and sorted; DEBIAN is not installed.
    fn files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for path in walk(&self.0)? {
            let relative = path.strip_prefix(&self.0)?;
            if path.is_file() && !relative.starts_with("DEBIAN") {
                files.push(relative.to_path_buf());
            }
        }
        files.sort();
        Ok(files)
    }

    fn set_directory_modes(&self) -> Result<()> {
        for path in walk(&self.0)?.into_iter().chain([self.0.clone()]) {
            if path.is_dir() {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
            }
        }
        Ok(())
    }
}

/// Every file and directory below `dir`.
fn walk(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("Could not read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            paths.extend(walk(&path)?);
        }
        paths.push(path);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_versions_must_match_exactly() {
        let scratch = TempDir::new("winrdp-package-version").unwrap();
        let binary = scratch.path().join("report-version");
        let expected = format!("Win RDP {VERSION}");
        for (reported, accepted) in [
            (expected.clone(), true),
            (format!("Win RDP 1{VERSION}"), false),
            (format!("{expected}-dev"), false),
            (format!("unrelated-program {VERSION}"), false),
        ] {
            fs::write(&binary, format!("#!/bin/sh\nprintf '%s\\n' '{reported}'\n")).unwrap();
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
            assert_eq!(check_version(&binary, &expected).is_ok(), accepted, "{reported}");
        }
    }

    #[test]
    fn finds_the_sonames_a_binary_names() {
        let binary = b"\0libxkbcommon-x11.so.0\0xlibX11.so.6.4\0libfoo.libbar.so.2\0lib.so.1\0libz.so.\0";
        let found: Vec<_> = sonames(binary).into_iter().collect();
        assert_eq!(found, ["libX11.so.6", "libbar.so.2", "libxkbcommon-x11.so.0"]);
    }
}
