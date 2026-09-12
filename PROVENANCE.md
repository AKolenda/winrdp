# Source provenance and recovery boundary

The preceding conversation linked `Win-RDP-Tauri-source.zip` despite failed
file-creation calls. There was no such ZIP in the mounted conversation files.
No Tauri Cargo.toml, tauri.conf.json, Rust source or native migration bridge was
preserved there. Screenshots of the 0.6 design and a 147-byte packaging report
were present; neither is source code or proof of a package build.

This 0.6.1 recovery preview is **newly assembled**, not a rediscovery of the
missing 0.6 implementation and not evidence that prior native claims were tested.

Recovered unchanged from `/usr/share/velordp/source` inside the attached
`win-rdp_0.4.0_all.deb`:

- engine/src/*.cpp and *.hpp (only core, profile and rdp_session enter this new build)
- core_tests.cpp and geometry_tests.cpp
- LICENSE and the original monitor SVG

Newly written in this pass:

- native/bridge.cpp and bridge.h: GTK native surface and C ABI
- native/CMakeLists.txt
- src-tauri: Rust commands, persistence and Tauri application
- frontend/index.html, app.css and app.js
- scripts/preflight.py, build-deb.sh, install-build-deps.sh and package-binary.py
- packaging metadata, instructions and recovery tests

Preserved historical material:

- website/*.html from the available winrdp-050 previews. The latest website
  variants and complete Next.js website source were not recoverable.

These distinctions are intentional. New bridge code is uncompiled in the
current authoring environment. Historical engine tests do not validate it.
