# Release validation — 2026-09-13

The 0.7.6 release contains the current simple/full launcher, native session
controls, connection/clipboard patches, AGPL-3.0-only project licensing, and the
guided website tour. It adds two behaviour changes over 0.7.5: Settings no longer
open at startup, and a refused connection is reported instead of closing the
session window silently. This records actual checks rather than treating
compilation as a live protocol test.

| Area | Result |
| --- | --- |
| Launcher | 23 Chromium checks with mocked native commands passed: blank optional names, address retention, draft cancellation, saved options, IP matching, and no automatic inbound sharing. |
| Credential errors | Compiled actual command helper; failing child arguments and stderr cannot expose the test password. |
| Native core | 2 CTest tests passed. |
| Packaging recovery | 17 Python tests passed; version metadata consistent at 0.7.6. |
| Connection failures | 8 Rust tests passed covering the failure classification, the NTSTATUS wording, JSON escaping of server-supplied text, and the unchanged transport record. |
| Live refused sign-in | Session binary against a Windows test host with a deliberately wrong password: CredSSP returned STATUS_LOGON_FAILURE, the session published `reason: credentials` and exited 76. Repeated through the rebuilt launcher on X11: the toast and the Connect dialog both reported "The user name or password is incorrect." and the dialog reopened focused and empty. |
| Live unreachable host | TCP connect refused on a closed port produced `reason: network` with the engine's error text, not a credentials message. |
| Startup | The rebuilt launcher opened straight to the computer list with a library whose stored `openSettings` is still `true`, confirming the retired preference is ignored and old libraries still load. |
| Builds | Launcher and standalone session 0.7.6 release builds passed with locked dependency resolution. |
| Clipboard backend | 14 Rust tests passed, including handshake timing, stale responses, timeout recovery and image conversion. Targeted clippy passed. |
| Live clipboard | Linux X11 session to a Windows test host over TCP: local text pasted exactly; remote edited text copied back exactly. Physical Ctrl shortcuts exercised. Test document discarded. |
| Sharing preferences | Effective RDP configuration confirmed clipboard disabled and audio disabled when those options are off. |
| Package | Both binaries included, dependencies resolved, relocated library checks passed, SHA-256 file produced. |
| Website | Desktop 1440 px and mobile 390 px layouts rendered without console errors; the CSS-only guided cursor tour and release link were verified locally. |

## Known validation gaps

- Native Wayland clipboard, live image transfer and file clipboard transfer were
  not verified in this run. File clipboard transfer is not supported by the Linux backend.
- The live regression test for the 0.7.6 changes used TCP. A fresh RDP-UDP regression
  run was completed afterwards and is recorded in the 2026-09-18 addendum below; a
  comparison on a lossy or high-latency link is still missing.
- Microphone, printer redirection, remote audio output, and a clean-distribution
  installation still require validation before making broad compatibility claims.
- Rust dependency inventory covers 640 packages and every one of them now carries
  collected upstream licence text; see the 2026-09-18 addendum below for how the last
  nineteen were obtained and for the three documented exceptions.
- The private Qt/FreeRDP runtime has source archives and local FreeRDP patches recorded;
  the exact installed shared-library build configuration is not attested.
- The installer was built and inspected, but not installed over the system package:
  this environment requires an administrator password for installation. The launcher and
  session binaries were run from the build tree for the checks recorded above.
- A successful sign-in was not re-run for this release: the failure paths were exercised
  live, and the unchanged connected-session transport record is covered by a unit test
  rather than by a fresh live desktop.

The original private repository remains private. The public repository is a new
source snapshot with independent Git history, not a fork or a rewritten copy of
that private history. Internal handoff notes and real desktop screenshots are
excluded. The audited IronRDP dependency remains a normal submodule pinned to its
public `winrdp` branch revision.

## Addendum — 2026-09-18

Checks run after the 0.7.6 release, against the released source and the 0.7.6 binaries.

