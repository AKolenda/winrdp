#!/usr/bin/env python3
"""Check version consistency without compiling or accessing credentials."""
import argparse
import ast
import json
import re
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--tag')
args = parser.parse_args()
versions = {}
for path in ('src-tauri/Cargo.toml', 'session/Cargo.toml'):
    versions[path] = tomllib.loads((ROOT / path).read_text())['package']['version']
versions['tauri.conf.json'] = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())['version']
for node in ast.parse((ROOT / 'scripts/package-binary.py').read_text()).body:
    if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'VERSION' for t in node.targets):
        versions['package-binary.py'] = ast.literal_eval(node.value)
versions['AppStream'] = ET.parse(ROOT / 'packaging/io.winrdp.Next.metainfo.xml').find('releases/release').get('version')
preview = re.search(r"PREVIEW_VERSION\s*=\s*'([^']+)'", (ROOT / 'frontend/app.js').read_text())
versions['frontend/app.js'] = preview.group(1) if preview else 'not found'
if len(set(versions.values())) != 1:
    raise SystemExit('Release versions disagree: ' + json.dumps(versions, sort_keys=True))
version = versions['session/Cargo.toml']

# Text a reader acts on: an install command or a download link naming the wrong
# release is as broken as a mismatched manifest, and nothing else checks these.
mentions = {
    'README.md': f'winrdp-next_{version}_amd64.deb',
    'BUILDING.md': f'winrdp-next_{version}_amd64.deb',
    'docs/RELEASE-CHECKLIST.md': f'check-release.py --tag v{version}',
    'docs/RELEASE-NOTES.md': f'Win RDP {version}',
    'website/public/index.html': f'/releases/tag/v{version}',
}
missing = [f'{path} does not mention "{text}"' for path, text in mentions.items()
           if text not in (ROOT / path).read_text()]
if missing:
    raise SystemExit(f'Release {version} is not reflected everywhere:\n- ' + '\n- '.join(missing))

if args.tag and args.tag != f'v{version}':
    raise SystemExit(f'Release tag must be v{version}')
print(f'Release metadata agrees: {version}')
