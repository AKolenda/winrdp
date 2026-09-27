# Source provenance

Where the code in this repository came from, so that a reader can tell written
work from recovered work from vendored third-party work.

## This repository

An independent source snapshot. Its Git history starts with the first public
release and is not a copy or a rewrite of the private repository the project was
developed in. Internal development notes and captures of private desktops are not
part of it.

## The RDP session window

`session/` is `winrdp-session`, this project's build of IronRDP's own viewer. It is
derived from `crates/ironrdp-viewer` in the vendored fork and keeps that code's
Apache-2.0/MIT licensing; the connection-failure reporting, the transport status
file, the full-screen connection bar (`bar.rs`), the disconnect dialog (`modal.rs`)
and the resize handling are this project's work.

## The RDP engine

`third_party/ironrdp/` is a Git submodule pinned to a revision of
[AKolenda/ironrdp-winrdp](https://github.com/AKolenda/ironrdp-winrdp), a fork of
[Devolutions/IronRDP](https://github.com/Devolutions/IronRDP). The fork carries the
MS-RDPEUDP transport, graphics over the reliable UDP tunnel, and the Linux
clipboard, printer and audio backends. The MS-RDPEUDP version 1/2 reliable data
transfer was contributed back and merged upstream as
[Devolutions/IronRDP#1919](https://github.com/Devolutions/IronRDP/pull/1919).
Upstream's licence files ship with the package as `IronRDP-LICENSE-MIT` and
`IronRDP-LICENSE-APACHE`.

## The retired classic engine

Releases up to 0.7.7 also carried `engine/`, the Qt/FreeRDP client that predates the
IronRDP work (recovered from the `win-rdp_0.4.0_all.deb` package), and `native/`, the
GTK bridge that let it draw a desktop in a launcher tab. Both were removed once every
session ran on IronRDP; git history keeps them. The launcher's computer-address and
profile rules were ported to Rust (`launcher/src/endpoint.rs`), with the engine's
address test cases.

## The launcher

`launcher/` is this project's work: a native iced window that replaced the earlier
Tauri shell and its HTML, CSS and JavaScript interface, keeping the same saved-computer
library format and behaviour. `packaging/` and `website/` are this project's work too.

## Third-party notices

`docs/THIRD-PARTY.md` lists what the source depends on; the package bundles no native
libraries. `docs/rust-dependency-notices.json` inventories every
Rust dependency with its registry checksum and source URL, and
`docs/licenses/rust/` holds their licence texts. `cargo xtask notices --check`
verifies that the inventory, `Cargo.lock` and the texts on disk agree.

## Licence

The project is AGPL-3.0-only; see `LICENSE`. Vendored and recovered code keeps the
licence it came with.