| Area | Result |
| --- | --- |
| Dependency notices | All 640 Rust packages now carry collected upstream licence text (previously 621). The nineteen crates that publish no licence file inside their crates.io archive were collected verbatim from each crate's own upstream repository, pinned to the commit recorded in the archive's `.cargo_vcs_info.json`, and every one of those packages records that provenance under `notice_source`. Each file was re-downloaded from the pinned revision and compared byte for byte against what is stored here. |
| Notice exceptions | Three cases needed an explanation rather than a straight copy and are listed under `notice_exceptions`, with the reasoning repeated in a `NOTICE.md` beside the affected texts: `selectors 0.36.1` (MPL-2.0; no licence text exists in the archive or upstream, so the canonical Mozilla text is reproduced), `dasp_sample 0.11.0` (upstream ships the short Apache application notice under the name `LICENSE-APACHE`; the MIT half of the disjunction is complete), and `drm-fourcc 2.2.0` (the published revision carried no licence file; the MIT text upstream added later is reproduced). |
| Notice regression guard | `tests/notice_checks.py` verifies the manifest against the tree — every referenced file present with a matching SHA-256, no unreferenced files, no package left without a notice, every `notice_source` URL pinned to a full revision, and the recorded IronRDP revision matching the submodule pointer. It runs in CI alongside the existing source checks, and was itself checked by tampering with a licence file and by adding a stray file. |
| RDP-UDP regression | Six live sessions from this Linux build (X11) to two Windows 11 hosts on the local network. Every run negotiated RDP-UDP version 2, reported `reliable_udp=true` within 0.1 s of connecting, decoded graphics-pipeline frames over the reliable tunnel, and exited without an active-session error. The TCP control runs reported `reliable_udp=false` as expected. |
| UDP against TCP | Four paired 34 s runs (UDP, TCP, TCP, UDP) driving identical scripted activity on the remote desktop: mean 7.8–9.8 fps on UDP against 7.1–9.2 fps on TCP, 0.13–0.49 MB against 0.15–0.22 MB, 1.5–4.5 ms decode time per frame against 2.1–7.8 ms. Run-to-run variation on one transport is larger than the difference between transports, so these runs confirm parity on a clean LAN rather than an advantage either way; frame rate is bounded by how much the remote screen changes. |

| 0.7.7 package | Rebuilt with the dependency fix and compared against the released 0.7.6 package: the eight bundled-library dependencies are gone (`qt6-base-abi (= 6.4.2)`, four `libqt6*`, two FreeRDP, WinPR) and the six X11/Wayland libraries the session opens at run time are declared, with no duplicates. Restoring the old ordering now fails the build rather than producing that package silently. Installation on a clean distribution is still untested here. |
| 0.7.7 checks | 14 session unit tests, clippy clean on the session crate with `-D warnings`, 17 packaging recovery tests, the command-failure redaction check, the dependency-notice check, the release-metadata check (now covering the install commands, the download link and these notes), the website build, AppStream and desktop-entry validation, and 29 launcher checks in Chromium including the refusal path, the "last opened" stamp and the dialog-reuse guard. Every one of these now runs in CI; before this release only the Python and `node --check` steps did. |
| 0.7.7 session behaviour | Verified against the built binary: an unusable destination now exits 78 and publishes `{"state":"failed","reason":"config",…}` to the status file instead of exiting 1 with an empty log, and `--version` still prints and exits 0 so packaging is unaffected. |

Still open after this addendum: native Wayland clipboard, live image and file clipboard
transfer, microphone, printer and remote audio behaviour, clean-distribution installation,
the exact Qt/FreeRDP runtime build configuration, and a UDP comparison on a lossy or
high-latency link.

One observation worth recording rather than fixing: on the first connection to a host that
had been idle, the server ran repeated network auto-detect cycles for about 14 s, roughly
20 MB of traffic that produced no decoded frames, while `ironrdp-session` logged the
bandwidth-measure PDUs as "not yet implemented" (`crates/ironrdp-session/src/x224/mod.rs`).
Only RTT requests are answered today. Later connections to the same host showed none of it,
so the behaviour could not be reproduced on demand and no change was made.

## Provisional 0.8.0 integration — 2026-09-26

