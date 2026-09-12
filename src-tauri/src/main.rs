// SPDX-License-Identifier: AGPL-3.0-only
#![cfg_attr(not(target_os = "linux"), allow(unused))]
#[cfg(not(target_os = "linux"))]
compile_error!("This recovery build targets Linux only.");
use glib::translate::ToGlibPtr;
use gtk::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{ffi::{c_char, c_void, CStr, CString}, fs::{self, OpenOptions}, io::Write,
          os::unix::fs::{OpenOptionsExt, PermissionsExt}, path::PathBuf, sync::Mutex, collections::HashMap};
use tauri::{Manager, WebviewWindow};
use zeroize::{Zeroize, Zeroizing};

extern "C" {
    fn wr_attach(vbox: *mut c_void, web: *mut c_void) -> *mut c_char;
    fn wr_command(op: *const c_char, json: *const c_char, password: *const c_char) -> *mut c_char;
    fn wr_poll() -> *mut c_char;
    fn wr_free(value: *mut c_char);
}
// Native APIs are invoked only inside run_on_main_thread/with_webview closures.
unsafe fn decode(ptr: *mut c_char) -> Result<Value, String> {
    if ptr.is_null() { return Err("The native backend returned no result.".into()); }
    let parsed = serde_json::from_slice::<Value>(CStr::from_ptr(ptr).to_bytes());
    wr_free(ptr);
    let value = parsed.map_err(|_| "The native backend returned invalid JSON.".to_string())?;
    if let Some(error) = value.get("error").and_then(Value::as_str) { return Err(error.to_owned()); }
    Ok(value)
}
fn trusted(window: &WebviewWindow) -> Result<(), String> {
    if window.label() != "main" { return Err("This window is not authorized.".into()); }
    let url = window.url().map_err(|e| e.to_string())?;
    let local = url.scheme() == "tauri" && url.host_str() == Some("localhost") ||
        matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost");
    if !local { return Err("Only packaged application pages may call the backend.".into()); }
    Ok(())
}
async fn native(window: &WebviewWindow, op: &str, payload: Value,
                secret: Option<Zeroizing<String>>) -> Result<Value, String> {
    trusted(window)?;
    let op = CString::new(op).map_err(|_| "Invalid operation")?;
    let payload = CString::new(payload.to_string()).map_err(|_| "Invalid payload")?;
    let (send, recv) = tokio::sync::oneshot::channel();
    window.run_on_main_thread(move || {
        let mut bytes = secret.as_deref().map(|s| s.as_bytes().to_vec()).unwrap_or_default();
        bytes.push(0);
        let result = unsafe { decode(wr_command(op.as_ptr(), payload.as_ptr(), bytes.as_ptr().cast())) };
        bytes.zeroize(); drop(secret);
        let _ = send.send(result);
    }).map_err(|e| e.to_string())?;
    recv.await.map_err(|_| "The UI thread stopped.".to_string())?
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Profile {
    id: String, name: String, address: String, username: String,
    #[serde(default = "group")] group: String,
    #[serde(default)] favorite: bool,
    #[serde(default = "yes")] clipboard: bool,
    #[serde(default = "yes")] audio: bool,
    /// Redirect the local microphone into the session (MS-RDPEAI).
    #[serde(default)] microphone: bool,
    /// Offer a "Win RDP" printer in the session; jobs go to the local default printer,
    /// or to ~/Downloads as PostScript when there is none.
    #[serde(default)] printer: bool,
    #[serde(default)] fullscreen: bool,
    #[serde(default)] all_monitors: bool,
    #[serde(default)] compatibility: bool,
    #[serde(default = "graphics")] graphics: String,
    #[serde(default = "layout")] keyboard_layout: u32,
    #[serde(default)] last_connected: String,
}
fn yes() -> bool { true }
/// Sessions run as separate winrdp-session processes (IronRDP engine) unless the
/// classic in-window FreeRDP surface is requested with WINRDP_ENGINE=freerdp.
fn engine() -> &'static str {
    if std::env::var("WINRDP_ENGINE").map(|v| v == "freerdp").unwrap_or(false) { "freerdp" } else { "ironrdp" }
}
struct Sessions { children: Mutex<HashMap<String, std::process::Child>> }
fn session_binary() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("WINRDP_SESSION_BIN") { let p = PathBuf::from(p); if p.is_file() { return Ok(p); } }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("No executable directory")?;
    for candidate in [dir.join("winrdp-session"), dir.join("../../../session/target/release/winrdp-session")] {
        if candidate.is_file() { return Ok(candidate); }
    }
    Err("The winrdp-session program was not found next to winrdp-next. Set WINRDP_SESSION_BIN.".into())
}
#[tauri::command]
fn launch_session(window: WebviewWindow, app: tauri::AppHandle, id: String, password: String, fullscreen: bool) -> Result<Value, String> {
    trusted(&window)?;
    let secret = Zeroizing::new(password);
    if secret.is_empty() || secret.len() > 4096 || secret.contains('\0') { return Err("Enter an account password, up to 4096 bytes.".into()); }
    let profile = { let store = app.state::<Store>(); let _guard = store.lock.lock().map_err(|_| "Library lock failed")?;
        store.load()?.computers.into_iter().find(|p| p.id == id).ok_or("Computer no longer exists")? };
    let bin = session_binary()?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    let log = log_dir.join(format!("session-{session_id}.log"));
    let status = status_file(&log_dir, &session_id);
    let _ = fs::remove_file(&status);
    // Credentials travel by environment (per-user visible only), never on the command line.
    let mut command = std::process::Command::new(&bin);
    command.env("RDP_HOSTNAME", &profile.address).env("RDP_USERNAME", &profile.username)
        .env("RDP_PASSWORD", secret.as_str())
        // Reliable RDP-UDP is on by default, offering MS-RDPEUDP version 2, which every
        // Windows host measured answers immediately; graphics (EGFX) are Soft-Synced onto
        // the tunnel and verified rendering. WINRDP_UDP=0 keeps a session on TCP. The
        // session falls back to TCP by itself when the UDP bootstrap fails.
        .env("IRONRDP_UDP", if std::env::var("WINRDP_UDP").map(|v| v == "0").unwrap_or(false) { "0" } else { "1" })
        .env("IRONRDP_UDP_OFFER", std::env::var("WINRDP_UDP_OFFER").unwrap_or_else(|_| "2".into()))
        // Graphics pipeline (EGFX) advertisement; WINRDP_EGFX=1 to evaluate. Off until verified.
        // The graphics pipeline renders (H.264-less) and is the fast path; on by default.
        .env("IRONRDP_EGFX", if std::env::var("WINRDP_EGFX").map(|v| v == "0").unwrap_or(false) { "0" } else { "1" })
        .env("WINRDP_TITLE", &profile.name)
        // The session publishes its transport (TCP / UDP v1-3) here; the tab shows it.
        .env("WINRDP_STATUS_FILE", &status)
        .env("WINRDP_FULLSCREEN", if fullscreen { "1" } else { "0" })
        .env("IRONRDP_LOG", std::env::var("IRONRDP_LOG").unwrap_or_else(|_| "info,ironrdp_client=debug,ironrdp_rdpeudp_tokio=debug".into()))
        .arg("--log-file").arg(&log)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    if !profile.clipboard { command.args(["--clipboard-type", "disable"]); }
    if !profile.audio { command.arg("--audio-disable"); }
    if profile.microphone { command.arg("--microphone"); }
    if profile.printer { command.env("WINRDP_PRINTER", "default"); }
    let child = command.spawn().map_err(|e| format!("Could not start the session window: {e}"))?;
    app.state::<Sessions>().children.lock().map_err(|_| "Session lock failed")?.insert(session_id.clone(), child);
    Ok(json!({"session": session_id, "log": log}))
}
#[tauri::command]
fn session_status(window: WebviewWindow, app: tauri::AppHandle) -> Result<Value, String> {
    trusted(&window)?;
    let mut ended = Vec::new();
    let mut live = Vec::new();
    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    let sessions = app.state::<Sessions>();
    let mut map = sessions.children.lock().map_err(|_| "Session lock failed")?;
    map.retain(|id, child| match child.try_wait() {
        Ok(Some(status)) => {
            let _ = fs::remove_file(status_file(&log_dir, id));
            ended.push(json!({"session": id, "code": status.code()})); false
        }
        _ => {
            // Transport as published by the session: {"transport":"udp","udpVersion":2,"label":"UDP v2"}.
            let transport = fs::read(status_file(&log_dir, id)).ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            live.push(json!({"session": id, "transport": transport}));
            true
        }
    });
    Ok(json!({"ended": ended, "live": live}))
}
fn status_file(log_dir: &std::path::Path, session_id: &str) -> PathBuf { log_dir.join(format!("session-{session_id}.status.json")) }
#[tauri::command]
fn kill_session(window: WebviewWindow, app: tauri::AppHandle, id: String) -> Result<Value, String> {
    trusted(&window)?;
    let sessions = app.state::<Sessions>();
    let mut map = sessions.children.lock().map_err(|_| "Session lock failed")?;
    if let Some(mut child) = map.remove(&id) { let _ = child.kill(); let _ = child.wait(); }
    if let Ok(log_dir) = app.path().app_log_dir() { let _ = fs::remove_file(status_file(&log_dir, &id)); }
    Ok(json!({"ok": true}))
}
fn group() -> String { "Personal".into() }
fn graphics() -> String { "auto".into() }
fn layout() -> u32 { 0x409 }
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Preferences { dark: bool, open_settings: bool, #[serde(default = "layout_simple")] layout: String, #[serde(default)] host_checked: bool }
fn layout_simple() -> String { "simple".into() }
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Library { version: u32, computers: Vec<Profile>, preferences: Preferences }
impl Default for Preferences {
    fn default() -> Self { Self { dark: false, open_settings: false, layout: layout_simple(), host_checked: false } }
}
impl Default for Library {
    fn default() -> Self { Self { version: 1, computers: vec![], preferences: Preferences::default() } }
}
struct Store { path: PathBuf, lock: Mutex<()> }
impl Store {
    fn load(&self) -> Result<Library, String> {
        if !self.path.exists() { return Ok(Library::default()); }
        if fs::metadata(&self.path).map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024 {
            return Err("The library exceeds the 2 MB limit; it was not changed.".into());
        }
        let value: Library = serde_json::from_slice(&fs::read(&self.path).map_err(|e| e.to_string())?)
            .map_err(|_| "The saved library is invalid; it was not changed.".to_string())?;
        if value.version != 1 || value.computers.len() > 1000 { return Err("Unsupported library format.".into()); }
        Ok(value)
    }
    fn save(&self, library: &Library) -> Result<(), String> {
        let parent = self.path.parent().ok_or("Invalid library directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        let tmp = parent.join(format!(".library-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), String> {
            let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600)
                .open(&tmp).map_err(|e| e.to_string())?;
            let bytes = serde_json::to_vec_pretty(library).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() { let _ = fs::remove_file(&tmp); }
        result
    }
}
#[tauri::command]
async fn bootstrap(window: WebviewWindow, app: tauri::AppHandle) -> Result<Value, String> {
    trusted(&window)?;
    let library = { let store = app.state::<Store>(); let _guard = store.lock.lock().map_err(|_| "Library lock failed")?; store.load()? };
    let info = native(&window, "info", json!({}), None).await?;
    Ok(json!({"library": library, "native": info, "version": "0.7.4", "engine": engine(),
        "user": std::env::var("USER").unwrap_or_default()}))
}
#[tauri::command]
async fn save_profile(window: WebviewWindow, app: tauri::AppHandle, mut profile: Profile) -> Result<Library, String> {
    trusted(&window)?;
    if profile.id.is_empty() { profile.id = uuid::Uuid::new_v4().to_string(); }
    native(&window, "validate", serde_json::to_value(&profile).map_err(|e| e.to_string())?, None).await?;
    let store = app.state::<Store>(); let _guard = store.lock.lock().map_err(|_| "Library lock failed")?;
    let mut library = store.load()?;
    if let Some(index) = library.computers.iter().position(|p| p.id == profile.id) { library.computers[index] = profile; }
    else { if library.computers.len() >= 1000 { return Err("The library is full.".into()); } library.computers.push(profile); }
    store.save(&library)?; Ok(library)
}
#[tauri::command]
fn delete_profile(window: WebviewWindow, app: tauri::AppHandle, id: String) -> Result<Library, String> {
    trusted(&window)?;
    let store = app.state::<Store>(); let _guard = store.lock.lock().map_err(|_| "Library lock failed")?;
    let mut library = store.load()?; library.computers.retain(|p| p.id != id); store.save(&library)?; Ok(library)
}
#[tauri::command]
fn save_preferences(window: WebviewWindow, app: tauri::AppHandle, preferences: Preferences) -> Result<Library, String> {
    trusted(&window)?;
    let store = app.state::<Store>(); let _guard = store.lock.lock().map_err(|_| "Library lock failed")?;
    let mut library = store.load()?; library.preferences = preferences; store.save(&library)?; Ok(library)
}
#[tauri::command]
async fn connect_session(window: WebviewWindow, app: tauri::AppHandle, id: String, password: String) -> Result<Value, String> {
    trusted(&window)?;
    let secret = Zeroizing::new(password);
    if secret.is_empty() || secret.len() > 4096 || secret.contains('\0') { return Err("Enter an account password, up to 4096 bytes.".into()); }
    let profile = { let store = app.state::<Store>(); let _guard = store.lock.lock().map_err(|_| "Library lock failed")?;
        store.load()?.computers.into_iter().find(|p| p.id == id).ok_or("Computer no longer exists")? };
    native(&window, "connect", json!({"profile": profile}), Some(secret)).await
}
#[tauri::command]
async fn session_action(window: WebviewWindow, operation: String, payload: Value) -> Result<Value, String> {
    if !matches!(operation.as_str(), "select" | "library" | "disconnect" | "certificate" | "clipboard" | "release" | "cad") {
        return Err("Unsupported session action.".into());
    }
    if payload.to_string().len() > 8192 { return Err("Request is too large.".into()); }
    native(&window, &operation, payload, None).await
}
#[tauri::command]
async fn poll_events(window: WebviewWindow) -> Result<Value, String> {
    trusted(&window)?;
    let (send, recv) = tokio::sync::oneshot::channel();
    window.run_on_main_thread(move || { let _ = send.send(unsafe { decode(wr_poll()) }); }).map_err(|e| e.to_string())?;
    recv.await.map_err(|_| "The UI thread stopped.".to_string())?
}
#[tauri::command]
fn window_action(window: WebviewWindow, operation: String) -> Result<Value, String> {
    trusted(&window)?;
    match operation.as_str() {
        "minimize" => window.minimize(),
        "maximize" => if window.is_maximized().map_err(|e| e.to_string())? { window.unmaximize() } else { window.maximize() },
        "fullscreen" => window.set_fullscreen(!window.is_fullscreen().map_err(|e| e.to_string())?),
        "restore" => window.set_fullscreen(false),
        "drag" => window.start_dragging(),
        _ => return Err("Unsupported window action.".into()),
    }.map_err(|e| e.to_string())?;
    Ok(json!({"ok": true}))
}
/// Sizes the launcher for a layout: the simple window is a small dialog, the full one a workspace.
#[tauri::command]
fn set_layout(window: WebviewWindow, layout: String) -> Result<Value, String> {
    trusted(&window)?;
    let (min, size) = match layout.as_str() {
        "simple" => ((460.0, 420.0), (480.0, 600.0)),
        "full" => ((780.0, 540.0), (1240.0, 820.0)),
        _ => return Err("Unknown layout.".into()),
    };
    let e = |e: tauri::Error| e.to_string();
    if window.is_fullscreen().map_err(e)? { window.set_fullscreen(false).map_err(e)?; }
    if window.is_maximized().map_err(e)? { window.unmaximize().map_err(e)?; }
    window.set_min_size(Some(tauri::LogicalSize::new(min.0, min.1))).map_err(e)?;
    window.set_size(tauri::LogicalSize::new(size.0, size.1)).map_err(e)?;
    window.set_resizable(layout != "simple").map_err(e)?;
    window.center().map_err(e)?;
    Ok(json!({"ok": true}))
}
/// One shell command with a timeout, for the GNOME Remote Desktop tools.
fn sh(program: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("timeout").arg("10").arg(program).args(args)
        .stdin(std::process::Stdio::null()).output().map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() { Ok(String::from_utf8_lossy(&output.stdout).into_owned()) }
    // Arguments and child diagnostics may contain a sharing password. Never surface them.
    else { Err(format!("{program} failed ({}).", output.status)) }
}
fn user_unit_active(unit: &str) -> bool {
    sh("systemctl", &["--user", "is-active", unit]).map(|s| s.trim() == "active").unwrap_or(false)
}
/// What answers RDP connections *into* this computer. Win RDP is a client; the host side is
/// GNOME Remote Desktop. Two of its modes matter: Desktop Sharing mirrors the desktop that is
/// signed in (what a user expects), and headless "Remote Login" creates a separate session with
/// its own login. This reports which one is up so the settings page can say so truthfully.
#[tauri::command]
fn host_status(window: WebviewWindow) -> Result<Value, String> {
    trusted(&window)?;
    let Ok(text) = sh("grdctl", &["status"]) else {
        return Ok(json!({"available": false, "sharing": false, "headless": false, "mirror": false, "credentials": false, "port": 3389}));
    };
    let field = |key: &str| text.lines().find_map(|l| { let l = l.trim(); l.strip_prefix(key).map(|v| v.trim().to_owned()) });
    let port = field("Port:").and_then(|p| p.parse::<u16>().ok()).unwrap_or(3389);
    let credentials = field("Username:").is_some_and(|u| !u.is_empty() && u != "(none)");
    let mode = sh("gsettings", &["get", "org.gnome.desktop.remote-desktop.rdp", "screen-share-mode"]).unwrap_or_default();
    Ok(json!({
        "available": true,
        "sharing": user_unit_active("gnome-remote-desktop.service"),
        "headless": user_unit_active("gnome-remote-desktop-headless.service"),
        "mirror": mode.contains("mirror"),
        "extend": mode.contains("extend"),
        "enabled": field("Status:").is_some_and(|s| s == "enabled"),
        "credentials": credentials,
        "port": port,
    }))
}
/// Makes this desktop reachable from Windows the way the user expects: mirror the signed-in
/// desktop on port 3389, on now and at every login, and turn the separate-session headless
/// mode off so it releases the port. Credentials are only replaced when a name is given.
#[tauri::command]
fn host_enable(window: WebviewWindow, username: String, password: String, mode: String) -> Result<Value, String> {
    trusted(&window)?;
    // `mirror-primary` shows the physical screen and needs a monitor that is on; `extend` is a
    // virtual screen in the same signed-in session and works with no monitor attached.
    let share_mode = match mode.as_str() { "extend" => "extend", _ => "mirror-primary" };
    if !username.trim().is_empty() {
        if password.is_empty() || password.len() > 512 || password.contains('\0') { return Err("Enter a sharing password.".into()); }
        sh("grdctl", &["rdp", "set-credentials", username.trim(), &password])?;
    }
    let _ = sh("systemctl", &["--user", "disable", "--now", "gnome-remote-desktop-headless.service"]);
    sh("gsettings", &["set", "org.gnome.desktop.remote-desktop.rdp", "screen-share-mode", share_mode])?;
    sh("grdctl", &["rdp", "enable"])?;
    sh("grdctl", &["rdp", "disable-view-only"])?;
    sh("systemctl", &["--user", "enable", "--now", "gnome-remote-desktop.service"])?;
    // A restart picks up the mode and credential changes when the service was already running.
    let _ = sh("systemctl", &["--user", "restart", "gnome-remote-desktop.service"]);
    Ok(json!({"ok": true}))
}
/// Stops accepting connections into this desktop, now and at login.
#[tauri::command]
fn host_disable(window: WebviewWindow) -> Result<Value, String> {
    trusted(&window)?;
    sh("systemctl", &["--user", "disable", "--now", "gnome-remote-desktop.service"])?;
    Ok(json!({"ok": true}))
}
/// Opens GNOME's Remote Desktop settings, where sharing this desktop is switched on.
#[tauri::command]
fn open_host_settings(window: WebviewWindow) -> Result<Value, String> {
    trusted(&window)?;
    for panel in ["remote-desktop", "sharing"] {
        if std::process::Command::new("gnome-control-center").arg(panel)
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .spawn().is_ok() { return Ok(json!({"ok": true})); }
    }
    Err("Could not open GNOME Settings. Open Settings, then System, then Remote Desktop.".into())
}
#[tauri::command]
async fn quit(window: WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    native(&window, "stop-all", json!({}), None).await?;
    if let Ok(mut map) = app.state::<Sessions>().children.lock() {
        for (_, child) in map.iter_mut() { let _ = child.kill(); let _ = child.wait(); }
        map.clear();
    }
    // Never destroy a running native worker or its GTK callback data.
    for _ in 0..300 {
        if native(&window, "shutdown-ready", json!({}), None).await?["ready"] == true { app.exit(0); return Ok(()); }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err("A connection is still shutting down. Wait a moment, then close again.".into())
}
fn main() {
    if std::env::args().any(|a| a == "--version") { println!("Win RDP Next 0.7.4 (migration preview)"); return; }
    // This recovery build establishes a software baseline, not an experimental GPU release.
    std::env::set_var("WINRDP_HWDECODER", "software");
    tauri::Builder::default()
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("computers.json");
            app.manage(Store { path, lock: Mutex::new(()) });
            app.manage(Sessions { children: Mutex::new(HashMap::new()) });
            let window = app.get_webview_window("main").ok_or("Main window missing")?;
            if std::env::var_os("WINRDP_SYSTEM_FRAME").is_some() { window.set_decorations(true)?; }
            let vbox = window.default_vbox()?;
            let pointer: *mut gtk::ffi::GtkBox = vbox.to_glib_none().0;
            let pointer = pointer as usize;
            window.with_webview(move |platform| {
                let inner = platform.inner();
                // Upcast to GTK's Widget so pointer typing is unambiguous.
                let widget: gtk::Widget = inner.upcast();
                let p: *mut gtk::ffi::GtkWidget = widget.to_glib_none().0;
                let p = p.cast::<c_void>();
                if let Err(reason) = unsafe { decode(wr_attach(pointer as *mut c_void, p)) } { eprintln!("Native attach failed: {reason}"); }
            })?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Some(web) = window.get_webview_window("main") { let _ = web.eval("window.WinRdp && window.WinRdp.confirmClose()"); }
            }
        })
        .invoke_handler(tauri::generate_handler![bootstrap, save_profile, delete_profile, save_preferences,
            connect_session, session_action, poll_events, window_action, quit, launch_session, session_status, kill_session,
            set_layout, host_status, host_enable, host_disable, open_host_settings])
        .run(tauri::generate_context!())
        .expect("Win RDP Next could not start");
}
