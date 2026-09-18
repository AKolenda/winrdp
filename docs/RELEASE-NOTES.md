Win RDP 0.7.7 fixes the package itself, three ways a session could go wrong without saying so, and completes the third-party licence notices.

Changes:
- The .deb no longer demands the system's Qt 6 and FreeRDP. It bundles its own, but the dependency scan resolved them against the build machine and pinned `qt6-base-abi (= 6.4.2)`, so the package refused to install on anything but Ubuntu 24.04 and pulled in about 100 MB it never used. Eight such dependencies are gone, and the eight X11 and Wayland libraries the session window opens at run time — which no dependency named before — are now declared.
- The session window's close button works while a connection is still being made. It used to send the shutdown signal that only an established session reads, so the X button, the window manager's close and the connection bar's X all did nothing until the attempt finished or timed out.
- Clicks land where you point them when the window and the remote desktop are different sizes. The frame is drawn corner to corner, but the pointer was being scaled, so every click was offset for as long as the two disagreed — a moment after each resize, and permanently on a host that refuses to resize.
- A computer that answers but fails the secure handshake no longer says "check that it is switched on".
- A session that cannot start at all — a bad address, an unusable setting — now says so. It used to exit silently and leave the launcher pointing at an empty log.
- A refused sign-in no longer re-points a Connect dialog you are already typing into at a different computer.
- A session that dies after connecting is reported instead of vanishing.
- "Last opened" is recorded when the computer actually answers, not when Connect is pressed, so a refused sign-in no longer claims the computer was opened.
- Resizing a session window too small for the disconnect dialog no longer freezes it behind a dialog that cannot be drawn.

Third-party notices: all 640 Rust dependencies now carry their upstream licence text. The nineteen crates that ship no licence file inside their crates.io archive were collected from each crate's own repository, pinned to the commit recorded in the published archive, and `tests/notice_checks.py` now verifies the inventory against the files on every CI run.

Also in this release: CI compiles and tests the session crate, lints it with clippy, builds and runs the engine core tests, builds the website and validates the AppStream and desktop metadata — none of which ran in any workflow before. The release check now covers the install commands, the download link and the release notes, not just the manifests.

Validation for this release is recorded in docs/RELEASE-VALIDATION.md, including a fresh RDP-UDP regression run against two Windows 11 hosts and 29 launcher checks in Chromium. Native Wayland clipboard, live image transfer, redirected audio/microphone/printer behaviour and clean-distribution installation still need broader validation. Please report the distribution, desktop session, and transport when filing an issue.