The native Rust launcher, shared framebuffer, session-module cleanup and shared
network-autodetection responder are being integrated. The results below were
completed on the protocol branch at fork commit
`deea1ad7de4a7c9c8f9b90ecba16e4e0d73eb717`; they do not certify the final combined
build. No live Windows host or existing user session was used for these checks.
The earlier observation that only RTT requests receive responses is superseded
by this implementation; its relationship to the historical cold-connection delay
has not been established by a live test.

| Protocol check | Completed result |
| --- | --- |
| Core test suite | `cargo test -p ironrdp-testsuite-core --locked`: 1,657 passed, 2 ignored. |
| Focused autodetection tests | 84 passed, covering wire/state behavior, byte accounting, saturation, mismatched stop requests and activation-state transfer. |
| Strict Clippy | Passed with `--all-targets --no-deps --locked -- -D warnings` for `ironrdp-autodetect`, `ironrdp-connector`, `ironrdp-session`, `ironrdp-fuzzing` and `ironrdp-testsuite-core`. |
| Server example | Strict Clippy passed for the server example with `cliprdr,connector,rdpsnd,server` features. |
| Minimal client build | `cargo check -p ironrdp-client --features rustls --locked` passed with an existing unused `reply_tunnel` warning. This was not a warning-free client Clippy result. |
| `no_std` responder | `cargo check -p ironrdp-autodetect --no-default-features --locked` passed. |
| Fuzz target | `cargo check --manifest-path fuzz/Cargo.toml --bin autodetect_state --locked` passed. The target compiled; no libFuzzer campaign was run. |
| Formatting | Rustfmt checks passed for the 14 changed Rust files; `git diff --check` passed. |

The completed local framebuffer comparison and its separate test results are
recorded in [FRAMEBUFFER-PERFORMANCE.md](FRAMEBUFFER-PERFORMANCE.md). Those are
synthetic loopback TCP/X11 measurements, not Windows or Wayland validation.

Final combined application/fork static checks, regenerated dependency notices,
release builds, package inspection and integrated smoke checks remain pending
at this checkpoint. No final installation, deployment or live protocol result is
claimed here. Windows-host bandwidth measurement/reactivation, native Wayland,
redirected microphone/printer/audio, live image clipboard transfer and a clean
supported-distribution installation remain validation gaps; Linux file clipboard
transfer is unsupported. A lossy/high-latency UDP comparison is also still open.

## Combined 0.8.0 verification — 2026-09-26

The combined application uses IronRDP revision
`0427e483817711feed7a271082e6afb0fe86c1e2`. The following checks ran after the
launcher, renderer, autodetection and static-analysis changes were integrated.

