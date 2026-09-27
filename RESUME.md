# Outstanding work

What an audit of this tree found and what has not been done yet. Everything listed
here was reproduced against the code, not guessed; the items that were fixed are in
the 0.7.7 release notes and are not repeated.

## Needs a decision or an action only the owner can take

- **Rotate the Windows password for the `novabila` test account.** It is in plain
  text in `HANDOFF.md` at the tip of the private `winrdp-next` repository's master
  and has been pushed to GitHub. Keeping that repository private limits the
  exposure but does not remove it. Nothing in this repository is affected.
- **Delete the stale `winrdp.openfuel-monorepo.workers.dev` Worker**, or point it
  at https://winrdp.app. It still serves an old build of the site whose every
  GitHub link targets the private repository (all 404 anonymously) and which
  states the wrong licence. Nothing here deploys or tests that target, so it will
  keep drifting.

## The IronRDP fork (`third_party/ironrdp`, branch `winrdp`)

Each of these needs a commit in the fork, a submodule pointer bump here and a
rebuild, which is why they are not in 0.7.7.

- **Numpad Enter, numpad `/` and the Menu key send wrong scancodes on Linux.**
  They reach the host as undefined make codes, so the keypress is lost or lands on
  another key. Upstream fixed this in `a59b609e` (`crates/ironrdp-viewer/src/keymap.rs`
  plus its call site); the fork predates it and `session/src/app.rs` carries the same
  fallback to raw evdev codes. Cherry-pick, port the table into `session/src/app.rs`,
  and assert `NumpadEnter -> (extended, 0x1C)` and `NumpadDivide -> (extended, 0x35)`.
- **Print jobs are spooled to a predictable path in shared `/tmp`**
  (`crates/ironrdp-rdpdr-native/src/nix/printer.rs`). On a multi-user host any local
  account can read a document while it prints, or pre-create a symlink there. Create
  the spool with `create_new(true).mode(0o600)` inside a per-session `0700` directory,
  or under `$XDG_RUNTIME_DIR`.
- **`#[must_use]` and the doc comment on `drain_output` were detached** by an inserted
  `output_size` (`crates/ironrdp-egfx/src/client.rs`). Ignoring `drain_output`'s return
  value permanently drops frame regions, which is why upstream marked it. Move
  `output_size` below it and restore the attribute.
- **Per-second diagnostics ship enabled.** `driver.rs` logs tunnel stats and datagram
  hex dumps every second at `info!`/`debug!`, roughly 2 MB an hour per session, and
  nothing prunes `session-*.log`. Gate them behind `trace!`, drop
  `ironrdp_rdpeudp_tokio=debug` from the launcher's default `IRONRDP_LOG`, and prune
  old session logs at launcher start.
- **`cargo xtask check lints` is red on the fork**, at 18 sites, so a PR from this
  branch fails upstream CI before review and a rebase onto upstream master would have
  to discard the fork's `connection.rs` almost entirely (upstream's merged #1919 is
  now authoritative). Worth doing before any further upstream contribution.
- **Upstreamable, in this order:** the RFX progressive SRL decoder leniency fix
  (`ironrdp-graphics/src/srl.rs`, well tested — delete the two now-dead error variants
  and the always-false branch first), the Soft-Sync tunnel DVC lifecycle and
  EGFX-over-tunnel fixes, then the auto-detect responder. All three are blocked on
  removing the `IRONRDP_UDP` / `IRONRDP_EGFX` / `IRONRDP_UDP_OFFER` environment
  switches in favour of connector config, which upstream will require.

## Still open here

- **Re-capture `docs/screenshots/session-window.png` against a demonstration
  Windows profile.** The published image was a real desktop; the account name is now
  replaced with a generic one and the taskbar cropped away, but a clean capture is
  better than a redacted one. `website/public/assets/session.png` is the same file.
- **Four per-computer settings can never be set and are ignored:** `compatibility`,
  `graphics`, `keyboardLayout` and `allMonitors` (`frontend/app.js`). They are saved
  into every library and read by nothing on the IronRDP path — the keyboard layout is
  pinned to `0x409`. Either honour them or drop them from the stored shape.
- **A library saved by 0.7.6 or later cannot be read back by 0.7.5.** `bootstrap`
  fails and the whole startup path stops at a toast. Either note the one-way step or
  keep serializing the retired field for one more release.
- **`tests/launcher_checks.mjs` needs Playwright.** It now exits non-zero when run
  without a browser instead of looking like a pass, and covers 29 checks including the
  refusal path, but no workflow runs it: add `playwright` as a dev dependency and a CI
  step, or state that it is manual.
- **Validation gaps carried forward** (see `docs/RELEASE-VALIDATION.md`): native
  Wayland clipboard, live image and file clipboard transfer, microphone, printer and
  remote audio, clean-distribution installation, and a UDP-against-TCP comparison on a
  lossy or high-latency link.
- **Bandwidth-measure auto-detect PDUs are answered with nothing**
  (`ironrdp-session/src/x224/mod.rs`, "not yet implemented"). On one cold connection
  the server spent about 14 s and 20 MB on repeated network auto-detect that produced
  no decoded frames. It did not recur on later connections, so the cause is unproven
  and no change was made; implementing MS-RDPBCGR 2.2.14.2.2 would rule it out.
