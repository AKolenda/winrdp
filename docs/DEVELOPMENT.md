# Development

Run `cargo xtask check` before submitting changes. It runs the same checks as CI:

- rustfmt for the three application workspace packages;
- cargo-machete for unused direct dependencies;
- Clippy for every workspace target, with warnings treated as errors;
- Rustdoc, including private items, with warnings treated as errors;
- workspace tests, including the launcher's simulated interaction tests.

Install the dependency checker with
`cargo install cargo-machete --version 0.9.2 --locked`.
Release metadata, dependency notices and the website have separate checks:
`cargo xtask check-release`, `cargo xtask notices --check`, and `cargo xtask website`.
These also run in CI. See [BUILDING.md](../BUILDING.md) for system dependencies.

## Responsibilities

| Area | Responsibility |
| --- | --- |
| `launcher/src/app.rs` | Window state and the explicit message dispatch. |
| `launcher/src/app/model.rs` | Dialog state, messages and shared connection options. |
| `launcher/src/app/computers.rs` | Selecting, editing and saving computers. |
| `launcher/src/app/connections.rs` | Starting sessions and applying their status updates. |
| `launcher/src/view/` | Simple and full layouts, dialogs, settings and shared controls. |
| `launcher/src/backend.rs` | Filesystem/process boundary and the in-memory test backend. |
| `launcher/src/library.rs` | Persisted profiles, validation and atomic library writes. |
| `session/src/app.rs` | Native window events and presentation orchestration. |
| `session/src/damage.rs` | Clipped rectangles, buffer-age history and pixel copies. |
| `session/src/drawing.rs` | Shared software drawing and text primitives. |
| `session/src/status.rs` | Session reports and connection-error classification. |
| `session/src/keymap.rs` | Physical-key to RDP scan-code mapping. |
| `xtask/src/` | Build, package, source checks, branding and website commands. |
| `xtask/src/notices/` | Dependency graph resolution and upstream notice retrieval. |

Keep state changes in the application layer and I/O in the backend. A failed save
must not replace the current library, change the layout, clear the selection or
open a session with older connection options. The simulated launcher tests can
exercise these failures without starting an RDP process or changing a real library.

Share connection-option state and controls between layouts. Keep reusable pixel
operations independent of the connection bar and dialogs. Avoid making an
abstraction generic unless there are callers that need that flexibility.

## Rendering contract

The session and the client share a persistent framebuffer. Notifications carry no
pixel data: updates accumulate in the frame until the window takes its dirty area.
Replacing a queued notification must not discard pending pixels.

Acquire compositor buffers before locking the shared framebuffer. Hold its lock
only while inspecting frame state and copying pixels; release it before drawing
overlays or presenting. Compositor waits must not block the protocol thread.

Damage history tracks both buffer age and desktop/window dimensions. Resizing,
unknown buffer contents and expired history require a full copy. Overlay damage
includes old and new bounds. Text rendering clips against both the logical
surface and the actual backing buffer. Rectangle drawing requires callers to
provide bounds that fit the surface; the bar and modal tests cover tiny windows.

## Static-analysis policy

The application workspace forbids unsafe code and ignored `must_use` results.
Clippy also rejects debug macros and placeholder implementations, and checks for
unchecked unwraps, explicit panics, redundant clones and avoidable collections.
Tests may panic or unwrap fixture setup; production code must handle runtime
failures. An `expect` is appropriate only for a documented invariant, such as
formatting into a `String`, not an operating-system operation that can fail.

Fix findings at their source. Keep necessary lint exceptions local and explain
the invariant or tool limitation. Avoid blanket allowances and broad dependency
ignore lists. The `md-5` dependency is named `md5` in the manifest to match its Rust
import, so dependency analysis works without hiding it.

Clippy's full pedantic set is useful for exploratory review but is not a blanket
gate: independent sharing options are naturally booleans, and UI event dispatch
can be longer than a numeric threshold. Evaluate those findings against behavior,
ownership and clarity. Passing static checks does not replace reviewing error
paths or testing the application.

## IronRDP changes

The submodule is a separate workspace with its own pinned toolchain, style rules
and CI checks. Read its `AGENTS.md`, `ARCHITECTURE.md` and `STYLE.md` before editing.
Run its relevant tests and `cargo xtask check lints -v` from the submodule.
The application's engine CI job checks this exact pinned revision with fork-wide
formatting and Clippy, then runs the core, extra, UDP and printer test suites.
It explicitly runs `cargo test -p ironrdp-client --features rustls,all --lib
--locked` as well, since that crate disables automatic library-test discovery.

The shared `ironrdp-autodetect` responder has no I/O; drivers supply timestamps and
the number of bytes received. Preserve its state across activation so a network
measurement is neither reset nor counted twice. Wire/state tests live in
`ironrdp-testsuite-core`; client framebuffer tests live in
`ironrdp-testsuite-extra`. Some library targets disable in-file tests, so verify
that a command actually runs the tests you intend.

If sharing a Cargo target directory between worktrees, check the paths printed by
xtask. Its compiled workspace path can belong to the previous checkout. Rebuild
the xtask package or use a separate target directory before relying on the result.

Publish the fork revision before publishing an application commit that pins it.
Keep live Windows and Wayland validation separate from simulated and loopback
results in [RELEASE-VALIDATION.md](RELEASE-VALIDATION.md).
