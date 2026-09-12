#!/usr/bin/env python3
"""Check this host without installing packages or modifying the classic client."""
from __future__ import annotations
import argparse, json, os, platform, shutil, subprocess, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
TOOLS = ('cargo','rustc','cmake','ninja','pkg-config','c++','dpkg-deb','dpkg-shlibdeps','readelf','patchelf','file')
SYSTEM_PKGS = ('gtk+-3.0','webkit2gtk-4.1','openssl','alsa','wayland-client','xkbcommon')
NATIVE_PKGS = ('Qt6Core','Qt6Gui','Qt6Network','freerdp3','freerdp-client3','winpr3')
def run(args, env=None):
    return subprocess.run(args,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=20)
def runtime_env(prefix):
    env=os.environ.copy()
    if prefix:
        locations=[prefix/'lib/pkgconfig',prefix/'lib64/pkgconfig']
        for parent in (prefix/'lib',):
            if parent.exists(): locations.extend(parent.glob('*/pkgconfig'))
        env['PKG_CONFIG_PATH']=os.pathsep.join(str(p) for p in locations if p.is_dir())
    return env
def discover_runtime():
    explicit=os.environ.get('WINRDP_RUNTIME')
    if explicit:
        prefix=Path(explicit).expanduser().resolve()
        if not prefix.is_dir(): raise ValueError(f'WINRDP_RUNTIME does not exist: {prefix}')
        return prefix
    home=Path(os.environ.get('XDG_DATA_HOME',str(Path.home()/'.local/share')))
    candidates=sorted((home/'velordp/runtimes').glob('*'),reverse=True)
    for prefix in candidates:
        if (prefix/'lib/pkgconfig/freerdp3.pc').is_file() and (prefix/'lib/pkgconfig/Qt6Core.pc').is_file():
            return prefix.resolve()
    return None

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--write-env',type=Path);args=parser.parse_args()
    errors=[]
    print('Win RDP build preflight')
    if platform.system()!='Linux': errors.append('Build on Linux (preferably the target Zorin machine).')
    for tool in TOOLS:
        if not shutil.which(tool): errors.append(f'Missing tool: {tool}')
    try: prefix=discover_runtime()
    except ValueError as e: print(e);return 2
    env=runtime_env(prefix)
    print(f'Native runtime: {prefix or "system packages"}')
    versions={}
    if shutil.which('pkg-config'):
        for name in SYSTEM_PKGS+NATIVE_PKGS:
            result=run(['pkg-config','--modversion',name],env=env)
            if result.returncode: errors.append(f'Missing development package: {name}')
            else: versions[name]=result.stdout.strip();print(f'  {name}: {versions[name]}')
        for name, minimum in [('freerdp3','3.31'),('Qt6Core','6.4')]:
            if name in versions and run(['pkg-config',f'--atleast-version={minimum}',name],env=env).returncode:
                errors.append(f'{name} must be at least {minimum}, found {versions[name]}.')
    for relative in ['native/bridge.cpp','engine/src/rdp_session.cpp','src-tauri/Cargo.toml','frontend/index.html',
                     'src-tauri/Cargo.lock','session/Cargo.toml','session/Cargo.lock',
                     'third_party/ironrdp/crates/ironrdp/Cargo.toml']:
        if not (ROOT/relative).is_file(): errors.append(f'Source file missing: {relative}')
    if errors:
        print('\nBuild has not started:')
        for item in errors:print('  '+item)
        print('\nRun bash scripts/install-build-deps.sh and install Rust with rustup (see BUILDING.md).')
        print('Keep your existing ~/.local/share/velordp/runtimes directory. No files there were changed.')
        return 2
    if args.write_env:
        args.write_env.parent.mkdir(parents=True,exist_ok=True)
        args.write_env.write_text(json.dumps({'runtime':str(prefix) if prefix else '',
            'pkg_config_path':env.get('PKG_CONFIG_PATH',''),'versions':versions},indent=2)+'\n')
    print('\nPreflight passed. Native compilation and live testing have NOT run yet.')
    return 0
if __name__=='__main__':sys.exit(main())
