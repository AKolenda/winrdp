# Third-party software

Win RDP's source is licensed under the GNU Affero General Public License v3.0;
see LICENSE. This is independent software,
not affiliated with Microsoft. The included monitor icon is the original project
asset, not a Microsoft application icon. No font files are bundled.

The source depends on:

- IronRDP (MIT OR Apache-2.0), through the fork at
  https://github.com/AKolenda/ironrdp-winrdp. Upstream:
  https://github.com/Devolutions/IronRDP. The package includes both license texts.
- FreeRDP and WinPR (Apache-2.0), linked natively, not rewritten in Rust.
  Source: https://github.com/FreeRDP/FreeRDP ; https://pub.freerdp.com/releases/
- Qt Core, Gui and Network (open-source LGPL/GPL/commercial options).
  Qt shared libraries can be bundled from the user's existing runtime; no Qt
  Widgets frontend is included in the Tauri binary. Use an appropriately licensed
  Qt build, retain notices and provide corresponding source/changes when required.
  Pinned classic source location: https://download.qt.io/archive/qt/6.11/6.11.2/submodules/
- GTK, GLib, Cairo and WebKitGTK from system packages (their respective licenses).
- Tauri and its Rust dependency graph. Exact transitive versions are recorded in
  `src-tauri/Cargo.lock` and `session/Cargo.lock`. Review third-party licenses
  before redistributing a built package.

The locally generated package is for evaluation. It is not a signed official
release and no complete redistribution/license-compliance audit has been done.
System GTK/WebKit, glibc, graphics drivers and fonts are not copied into it.

## Recorded development runtime

`runtime-source-manifest.json` records source-archive SHA-256 hashes and versioned
download links for the development runtime: Qt Base 6.11.2 and FreeRDP 3.31.1.
Qt Base's extracted source matched its downloaded archive. FreeRDP had local H.264
changes in two files; `runtime-patches/freerdp-3.31.1.patch` preserves those changes.
The package includes these records and the collected license/attribution texts in
`licenses/`. This directory includes the source trees' license inventory; inclusion
of a license text does not mean every optional upstream component is linked.

These records describe the inspected development runtime, not every runtime that
a contributor may select. Before publishing a binary, verify its actual linked
libraries against these sources, provide the matching source archives and build
configuration, and collect notices for its Rust dependencies. Both Cargo lockfiles
pin the Rust dependency versions and registry checksums. Package checksums and
source revision identifiers should accompany the GitHub release.


Rust normal/build dependency inventory for Linux x86_64 is recorded in
[rust-dependency-notices.json](rust-dependency-notices.json). Exact registry archive
checksums and source URLs are included. 621 of 640 packages have collected upstream
license/notice text under `licenses/rust/`; 19 missing upstream license texts are
explicitly listed. This inventory is not a completed binary redistribution audit.
