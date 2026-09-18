import { cp, mkdir, rm, readFile } from 'node:fs/promises';
// Only public/ is deployed: never publish source, credentials, or design prototypes.
const html = await readFile(new URL('./public/index.html', import.meta.url), 'utf8');
// The version comes from the app, not from a literal here: a release that forgets
// the site would otherwise ship while winrdp.app still advertised the old one.
const { version } = JSON.parse(await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
if (!html.includes('https://github.com/AKolenda/winrdp/releases')) throw new Error('Missing GitHub releases link');
if (!html.includes(`/releases/tag/v${version}`)) throw new Error(`Release link does not point at v${version}`);
if (!html.includes(`Download v${version}`)) throw new Error(`Download button does not say v${version}`);
if (!html.includes('cursor-connect')) throw new Error('Missing guided product tour');
if (/href=["'][^"']*\.(deb|zip|AppImage)["']/i.test(html)) throw new Error('Downloads must remain on GitHub Releases');
await rm(new URL('./dist', import.meta.url), { recursive: true, force: true });
await mkdir(new URL('./dist', import.meta.url), { recursive: true });
await cp(new URL('./public', import.meta.url), new URL('./dist', import.meta.url), { recursive: true });
console.log('Static site built. Downloads link directly to GitHub Releases.');
