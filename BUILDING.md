# Build Win RDP on Linux

Win RDP builds a binary Debian package containing the launcher and the IronRDP
session window. Build as a normal user on the oldest Debian/Ubuntu-based
distribution you intend to support; packages built against newer system libraries
may not run on older distributions.

## Get the source

```sh
git clone --recurse-submodules https://github.com/AKolenda/winrdp.git
cd winrdp
```

For an existing checkout, initialize the pinned IronRDP fork before building:

```sh
git submodule update --init --recursive
```

The fork lives at https://github.com/AKolenda/ironrdp-winrdp, on the `winrdp`
branch. Use the committed submodule revision for reproducible builds.

## Install dependencies

Install a current stable Rust toolchain through [rustup](https://rustup.rs/), and a
C toolchain for it to link with (`sudo apt install build-essential`); the IronRDP
fork uses edition 2024. Then:

```sh
cargo xtask install-deps
cargo --version
rustc --version
```

`cargo xtask` runs the project's build, packaging and release tasks, which are Rust
too (`xtask/`); no Python, Node or Tauri CLI is needed. `install-deps` uses apt and
requests sudo only to install packages.

Everything is Rust: there is no C or C++ engine and no Qt or FreeRDP to install.
The launcher is a native window and no longer needs GTK or WebKitGTK. The helper
installs the ALSA and zlib development packages the executables link, the
Wayland, X11 and xkbcommon libraries both windows open at run time, and CMake, which
a Rust audio crate uses to build the Opus codec it bundles. Keep the committed
`Cargo.lock`.

## Build and install

```sh
cargo xtask preflight
cargo xtask deb
sudo apt install ./dist/winrdp-next_0.8.1_amd64.deb
winrdp-next
```

Use the filename printed by the build on other architectures. The default build
uses four jobs; set `WINRDP_JOBS=2` on lower-memory systems. Both Rust executables
build with `--locked`. The build runs the workspace's Rust tests, verifies executable
link dependencies, packages both executables, and writes SHA-256 checksums next
to the Debian package. Build logs are under `logs/`. To package executables you
have already built, run `cargo xtask package`.

The package installs `winrdp-next` and `winrdp-session` and bundles no native
libraries; glibc, ALSA, the X11 and Wayland libraries and graphics drivers come from
system packages.
Review [third-party notices](docs/THIRD-PARTY.md) before publishing binary releases.
Downloads should be attached to [GitHub Releases](https://github.com/AKolenda/winrdp/releases).

## Verify changes

```sh
cargo install cargo-machete --version 0.9.2 --locked
cargo xtask check
cargo xtask check-release
cargo xtask notices --check
cargo xtask deb
```

`check` runs rustfmt, unused-dependency analysis, Clippy with warnings treated as
errors, Rust documentation checks and the workspace tests. It uses the same
commands as CI. See [development guidelines](docs/DEVELOPMENT.md) for the module
boundaries and the policy for adding or suppressing checks.

After a dependency change, `cargo xtask notices` regenerates the third-party notice
inventory. `cargo xtask` with no command lists every task.

A successful build does not validate live RDP behavior. Before a release, test
new and saved computers, invalid credentials, resizing and fullscreen, reconnect,
clipboard in both directions, and TCP fallback when UDP is unavailable. Verify
clipboard text on X11 and Wayland separately. Test any advertised image, audio,
microphone or printer support on an actual supported Windows host.

## Troubleshooting

- Missing submodule sources: run `git submodule update --init --recursive`.
- Missing development libraries: run `cargo xtask preflight` and use its errors to
  identify packages.
- Session binary missing: build both executables with `cargo xtask deb`. For
  development only, `WINRDP_SESSION_BIN` can select another session executable.
- Native window controls fail: try `WINRDP_SYSTEM_FRAME=1 winrdp-next`.
- Compare TCP and UDP: launch with `WINRDP_UDP=0` or `WINRDP_UDP=1`.
- Graphics pipeline: `WINRDP_EGFX=0` forces the legacy bitmap path.
- Build fails: inspect the final error in `logs/build-*.log`; logs may contain
  local paths or hostnames, so redact them before posting an issue.

To uninstall the package while retaining user settings:

```sh
sudo apt remove winrdp-next
```

## Runtime switches

The launcher sets these for every session it starts; they are listed here because
they are the only way to change what a session does without rebuilding. Names
beginning `WINRDP_` are read by the launcher and passed to the session binary; the
`IRONRDP_` names are what the session binary itself reads, so they are the ones to
use when running `winrdp-session` by hand.

| Variable | Default | What it does |
|---|---|---|
| `WINRDP_UDP` / `IRONRDP_UDP` | `1` | Reliable RDP-UDP transport. `0` keeps the session on TCP. A failed UDP bootstrap falls back to TCP on its own. |
| `WINRDP_UDP_OFFER` / `IRONRDP_UDP_OFFER` | `2` | RDP-UDP version to offer. Run by hand, the session binary offers version 3 unless this is set, and hosts that do not implement it never answer the SYN. |
| `WINRDP_EGFX` / `IRONRDP_EGFX` | `1` | The graphics pipeline (MS-RDPEGFX). `0` falls back to bitmap updates. |
| `WINRDP_FULLSCREEN` | from the connect dialog | Opens the session window borderless full screen. |
| `WINRDP_PRINTER` | unset | Offers a printer to the session: `default`, a CUPS destination name, or `folder:<dir>` to keep jobs as PostScript files. |
| `WINRDP_TITLE` | the saved computer name | The session window title, and the name the disconnect dialog asks about. |
| `WINRDP_STATUS_FILE` | set by the launcher | Where the session publishes its transport, and why it stopped. The launcher reads it for the tab badge and the failure message. |
| `WINRDP_SESSION_BIN` | the installed binary | Another session executable, for development. |
| `WINRDP_SYSTEM_FRAME` | unset | `1` uses the system window frame for the launcher window. |
| `IRONRDP_LOG` | `info,ironrdp_client=debug` | Tracing filter for the session log. |

### Reading a session log

- `RDP-UDP handshake complete version=2` — the tunnel came up.
- `session transport reliable_udp=true udp_version=2` — graphics are on the tunnel.
  The window title and the launcher tab show the same thing (`UDP v2` or `TCP`).
- `session perf transport="udp" fps=… kbps=… busy_pct=…` — one line per second,
  with decode time and totals; this is what the throughput comparison above uses.
