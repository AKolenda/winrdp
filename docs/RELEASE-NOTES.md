Win RDP 0.7.4 is a Linux Remote Desktop client with a Tauri launcher and standalone IronRDP session windows.

Changes:
- New computer drafts keep the entered address and leave the optional name blank.
- Corrected saved-computer matching and clipboard/audio preference forwarding.
- Fixed Ctrl+C/Ctrl+V modifier delivery and clipboard initialization/timeout handling.
- Removed automatic inbound desktop sharing on launcher startup.
- Prevented command failures from disclosing sharing passwords.
- Added session packaging checks, checksums, dependency notices and release documentation.

Validation includes 23 launcher checks, 14 clipboard tests, native core and recovery tests, release builds, and a live bidirectional text clipboard test over TCP/X11. See docs/RELEASE-VALIDATION.md for the test scope and remaining gaps.

This is a draft candidate. Clean-distribution installation, native Wayland clipboard, live image sharing, audio/microphone/printer behavior and exact private-runtime build provenance remain release gates. Packages and SHA-256 checksums are attached here for review; the website links directly to GitHub Releases.
