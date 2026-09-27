#!/usr/bin/env python3
"""Check this host without installing packages or changing anything."""
from __future__ import annotations
import platform, shutil, subprocess, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
TOOLS = ('cargo','rustc','cmake','pkg-config','dpkg-deb','dpkg-shlibdeps','readelf','file')
SYSTEM_PKGS = ('gtk+-3.0','webkit2gtk-4.1','openssl','alsa','wayland-client','xkbcommon')
def run(args):
    return subprocess.run(args,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=20)

def main():
    errors=[]
    print('Win RDP build preflight')
    if platform.system()!='Linux': errors.append('Build on Linux (preferably the target Zorin machine).')
    for tool in TOOLS:
        if not shutil.which(tool): errors.append(f'Missing tool: {tool}')
    if shutil.which('pkg-config'):
        for name in SYSTEM_PKGS:
            result=run(['pkg-config','--modversion',name])
            if result.returncode: errors.append(f'Missing development package: {name}')
            else: print(f'  {name}: {result.stdout.strip()}')
    for relative in ['src-tauri/Cargo.toml','frontend/index.html',
                     'src-tauri/Cargo.lock','session/Cargo.toml','session/Cargo.lock',
                     'third_party/ironrdp/crates/ironrdp/Cargo.toml']:
        if not (ROOT/relative).is_file(): errors.append(f'Source file missing: {relative}')
    if errors:
        print('\nBuild has not started:')
        for item in errors:print('  '+item)
        print('\nRun bash scripts/install-build-deps.sh and install Rust with rustup (see BUILDING.md).')
        return 2
    print('\nPreflight passed. Compilation and live testing have NOT run yet.')
    return 0
if __name__=='__main__':sys.exit(main())
