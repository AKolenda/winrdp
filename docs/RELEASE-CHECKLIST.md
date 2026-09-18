# Release checklist

## Source publication

- Review all published branch and tag history for credentials and private workstation
  data before it is pushed. Do not publish local application checkpoint refs and do not
  use `git push --mirror`.
- Grep anything about to be published for test-host names, LAN addresses and local user
  names; screenshots and doc comments are the usual carriers.
- Make the IronRDP fork revision available before publishing an app revision that refers to it.
- Confirm screenshots show demonstration profiles and contain no private desktop content.
- Run `python3 tests/recovery_tests.py`, `python3 tests/notice_checks.py` and
  `python3 scripts/check-release.py`.

## Binary validation

- Build both binaries with `bash scripts/build-deb.sh` and install the resulting package
  in a clean supported distribution. Check launcher and session startup without a private
  development environment. Record the distribution and desktop session type.
- Test new and saved computer connections, blank optional names, invalid credentials,
  resize, fullscreen, disconnect, reconnect and TCP fallback.
- Test clipboard text in both directions, clipboard disabled, reconnect after clipboard
  use, and the advertised image formats. Verify X11 and Wayland separately.
- Test audio, microphone and printer features against Windows policies that allow them;
  record limitations instead of treating a successful build as proof.
- Assemble notices and corresponding source for all bundled dependencies, including the
  exact private Qt/FreeRDP build and Rust dependencies. Generic upstream links alone do
  not establish the provenance of a locally patched runtime.

## GitHub release

- Keep version fields synchronized; `scripts/check-release.py --tag v0.7.7` validates them.
- Create and push the release tag only after the desired source is committed in both repos.
- Run the manual **Prepare draft release** workflow against that tag. It creates a draft
  and never publishes it automatically.
- Attach the validated `.deb`, `.deb.sha256`, package report, dependency notices and any
  corresponding-source archives to the draft. Use GitHub Releases for all downloads.
- Review the draft's notes against recorded live-test results before publication.
