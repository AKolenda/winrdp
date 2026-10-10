# Release checklist

## Source publication

- Review all published branch and tag history for credentials and private workstation
  data before it is pushed. Do not publish local application checkpoint refs and do not
  use `git push --mirror`.
- Grep anything about to be published for test-host names, LAN addresses and local user
  names; screenshots and doc comments are the usual carriers.
- Make the IronRDP fork revision available before publishing an app revision that refers to it.
- Confirm screenshots show demonstration profiles and contain no private desktop content.
- Run `cargo xtask check`, `cargo xtask notices --check` and
  `cargo xtask check-release`.

## Binary validation

- Build both binaries with `cargo xtask deb` and install the resulting package
  in a clean supported distribution. Check launcher and session startup without a private
  development environment. Record the distribution and desktop session type.
- Test new and saved computer connections, blank optional names, invalid credentials,
  resize, fullscreen, disconnect, reconnect and TCP fallback.
- Test clipboard text in both directions, clipboard disabled, reconnect after clipboard
  use, and the advertised image formats. Verify X11 and Wayland separately.
- Test audio, microphone and printer features against Windows policies that allow them;
  record limitations instead of treating a successful build as proof.
- Assemble notices for all bundled dependencies: the Rust dependency graph and the
  IronRDP fork. The package bundles no native libraries.

## GitHub release

- Keep version fields synchronized; `cargo xtask check-release --tag v0.8.2` validates them.
- Create and push the release tag only after the desired source is committed in both repos.
- Run the manual **Prepare draft release** workflow against that tag. It builds the
  package in an Ubuntu 22.04 container, so its dependencies are the oldest supported
  versions, installs it with apt on clean Ubuntu 22.04 and 24.04 and Debian 12 and 13, and
  only then creates a draft with the `.deb`, `.deb.sha256`, package report and dependency
  notices attached. It never publishes. Use GitHub Releases for all downloads.
- Review the draft's notes against recorded live-test results before publication.
