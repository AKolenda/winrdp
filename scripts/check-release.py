#!/usr/bin/env python3
"""Check version consistency without compiling or accessing credentials."""
import argparse
import ast
import json
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
if len(set(versions.values())) != 1:
    raise SystemExit('Release versions disagree: ' + json.dumps(versions, sort_keys=True))
version = versions['session/Cargo.toml']
if args.tag and args.tag != f'v{version}':
    raise SystemExit(f'Release tag must be v{version}')
print(f'Release metadata agrees: {version}')
