#!/usr/bin/env python3
"""Package only an actually compiled Win RDP binary; never emit a source-only .deb.

Run on the target distribution. Resolve system Depends with dpkg-shlibdeps,
include native Qt/FreeRDP libraries from the selected private runtime only,
and do not bundle glibc, GTK, WebKitGTK or graphics drivers.
"""
from __future__ import annotations
import argparse, hashlib, json, os, re, shutil, subprocess, sys, tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
VERSION='0.7.5'

def command(args, *, cwd=None, env=None):
    result=subprocess.run([str(a) for a in args],cwd=cwd,env=env,text=True,
        stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=180)
    if result.returncode:
        raise RuntimeError('Command failed: '+' '.join(map(str,args))+'\n'+result.stderr)
    return result.stdout

def elf(path):
    with path.open('rb') as stream: return stream.read(4)==b'\x7fELF'

def linked(path, env):
    output=command(['ldd',path],env=env)
    if 'not found' in output: raise RuntimeError(f'Unresolved libraries for {path}:\n{output}')
    matches={}
    for line in output.splitlines():
        m=re.match(r'\s*(\S+)\s+=>\s+(/\S+)\s+\(',line)
        if m: matches[m[1]]=Path(m[2])
    return matches

def inside(path, root):
    return root is not None and path.resolve().is_relative_to(root.resolve())

def owned_system_library(path):
    options={str(path),str(path.resolve())}
    if str(path).startswith('/usr/lib/'):options.add(str(path).replace('/usr/lib/','/lib/',1))
    if str(path).startswith('/lib/'):options.add('/usr'+str(path))
    for option in options:
        result=subprocess.run(['dpkg-query','-S',option],text=True,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=10)
        if result.returncode==0:return
    raise RuntimeError(f'System library is not owned by an installed Debian package: {path}. '
                       'Use a matching, packaged build environment; not an arbitrary local library.')

def write(path, value, mode=0o644):
    path.parent.mkdir(parents=True,exist_ok=True);path.write_text(value);path.chmod(mode)

