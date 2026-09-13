Win RDP 0.7.6 fixes two things that made the client harder to live with than it should be.

Changes:
- Win RDP no longer opens Settings when it starts. The window goes straight to the computer list. The "Open Settings at startup" switch is gone; Settings stays one click away from the gear button, the sidebar, or Ctrl+,.
- A refused connection now says why. Previously a mistyped password closed the session window with no explanation at all. The session window reports the reason it stopped, the launcher shows it, and a wrong user name or password reopens the Connect dialog with "The user name or password is incorrect." above the password box.
- Sign-ins Windows refuses for a reason retyping cannot fix — a locked-out or disabled account, an expired password, an account that may not sign in over Remote Desktop — are named individually instead of being reported as a bad password.
- Unreachable computers, refusals from the server and protocol errors are reported in their own words, with the engine's error text underneath.
- Session windows now exit non-zero when a connection is refused or drops on an error, instead of always exiting successfully.

Existing behaviour is unchanged: saved computers, preferences and the transport badge are read and written exactly as before, and libraries saved by earlier versions load without change.

This is an early-access Linux x86_64 release. Native Wayland clipboard, live image transfer, UDP regression coverage, redirected audio/microphone/printer behavior, and clean-distribution installation still need broader validation. Please report the distribution, desktop session, and transport when filing an issue.
