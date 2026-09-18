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

Production uses `winrdp.app` in the Cloudflare account that owns the domain, selected
by `account_id` in `wrangler.jsonc`. Authenticate a Cloudflare account with access to
that account and zone before deploying; the login is local Wrangler configuration and
no token is stored in this repository. Where a machine holds several logins, select one
explicitly:

```sh
npx wrangler deploy --profile YOUR_PROFILE
```

See [Custom Domains](https://developers.cloudflare.com/workers/configuration/routing/custom-domains/).
