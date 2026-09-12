# Win RDP website

Static release feeder website, deployed to the current Cloudflare account at
https://winrdp.app.

Every download link goes directly to
https://github.com/AKolenda/winrdp/releases. No installers are hosted here.

```sh
cd website
npm ci
npm run build
npm run dev
npm run deploy
```

The build copies only `public/` into `dist/`; legacy design concepts and local
configuration are never deployed. Wrangler is pinned in the lockfile. The site
has no analytics, cookies, external fonts, or JavaScript dependency at runtime.

The screenshot uses fictional example computers and documentation IP addresses.

## Domain and account

Production uses `winrdp.app` in its domain-owning Cloudflare account, explicitly
selected by `account_id` in `wrangler.jsonc`. On the maintainer machine the website
directory is bound to the existing `openxplorer` Wrangler auth profile. This uses
the domain account's existing login without changing the default account or
other projects. The binding is local Wrangler configuration; no token is stored
in this repository.

On a different machine, authenticate an account with access to that account and
zone before deploying. A manual profile selection is also available:

```sh
npx wrangler deploy --profile YOUR_PROFILE
```

The original preview on `winrdp.openfuel-monorepo.workers.dev` belongs to the main
account and remains available. It is not the production deployment target.

See [Custom Domains](https://developers.cloudflare.com/workers/configuration/routing/custom-domains/).
