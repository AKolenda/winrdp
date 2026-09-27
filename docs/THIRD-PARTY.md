# Third-party software

Win RDP's source is licensed under the GNU Affero General Public License v3.0;
see LICENSE. This is independent software,
not affiliated with Microsoft. The app icon (the ribbon W) is an original project
asset, not a Microsoft application icon. No font files are bundled.

The source depends on:

- IronRDP (MIT OR Apache-2.0), through the fork at
  https://github.com/AKolenda/ironrdp-winrdp. Upstream:
  https://github.com/Devolutions/IronRDP. The package includes both license texts.
- GTK, GLib, Cairo and WebKitGTK from system packages (their respective licenses).
- Tauri and its Rust dependency graph. Exact transitive versions are recorded in
  `src-tauri/Cargo.lock` and `session/Cargo.lock`. Review third-party licenses
  before redistributing a built package.

The locally generated package is for evaluation. It is not a signed official
release and no complete redistribution/license-compliance audit has been done.
System GTK/WebKit, glibc, graphics drivers and fonts are not copied into it, and
it bundles no other native libraries: both programs are Rust, and the package holds
only them, their assets and these notices.

Rust normal/build dependency inventory for Linux x86_64 is recorded in
[rust-dependency-notices.json](rust-dependency-notices.json). Exact registry archive
checksums and source URLs are included. All 640 packages have collected upstream
license/notice text under `licenses/rust/`. Nineteen crates ship no licence file inside
their crates.io archive; their texts were taken verbatim from the crate's own upstream
repository, pinned to the commit recorded in the archive's `.cargo_vcs_info.json`, and each
such package records that provenance under `notice_source`. Three crates needed an
explanation rather than a straight copy and are listed under `notice_exceptions`, with the
reasoning repeated in a `NOTICE.md` beside the affected texts. This inventory is not a
completed binary redistribution audit.
