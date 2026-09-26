import { cp, mkdir, rm, readFile } from 'node:fs/promises';
// Only public/ is deployed: never publish source, credentials, or design prototypes.
const html = await readFile(new URL('./public/index.html', import.meta.url), 'utf8');
// Downloads go to GitHub's latest release, never a pinned version: a release then
// needs no website deploy, and winrdp.app can never advertise an old build.
if (!html.includes('https://github.com/AKolenda/winrdp/releases/latest')) throw new Error('Download link does not point at the latest release');
if (/\/releases\/tag\/|Download v\d/.test(html)) throw new Error('The site names a specific release; link to /releases/latest instead');
if (!html.includes('cursor-connect')) throw new Error('Missing guided product tour');
if (/href=["'][^"']*\.(deb|zip|AppImage)["']/i.test(html)) throw new Error('Downloads must remain on GitHub Releases');
await rm(new URL('./dist', import.meta.url), { recursive: true, force: true });
await mkdir(new URL('./dist', import.meta.url), { recursive: true });
await cp(new URL('./public', import.meta.url), new URL('./dist', import.meta.url), { recursive: true });
console.log('Static site built. Downloads link directly to GitHub Releases.');
