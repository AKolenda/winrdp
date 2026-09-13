#!/usr/bin/env python3
"""Dependency-free checks. These do not compile Rust/GTK or exercise RDP."""
import importlib.util, json, subprocess, tempfile, unittest, xml.etree.ElementTree as ET
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
pkg=load('package_binary',ROOT/'scripts/package-binary.py')
pre=load('preflight',ROOT/'scripts/preflight.py')
class RecoveryTests(unittest.TestCase):
    def test_manifest(self):
        config=json.loads((ROOT/'src-tauri/tauri.conf.json').read_text())
        self.assertEqual(config['identifier'],'io.winrdp.Next');self.assertFalse(config['app']['windows'][0]['decorations'])
        self.assertTrue((ROOT/'src-tauri'/config['build']['frontendDist']/'index.html').is_file())
    def test_separate_identity(self):
        desktop=(ROOT/'packaging/io.winrdp.Next.desktop').read_text()
        self.assertIn('Exec=winrdp-next\n',desktop);self.assertIn('Keywords=RDP;',desktop)
        self.assertNotIn('Exec=winrdp\n',desktop)
    def test_no_fake_binary_package(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);binary=root/'binary';binary.write_text('not an executable');out=root/'dist'
            with self.assertRaises(RuntimeError):pkg.package(binary,None,out)
            self.assertFalse(out.exists())
    def test_missing_binary(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            with self.assertRaises(RuntimeError):pkg.package(root/'missing',None,root/'out')
            self.assertFalse((root/'out').exists())
    def test_private_path_boundary(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);private=root/'runtime';private.mkdir();file=private/'a.so';file.touch()
            outer=root/'outside.so';outer.touch();link=private/'link.so';link.symlink_to(outer)
            self.assertTrue(pkg.inside(file,private));self.assertFalse(pkg.inside(outer,private))
            self.assertFalse(pkg.inside(link,private));self.assertFalse(pkg.inside(file,None))
    def test_env_is_private_first(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);(p/'lib/pkgconfig').mkdir(parents=True)
            self.assertEqual(pre.runtime_env(p)['PKG_CONFIG_PATH'],str(p/'lib/pkgconfig'))
    def test_no_web_frame_transport(self):
        js=(ROOT/'frontend/app.js').read_text();native=(ROOT/'native/bridge.cpp').read_text()
        self.assertNotIn('toDataURL',js);self.assertNotIn('base64',js);self.assertIn('cairo_image_surface_create_for_data',native)
        self.assertNotIn('<canvas',(ROOT/'frontend/index.html').read_text())
    def test_original_clipboard_fix_retained(self):
        source=(ROOT/'engine/src/rdp_session.cpp').read_text()
        self.assertIn('response.common.msgFlags',source);self.assertNotIn('response.msgFlags',source)
    def test_password_not_profile(self):
        rust=(ROOT/'src-tauri/src/main.rs').read_text();profile=rust.split('struct Profile {',1)[1].split('\n}',1)[0]
        self.assertNotIn('password',profile);self.assertIn('Zeroizing::new(password)',rust)
        self.assertIn('deny_unknown_fields',rust)
    def test_settings_do_not_open_themselves(self):
        js=(ROOT/'frontend/app.js').read_text();html=(ROOT/'frontend/index.html').read_text()
        self.assertNotIn('preferences.openSettings',js);self.assertNotIn('startup-switch',html);self.assertNotIn('startup-switch',js)
        self.assertIn("get('open')==='settings'",js)
    def test_refused_connections_are_reported(self):
        js=(ROOT/'frontend/app.js').read_text();launcher=(ROOT/'src-tauri/src/main.rs').read_text()
        session=(ROOT/'session/src/app.rs').read_text()
        self.assertIn('reportSessionEnd',js);self.assertIn("failure.reason==='credentials'",js)
        self.assertIn('id="password-error"',(ROOT/'frontend/index.html').read_text())
        self.assertIn('"status": published',launcher)
        self.assertIn('write_failure_status(&failure)',session);self.assertIn('WRONG_CREDENTIALS',session)
    def test_capabilities(self):
        capabilities=json.loads((ROOT/'src-tauri/capabilities/main.json').read_text())
        self.assertEqual(capabilities['windows'],['main']);self.assertNotIn('remote',capabilities)
        self.assertFalse(any('shell' in p or 'fs:' in p for p in capabilities['permissions']))
    def test_native_no_widgets_frontend(self):
        cmake=(ROOT/'native/CMakeLists.txt').read_text()
        self.assertNotIn('Qt6::Widgets',cmake);self.assertIn('Qt6::Network',cmake)
    def test_source_files(self):
        for name in ['frontend/app.js','frontend/app.css','native/bridge.h','src-tauri/src/main.rs','BUILDING.md','PROVENANCE.md']:
            self.assertGreater((ROOT/name).stat().st_size,100)
    def test_metadata_xml(self):
        root=ET.parse(ROOT/'packaging/io.winrdp.Next.metainfo.xml').getroot()
        self.assertEqual(root.find('launchable').text,'io.winrdp.Next.desktop')
    def test_build_script_syntax(self):
        for name in ['build-deb.sh','install-build-deps.sh']:
            subprocess.run(['bash','-n',str(ROOT/'scripts'/name)],check=True)
    def test_js_syntax(self):
        subprocess.run(['node','--check',str(ROOT/'frontend/app.js')],check=True)
if __name__=='__main__':unittest.main(verbosity=2)
