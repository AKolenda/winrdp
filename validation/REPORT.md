# Validation of the 0.6.1 recovery source

## Completed

- Recovered core C++ code compiled: 174 checks passed.
- Recovered geometry logic compiled: 55 mathematical scenarios passed.
- 15 dependency-free recovery/build-contract tests passed.
- 17 Chromium browser checks passed, including filtering, editing, custom display
  selection, light/dark controls, literal rendering of user text, and a simulated
  certificate/connect/disconnect command flow.
- JavaScript syntax, Python syntax, shell syntax and XML/JSON parsing checked.
- A complete source ZIP is produced and CRC-tested separately at delivery.

The Chromium environment blocks local navigation. The browser checks injected
local HTML/CSS/JavaScript into an empty page and replaced icon URLs with data
URLs. Native commands were explicitly mocked. This is NOT a Tauri/WebKitGTK
runtime test, and not a test of the actual RDP connection.

## Build attempts

`build-attempt.txt`: the actual normal-user build script stopped at preflight:
no cargo/rustc/patchelf and no GTK/WebKit/Qt/FreeRDP development packages.

`native-configure.txt`: direct CMake configure failed because Qt6 development
configuration was absent. No native bridge compilation happened.

`download-attempt.txt`: the crates registry check failed DNS resolution.
The new Rust crate has not been dependency-resolved or compiled here. A
Cargo.lock cannot be provided until real registry resolution succeeds.

## Not verified

Full C++ bridge / Rust compilation, linker success, Tauri launch, native GTK
surface placement, actual certificate/password handling, keyboard/mouse,
clipboard, audio, fullscreen, GPU behavior, or Windows interoperability.

The binary packaging script has only its refusal/structure paths tested. It has
NOT packaged this new application because no new executable exists here.
There is NO completed Tauri .deb in this delivery. Run the provided build on
Zorin; compiler or integration errors may still need correction.

The archive is a reconstructed source preview, not the unavailable prior
migration snapshot or a feature-complete replacement for the working client.
