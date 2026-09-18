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
