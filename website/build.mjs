import { cp, mkdir, rm, readFile } from 'node:fs/promises';
// Only public/ is deployed: never publish source, credentials, or design prototypes.
const html = await readFile(new URL('./public/index.html', import.meta.url), 'utf8');
if (!html.includes('https://github.com/AKolenda/winrdp/releases')) throw new Error('Missing GitHub releases link');
if (!html.includes('/releases/tag/v0.7.5')) throw new Error('Missing current release link');
if (!html.includes('cursor-connect')) throw new Error('Missing guided product tour');
if (/href=["'][^"']*\.(deb|zip|AppImage)["']/i.test(html)) throw new Error('Downloads must remain on GitHub Releases');
await rm(new URL('./dist', import.meta.url), { recursive: true, force: true });
await mkdir(new URL('./dist', import.meta.url), { recursive: true });
await cp(new URL('./public', import.meta.url), new URL('./dist', import.meta.url), { recursive: true });
console.log('Static site built. Downloads link directly to GitHub Releases.');
