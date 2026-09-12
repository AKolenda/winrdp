Win RDP 0.7.5 is the first release under the GNU Affero General Public License v3.0.

Changes:
- Relicensed Win RDP's first-party source and packaged application under AGPL-3.0-only while preserving the licenses and notices of third-party components.
- Added a guided product tour to winrdp.app with animated, Figma-style cursors showing how to enter an address, choose a saved computer, and connect.
- Added a direct website download link to this release.
- Simplified release-facing version and package descriptions now that the public repository is established.

The application behavior is unchanged from the 0.7.4 candidate. Existing validation covers launcher behavior, clipboard text over TCP/X11, native core tests, packaging checks, and release builds. See `docs/RELEASE-VALIDATION.md` for the recorded scope and remaining platform gaps.

This is an early-access Linux x86_64 release. Native Wayland clipboard, live image transfer, UDP regression coverage, redirected audio/microphone/printer behavior, and clean-distribution installation still need broader validation. Please report the distribution, desktop session, and transport when filing an issue.