| Check | Result |
| --- | --- |
| Application quality gate | `cargo xtask check` passed: formatting, unused direct dependencies, strict Clippy for all targets, private-item Rustdoc with warnings denied, and 99 tests (50 launcher, 33 session, 16 tooling). The screenshot-export test is intentionally ignored by the normal test run. |
| Fork quality gate | `cargo xtask check fmt -v` and `cargo xtask check lints -v` passed using the fork's pinned Rust 1.94.1 toolchain. The latter checks the entire workspace and all targets with `helper,__bench` features and warnings denied. |
| Combined fork tests | `cargo test --locked -p ironrdp-testsuite-core -p ironrdp-testsuite-extra -p ironrdp-rdpeudp -p ironrdp-rdpdr-native` passed: 1,657 core tests (2 ignored), 109 extra tests, 195 UDP tests and 6 printer tests. The Windows-only printer test target ran no tests on Linux. |
| Client shutdown regressions | `cargo test -p ironrdp-client --features rustls,all --lib` passed 47 tests. New cases distinguish expected EOF/reset/aborted errors after local shutdown from unsolicited closure, decoding failures and other read errors. The client disables automatic library-test discovery, so CI requests this target explicitly. |
| Responder build boundaries | The final fork passed `cargo check -p ironrdp-autodetect --no-default-features --locked` and `cargo check --manifest-path fuzz/Cargo.toml --bin autodetect_state --locked`. This verifies the `no_std` build and fuzz-target compilation, not a fuzz campaign. |
| Dependency notices | Regenerated and verified: 530 packages, 982 licence files, and the committed fork revision matches the inventory. |
| Release metadata | `cargo xtask check-release --tag v0.8.0`, AppStream validation and desktop-entry validation passed. |
| Package | `cargo xtask deb` built both release binaries and the amd64 package. Exact binary versions, dynamic dependencies and relocated executables were checked by the packager. The package checksum verified: `e3edf082c53f3b0cf673d931e1acabbb795eec18dd53a9145165096c4a213663`. No system package was installed. |
| Packaged desktop smoke | The extracted final package launched its sibling session executable with an isolated profile on Xephyr/Openbox X11. Quick connect, rendered frames, keypad Enter/Divide/Menu extended scan codes, fullscreen, bar hide/reveal, window padding/clipping and dialog cancel passed. Requested Disconnect produced `connected -> closed`. A closed loopback port produced `failed/network`; stopping the owned server without requesting close produced `connected -> failed/session`. The server logged 227 RTT and 17 bandwidth measurements. |
| Website | Rebuilt with `cargo xtask website`. Headless Chromium at 320, 390, 768 and 1,440 pixels reported no page, console or request errors, no horizontal overflow and all images decoded. The download targets GitHub's latest release. Narrow navigation wrapping was fixed and rechecked. Wrangler's deployment dry run passed with 11 static assets. |
| Review | Independent review covered launcher failure state, shared rendering, protocol response state, package-version checks, the fork's conditional simplifications and publication history. Newly published source/history was checked for private credentials and attribution. |

CI runs the application's quality gate and separately checks the exact pinned
fork with the same formatting, Clippy and test-suite commands above.

The packaged smoke used TCP and a synthetic loopback server. That server does not
complete dynamic desktop resizing: a resize timeout exercised reconnection and
local presentation, not a successful Windows resize negotiation. Native Wayland,
Windows-host measurement/reactivation, redirected devices, live image clipboard
and a clean-distribution installation remain unverified for this build.

### Publication follow-up

CI exposed a dependency-notice parser issue when `CARGO_TERM_COLOR=always`
colored Cargo's repeated-node marker. The failure was reproduced locally;
requesting `--color never` for the machine-read dependency tree fixes it without
changing or weakening the inventory. The forced-color notice check, xtask
formatting, strict Clippy and all 16 tooling tests pass after the fix.

The website was deployed to <https://winrdp.app/> as Worker version
`41127004-0fa0-40e5-bcdd-847b48852db2` (deployment
`daa06e12-da99-4af7-a709-e181663591da`). Production checks at 320, 390, 768 and
1,440 pixels returned HTTP 200 with no overflow and all five image elements
loaded. Served CSS, logo and screenshots match the build; the custom missing-page
response returns HTTP 404 with a working home link. The latest-release link still
resolves to published release 0.7.7; the 0.8.0 package above was built locally.

Cloudflare injects an analytics beacon that the site's existing CSP blocks,
producing one console warning and one blocked request per page load. Page assets
and navigation work. Neither the CSP nor analytics settings were changed.

### Native Wayland check — 2026-09-27

The tagged 0.8.0 package (`07026073bc4516fcb110384a432be2fea13d59dc247c73388f2ab2cc049926c5`)
was extracted and run as native Wayland clients, with no X11 display, under a nested
Weston 13 compositor using its software (pixman) renderer. The launcher drew both
layouts' simple window and its dialogs. It started the packaged session, which
connected over TCP to the fork's example server, presented frames through `wl_shm`
with client-side decorations, reported `TCP`, and stamped "last opened". Closing the
session window ended it cleanly. A copy of a library written by 0.7.4 (five computers,
dark appearance, the retired `openSettings` preference) loaded unchanged. The server
does not complete dynamic resizing, so the Wayland window's first resize fell back to
reconnecting after 10 s, as on X11. Clipboard redirection was off: Weston offers no
data-control protocol and no X11 fallback there.

Not covered: GNOME Shell's own Wayland compositor, and any Windows host. This build
has not yet been connected to a Windows computer.
