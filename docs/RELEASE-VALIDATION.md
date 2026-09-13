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
- The live regression test used TCP. Historical UDP measurements are not a substitute
  for a fresh UDP regression run after these changes.
- Microphone, printer redirection, remote audio output, and a clean-distribution
  installation still require validation before making broad compatibility claims.
- Rust dependency inventory covers 640 packages; 19 upstream license texts remain
  missing and are listed in `docs/rust-dependency-notices.json`.
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
