# Release website design

Question: which landing-page structure makes the native Linux client and GitHub
release destination clear without recreating a download service?

Three throwaway layouts were explored on one `prototype.html?variant=A|B|C`
route: a blue split layout with the actual launcher, a centered product showcase,
and a developer handbook with a fixed sidebar. The split layout is the selected
implementation: it presents the real client and the release link together in the
first viewport, while keeping build and contribution information below.

Design tokens: Windows blue `#123fc6`, ink `#192b47`, secondary text `#53637a`,
paper `#ffffff`, separator `#d8e0ed`, and soft background `#eff3fa`.
Typography uses Aptos/Segoe UI with system fallbacks. The launcher screenshot is
the characteristic visual; no stock illustration or decorative dashboard is used.
The site requires no client JavaScript.

The primary-source prototype is preserved on the `prototype/release-website`
branch. Run `cd website && npm run prototype`, then open
`http://127.0.0.1:8090/prototype.html?variant=A`. Arrow keys and the floating
switcher cycle the variants. The branch is throwaway and is not deployed.
Production includes only the selected implementation under `website/public/`.
