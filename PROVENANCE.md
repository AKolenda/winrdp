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

## The classic engine

`engine/` is the Qt/FreeRDP client that predates the IronRDP work and still drives
in-tab sessions. Its core, profile and session sources were recovered unchanged
from `/usr/share/velordp/source` inside the `win-rdp_0.4.0_all.deb` package, along
with `tests/core_tests.cpp`, `tests/geometry_tests.cpp` and the original monitor
SVG. `native/` is the GTK bridge and C ABI between that engine and the launcher
shell, written for this project.

## The launcher

`src-tauri/` (the Tauri shell and its commands), `frontend/` (plain HTML, CSS and
JavaScript, no framework), `scripts/`, `packaging/` and `website/` are this
project's work.

## Third-party notices

`docs/THIRD-PARTY.md` records the bundled Qt and FreeRDP runtime with its source
archives and local patches. `docs/rust-dependency-notices.json` inventories every
Rust dependency with its registry checksum and source URL, and
`docs/licenses/rust/` holds their licence texts. `tests/notice_checks.py` verifies
that the inventory and the texts on disk agree.

## Licence

The project is AGPL-3.0-only; see `LICENSE`. Vendored and recovered code keeps the
licence it came with.