def package(binary, runtime, output, session=None):
    binary=binary.resolve();runtime=runtime.resolve() if runtime else None
    session=session.resolve() if session else None
    if not binary.is_file() or not elf(binary):raise RuntimeError('A compiled ELF Win RDP binary is required. No package was created.')
    if not session or not session.is_file() or not elf(session):raise RuntimeError('A compiled ELF winrdp-session binary is required. No package was created.')
    if VERSION not in command([binary,'--version']):raise RuntimeError('Unexpected application version; refusing to package.')
    if command([session,'--version']).strip() != f'winrdp-session {VERSION}':raise RuntimeError('Session and launcher versions must match; refusing to package.')
    arch=command(['dpkg','--print-architecture']).strip()
    env=os.environ.copy()
    if runtime:env['LD_LIBRARY_PATH']=f'{runtime}/lib:{runtime}/lib64'+(':'+env['LD_LIBRARY_PATH'] if env.get('LD_LIBRARY_PATH') else '')
    output.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='winrdp-deb-') as tmp:
        temp=Path(tmp);stage=temp/'stage';private=stage/'usr/lib/winrdp-next';private.mkdir(parents=True)
        installed=private/'bin/winrdp-next';installed.parent.mkdir();shutil.copy2(binary,installed);installed.chmod(0o755)
        session_installed=None
        if session:session_installed=private/'bin/winrdp-session';shutil.copy2(session,session_installed);session_installed.chmod(0o755)
        private_sources={};system_sources={};plugin_sources=[]
        if runtime and (runtime/'plugins/tls').is_dir():
            for p in sorted((runtime/'plugins/tls').glob('*.so')):
                plugin_sources.append((p,private/'qt-plugins/tls'/p.name))
        inspect=[binary]+([session] if session else [])+[p for p,_ in plugin_sources]
        for item in inspect:
            for name,src in linked(item,env).items():
                if inside(src,runtime):
                    if name in private_sources and private_sources[name].resolve()!=src.resolve():
                        raise RuntimeError('Conflicting private libraries named '+name)
                    private_sources[name]=src
                else: system_sources[name]=src
        for src in system_sources.values():owned_system_library(src)
        for name,src in private_sources.items():shutil.copy2(src.resolve(),private/name)
        for src,dest in plugin_sources:
            dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(src,dest)
            command(['patchelf','--set-rpath','$ORIGIN/../..',dest])
        for name in private_sources:command(['patchelf','--set-rpath','$ORIGIN',private/name])
        command(['patchelf','--set-rpath','$ORIGIN/..',installed])
        command(['patchelf','--set-rpath','$ORIGIN/..',session_installed])
        # --version does not launch GTK or connect to any remote machine.
        clean_env={k:v for k,v in os.environ.items() if k not in ('LD_LIBRARY_PATH','QT_PLUGIN_PATH','QML2_IMPORT_PATH')}
        command([installed,'--version'],env=clean_env)
        linked(installed,clean_env)
        command([session_installed,'--version'],env=clean_env)
        linked(session_installed,clean_env)
        write(private/'bin/qt.conf','[Paths]\nPrefix=..\nPlugins=qt-plugins\n')
        write(stage/'usr/bin/winrdp-next','#!/bin/sh\nexec /usr/lib/winrdp-next/bin/winrdp-next "$@"\n',0o755)
        write(stage/'usr/bin/winrdp-session','#!/bin/sh\nexec /usr/lib/winrdp-next/bin/winrdp-session "$@"\n',0o755)
        desktop=(ROOT/'packaging/io.winrdp.Next.desktop').read_text()
        write(stage/'usr/share/applications/io.winrdp.Next.desktop',desktop)
        icon=stage/'usr/share/icons/hicolor/scalable/apps/winrdp-next.svg';icon.parent.mkdir(parents=True);shutil.copy2(ROOT/'frontend/assets/winrdp.svg',icon)
        icon=stage/'usr/share/icons/hicolor/256x256/apps/winrdp-next.png';icon.parent.mkdir(parents=True);shutil.copy2(ROOT/'src-tauri/icons/icon.png',icon)
        write(stage/'usr/share/metainfo/io.winrdp.Next.metainfo.xml',(ROOT/'packaging/io.winrdp.Next.metainfo.xml').read_text())
        docs=stage/'usr/share/doc/winrdp-next';docs.mkdir(parents=True)
        for filename in ('README.md','BUILDING.md','LICENSE','PROVENANCE.md'):shutil.copy2(ROOT/filename,docs/filename)
        write(docs/'THIRD-PARTY.md',(ROOT/'docs/THIRD-PARTY.md').read_text())
        shutil.copytree(ROOT/'docs/licenses',docs/'licenses')
        shutil.copytree(ROOT/'docs/runtime-patches',docs/'runtime-patches')
        shutil.copy2(ROOT/'docs/runtime-source-manifest.json',docs/'runtime-source-manifest.json')
        shutil.copy2(ROOT/'docs/rust-dependency-notices.json',docs/'rust-dependency-notices.json')
        for filename in ('LICENSE-MIT','LICENSE-APACHE'):
            shutil.copy2(ROOT/'third_party/ironrdp'/filename,docs/f'IronRDP-{filename}')
        # dpkg-shlibdeps needs a package source/control context. Unpackaged private
        # runtime libraries have no shlibs entries; their system dependencies are
        # inspected explicitly below. Every non-private ELF dependency was checked
        # for a dpkg owner above before --ignore-missing-info is used.
        write(temp/'debian/control','Source: winrdp-next\nSection: net\nPriority: optional\nMaintainer: Win RDP project <build@localhost>\n\nPackage: winrdp-next\nArchitecture: any\nDescription: Win RDP desktop client\n')
        elves=[installed]+([session_installed] if session_installed else [])+[private/name for name in private_sources]+[dest for _,dest in plugin_sources]
        result=command(['dpkg-shlibdeps','-O','--ignore-missing-info',f'-l{private}']+['-e'+str(p) for p in elves],cwd=temp,env=clean_env)
        deps=next((line.partition('=')[2] for line in result.splitlines() if line.startswith('shlibs:Depends=')),None)
        if not deps or 'libc6' not in deps or 'libwebkit2gtk-4.1-0' not in deps:
            raise RuntimeError('Could not derive complete runtime dependencies; refusing to create .deb.\n'+result)
        size=sum(p.stat().st_size for p in stage.rglob('*') if p.is_file())//1024
        write(stage/'DEBIAN/control',f'Package: winrdp-next\nVersion: {VERSION}\nArchitecture: {arch}\nSection: net\nPriority: optional\nMaintainer: Win RDP project <build@localhost>\nInstalled-Size: {size}\nDepends: {deps}, fonts-dejavu-core\nDescription: Win RDP desktop client for Linux\n Rust-powered IronRDP sessions and a native GTK desktop surface.\n This package installs a client only; it does not enable an RDP host service.\n')
        write(stage/'DEBIAN/postinst','#!/bin/sh\nset -e\nif command -v update-desktop-database >/dev/null; then update-desktop-database -q /usr/share/applications || true; fi\nif command -v gtk-update-icon-cache >/dev/null; then gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true; fi\n',0o755)
        manifest=[]
        for p in sorted(stage.rglob('*')):
            if p.is_file() and 'DEBIAN' not in p.relative_to(stage).parts:
                manifest.append(hashlib.md5(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(stage)))
        write(stage/'DEBIAN/md5sums','\n'.join(manifest)+'\n')
        destination=output/f'winrdp-next_{VERSION}_{arch}.deb'
        command(['dpkg-deb','--root-owner-group','--build',stage,destination])
        command(['dpkg-deb','--info',destination])
        report={'version':VERSION,'architecture':arch,'depends':deps,
                'private_libraries':sorted(private_sources),'qt_tls_plugins':[p.name for p,_ in plugin_sources],
                'sha256':hashlib.sha256(destination.read_bytes()).hexdigest(),
                'live_rdp_tested':False}
        write(output/'package-report.json',json.dumps(report,indent=2)+'\n')
        write(output/(destination.name+'.sha256'),report['sha256']+'  '+destination.name+'\n')
        print('Created real binary package:',destination)
        print('SHA256:',report['sha256'])
        print('The classic client, its settings, host services and firewall were not modified.')
        return destination

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',required=True,type=Path)
    parser.add_argument('--runtime',default='');parser.add_argument('--out',type=Path,default=ROOT/'dist')
    parser.add_argument('--session',type=Path,default=None)
    args=parser.parse_args()
    try:package(args.binary,Path(args.runtime) if args.runtime else None,args.out,args.session)
    except (RuntimeError,OSError,subprocess.SubprocessError) as e:print(str(e),file=sys.stderr);return 1
    return 0
if __name__=='__main__':sys.exit(main())
