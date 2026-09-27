# Third-party software

Win RDP's source is licensed under the GNU Affero General Public License v3.0;
see LICENSE. This is independent software,
not affiliated with Microsoft. The app icon (the ribbon W) is an original project
asset, not a Microsoft application icon. No font files are bundled.

The source depends on:

- IronRDP (MIT OR Apache-2.0), through the fork at
  https://github.com/AKolenda/ironrdp-winrdp. Upstream:
  https://github.com/Devolutions/IronRDP. The package includes both license texts.
- iced (MIT) for the launcher window, winit and softbuffer for the session windows, and
  the rest of the Rust dependency graph. Exact transitive versions are recorded in the
  workspace `Cargo.lock`. Review third-party licenses before redistributing a built package.
- At run time, system libraries loaded from distribution packages: glibc, ALSA, and the
  X11/Wayland and xkbcommon libraries the windows open.

The locally generated package is for evaluation. It is not a signed official
release and no complete redistribution/license-compliance audit has been done.
System libraries, graphics drivers and fonts are not copied into it, and it bundles
no other native libraries: both programs are Rust, and the package holds only them,
their assets and these notices.

Rust normal/build dependency inventory for Linux x86_64 is recorded in
[rust-dependency-notices.json](rust-dependency-notices.json). Exact registry archive
checksums and source URLs are included. All 529 packages have collected upstream
license/notice text under `licenses/rust/`. Twenty crates ship no licence file inside
their crates.io archive; their texts were taken verbatim from the crate's own upstream
repository, pinned to the commit recorded in the archive's `.cargo_vcs_info.json`, and each
such package records that provenance under `notice_source`. Four crates needed an
explanation rather than a straight copy and are listed under `notice_exceptions`, with the
reasoning repeated in a `NOTICE.md` beside the affected texts. `cargo xtask notices`
regenerates the inventory from `Cargo.lock` and `cargo xtask notices --check` verifies
it. This inventory is not a completed binary redistribution audit.
