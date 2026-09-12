# Release validation — 2026-09-12

The 0.7.5 release contains the current simple/full launcher, native session
controls, connection/clipboard patches, AGPL-3.0-only project licensing, and the
guided website tour. Application behavior is unchanged from the live-tested
0.7.4 candidate. This records actual checks rather than treating compilation as
a live protocol test.

| Area | Result |
| --- | --- |
| Launcher | 23 Chromium checks with mocked native commands passed: blank optional names, address retention, draft cancellation, saved options, IP matching, and no automatic inbound sharing. |
| Credential errors | Compiled actual command helper; failing child arguments and stderr cannot expose the test password. |
| Native core | 2 CTest tests passed. |
| Packaging recovery | 15 Python tests passed; version metadata consistent at 0.7.5. |
| Builds | Launcher and standalone session 0.7.5 release builds passed with locked dependency resolution. |
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
  this environment requires an administrator password for installation.

The original private repository remains private. The public repository is a new
source snapshot with independent Git history, not a fork or a rewritten copy of
that private history. Internal handoff notes and real desktop screenshots are
excluded. The audited IronRDP dependency remains a normal submodule pinned to its
public `winrdp` branch revision.
