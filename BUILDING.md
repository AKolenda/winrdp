# Build Win RDP on Linux

Win RDP builds a binary Debian package containing the Tauri launcher and the
IronRDP session window. Build as a normal user on the oldest Debian/Ubuntu-based
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

```sh
bash scripts/install-build-deps.sh
cargo --version
rustc --version
```

The dependency helper uses apt and requests sudo only to install packages. Rust
is installed separately through [rustup](https://rustup.rs/). Use a current stable
Rust toolchain; the IronRDP fork uses edition 2024. No Node or Tauri CLI is needed
for the desktop build: the launcher frontend is plain HTML, CSS, and JavaScript.

The native bridge also needs Qt >= 6.4 and FreeRDP >= 3.31 development libraries.
A suitable private runtime under `~/.local/share/velordp/runtimes` is discovered
automatically. A clean contributor machine must provide these dependencies from
system packages or a compatible runtime; the apt helper does not build them.
If your distro ships an older FreeRDP, build or install a suitable version first.
To choose a runtime explicitly:

```sh
export WINRDP_RUNTIME=/path/to/qt-freerdp-runtime
```

The helper also installs the ALSA, Wayland, xkbcommon and XCB development packages
the standalone session needs. Keep both committed Cargo lockfiles.

## Build and install

```sh
bash scripts/build-deb.sh --check
bash scripts/build-deb.sh
sudo apt install ./dist/winrdp-next_0.7.7_amd64.deb
winrdp-next
```

Use the filename printed by the build on other architectures. The default build
uses four jobs; set `WINRDP_JOBS=2` on lower-memory systems. Both Rust executables
build with `--locked`. The build runs the native C++ tests, verifies executable
link dependencies, packages both executables, and writes SHA-256 checksums next
to the Debian package. Build logs are under `logs/`.

The package installs `winrdp-next` and `winrdp-session`. Libraries from a selected
private runtime are included; GTK, WebKitGTK, glibc and graphics drivers come from
system packages. Review [third-party notices](docs/THIRD-PARTY.md) before publishing
binary releases, especially packages containing a private Qt/FreeRDP runtime.
Downloads should be attached to [GitHub Releases](https://github.com/AKolenda/winrdp/releases).

## Verify changes

```sh
python3 tests/recovery_tests.py
bash scripts/build-deb.sh
```

A successful build does not validate live RDP behavior. Before a release, test
new and saved computers, invalid credentials, resizing and fullscreen, reconnect,
clipboard in both directions, and TCP fallback when UDP is unavailable. Verify
clipboard text on X11 and Wayland separately. Test any advertised image, audio,
microphone or printer support on an actual supported Windows host.

## Troubleshooting

- Missing submodule sources: run `git submodule update --init --recursive`.
- Missing development libraries: use the preflight errors to identify packages.
  Do not mix incompatible distribution repositories to obtain FreeRDP.
- Session binary missing: build both executables with the build script. For
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
| `WINRDP_ENGINE` | unset | `freerdp` uses the classic in-window engine instead of an IronRDP session window. |
| `WINRDP_TITLE` | the saved computer name | The session window title, and the name the disconnect dialog asks about. |
| `WINRDP_STATUS_FILE` | set by the launcher | Where the session publishes its transport, and why it stopped. The launcher reads it for the tab badge and the failure message. |
| `WINRDP_SESSION_BIN` | the installed binary | Another session executable, for development. |
| `WINRDP_SYSTEM_FRAME` | unset | `1` uses the system window frame for the launcher window. |
| `IRONRDP_LOG` | `info,ironrdp_client=debug,ironrdp_rdpeudp_tokio=debug` | Tracing filter for the session log. |

### Reading a session log

- `RDP-UDP handshake complete version=2` — the tunnel came up.
- `session transport reliable_udp=true udp_version=2` — graphics are on the tunnel.
  The window title and the launcher tab show the same thing (`UDP v2` or `TCP`).
- `session perf transport="udp" fps=… kbps=… busy_pct=…` — one line per second,
  with decode time and totals; this is what the throughput comparison above uses.
