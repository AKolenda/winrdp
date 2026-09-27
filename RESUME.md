# Resume — 2026-09-26

The 0.8.0 implementation is integrated. The completed changes and remaining
product limits are listed below. Current and historical verification records are in
[docs/RELEASE-VALIDATION.md](docs/RELEASE-VALIDATION.md).

## Completed implementation

- The launcher and build tools are native Rust. Launcher messages/state, computer
  editing, connection management and views have separate modules; both layouts
  share connection options and controls. Failed saves preserve selection and
  layout and stop connection attempts. Editing the selected computer in Full
  layout now refreshes the options used by Simple layout.
- Session event handling, drawing, status reporting and physical-key mapping have
  separate responsibilities. Numpad Enter, numpad divide and Menu mappings are
  fixed. Rendering uses an owned display handle and reports presentation errors.
- The client and session share a persistent framebuffer. Updates accumulate until
  painted; conversion and presentation copy changed regions, with buffer-age,
  resize and overlay restoration handling. Local synthetic TCP measurements and
  their limits are in [docs/FRAMEBUFFER-PERFORMANCE.md](docs/FRAMEBUFFER-PERFORMANCE.md).
- The fork's shared `ironrdp-autodetect` responder handles RTT and bandwidth
  measurements in connection, active-session and reactivation paths. Drivers
  supply timing and received-byte counts; the responder performs no I/O. Tests
  cover wire encoding, measurement state, byte accounting and activation transfer.
- Fork fixes restore `drain_output`'s `must_use` contract, use private printer
  spool files and move packet dumps and frequent UDP diagnostics to trace level.
- `cargo xtask check` and CI run formatting, unused-dependency analysis, Clippy,
  Rustdoc and workspace tests. Notice collection is split into graph and upstream
  retrieval modules. Package creation checks both binaries' exact versions.
  See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for boundaries and lint policy.

## Combined verification

- `cargo xtask check` passes all formatting, dependency, Clippy, Rustdoc and
  application test checks: 99 tests passed, one screenshot-export test ignored.
- The fork's full-workspace formatting and strict Clippy checks pass. Its core,
  extra, UDP and printer suites pass 1,967 tests, with two core tests ignored.
  Another 47 client tests cover shutdown and other client behavior. App CI runs
  these checks against the exact pinned fork revision.
- Dependency notices agree: 530 packages and 982 licence files. Release metadata,
  AppStream and desktop-entry validation pass. `cargo xtask deb` builds the real
  `winrdp-next_0.8.0_amd64.deb` package; no system installation was changed.
- The final packaged X11/TCP smoke passes connection, rendering, keyboard, window
  controls, clean disconnect and genuine-error reporting against an isolated
  loopback server. RTT and bandwidth responses were observed. Dynamic desktop
  resize on Windows remains unverified; the test server exercises reconnect fallback.
- Website checks pass at 320, 390, 768 and 1,440 pixels, including image decoding,
  no horizontal overflow and the latest GitHub release link.
- Earlier focused protocol checks also cover the responder's `no_std` build and
  compilation of its fuzz target. No sustained fuzz campaign was run.

## Remaining product and validation limits

- Native Wayland presentation/clipboard, Windows-host network measurement and
  resize behavior, live image clipboard transfer, microphone/printer/audio,
  clean-distribution installation and lossy/high-latency UDP comparisons still
  need validation. Linux file clipboard transfer remains unsupported.
- The iced launcher has no accessibility tree for screen readers. The retained
  `compatibility`, `graphics`, `keyboardLayout` and `allMonitors` profile fields
  round-trip older libraries but do not configure the session.
- Saving with 0.7.6 or later drops retired preferences; opening that library in
  0.7.5 is not supported. Preserve a backup before downgrading.
- A new demonstration capture is still needed for the session screenshot and its
  website copy. Upstream submission of the fork's protocol changes is separate
  work, including replacing environment switches with connector configuration.

## Prior owner follow-ups, not rechecked in this integration

- Rotate the test credential previously recorded in the private repository's
  handoff document; no credential value belongs in this repository.
- Remove or redirect the stale `winrdp.openfuel-monorepo.workers.dev` deployment,
  which the earlier audit found still linked to the private repository.
