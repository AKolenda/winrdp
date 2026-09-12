fn main() {
    for var in ["WINRDP_NATIVE_BUILD", "PKG_CONFIG_PATH"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let native = std::env::var("WINRDP_NATIVE_BUILD")
        .expect("Run bash scripts/build-deb.sh from the source root first.");
    let dir = std::path::Path::new(&native);
    for archive in ["libwinrdp_bridge.a", "libwinrdp_core.a"] {
        if !dir.join(archive).is_file() { panic!("Native library missing: {archive}"); }
        println!("cargo:rerun-if-changed={}", dir.join(archive).display());
    }
    println!("cargo:rustc-link-search=native={native}");
    println!("cargo:rustc-link-lib=static=winrdp_bridge");
    println!("cargo:rustc-link-lib=static=winrdp_core");
    println!("cargo:rustc-link-lib=stdc++");
    for name in ["Qt6Network", "Qt6Gui", "Qt6Core", "freerdp-client3", "freerdp3", "winpr3", "gtk+-3.0"] {
        pkg_config::Config::new().probe(name).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    // Binary is installed in /usr/lib/winrdp-next/bin; private shared libraries
    // live one level above it. No author-machine path is embedded.
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/..");
    tauri_build::build();
}
