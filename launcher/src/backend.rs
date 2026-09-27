// SPDX-License-Identifier: AGPL-3.0-only
//! What the launcher does outside its window: the library file and the
//! `winrdp-session` processes. [`Demo`] keeps both in memory for tests and screenshots.
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde::Deserialize;
use zeroize::Zeroizing;

pub use crate::library::Store;
use crate::library::{Library, Preferences, Profile};

/// Transport a running session published: `{"transport":"udp","udpVersion":2,"label":"UDP v2"}`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transport {
    pub transport: String,
    #[serde(default)]
    pub udp_version: Option<u8>,
    pub label: String,
}
/// Why a session stopped, as it published before exiting:
/// `{"state":"failed","reason":"credentials","message":"...","detail":"..."}`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Report {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub detail: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Ended {
    pub session: String,
    pub code: Option<i32>,
    pub report: Option<Report>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Live {
    pub session: String,
    pub transport: Option<Transport>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub ended: Vec<Ended>,
    pub live: Vec<Live>,
}

pub trait Backend {
    fn load(&mut self) -> Result<Library, String>;
    /// Saves a computer and returns the new library with the identifier it was saved under.
    fn save_profile(&mut self, profile: Profile) -> Result<(Library, String), String>;
    fn delete_profile(&mut self, id: &str) -> Result<Library, String>;
    fn save_preferences(&mut self, preferences: Preferences) -> Result<Library, String>;
    /// Opens a desktop in its own session window; returns the session identifier.
    fn launch(&mut self, profile: &Profile, password: &str, fullscreen: bool) -> Result<String, String>;
    fn poll(&mut self) -> Status;
    fn kill(&mut self, session: &str);
    fn kill_all(&mut self);
}

/// `$XDG_CONFIG_HOME/io.winrdp.Next` and `$XDG_DATA_HOME/io.winrdp.Next/logs`: where the
/// Tauri launcher kept the library and session logs, so both carry over.
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("io.winrdp.Next")
}
pub fn log_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("io.winrdp.Next").join("logs")
}
fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var).map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback),
    }
}

/// The session log filter unless `IRONRDP_LOG` says otherwise: connection and transport
/// events, and the per-second `session perf` line, without the RDP-UDP packet diagnostics.
const DEFAULT_LOG_FILTER: &str = "info,ironrdp_client=debug";
/// Session logs and status files older than this are deleted when the launcher starts.
const LOG_RETENTION: Duration = Duration::from_secs(14 * 24 * 60 * 60);

pub struct Real {
    store: Store,
    log_dir: PathBuf,
    children: HashMap<String, Child>,
}
impl Real {
    pub fn new() -> Self {
        let log_dir = log_dir();
        prune_logs(&log_dir, LOG_RETENTION);
        Self {
            store: Store {
                path: config_dir().join("computers.json"),
            },
            log_dir,
            children: HashMap::new(),
        }
    }
    fn change(&mut self, edit: impl FnOnce(&mut Library) -> Result<(), String>) -> Result<Library, String> {
        let mut library = self.store.load()?;
        edit(&mut library)?;
        self.store.save(&library)?;
        Ok(library)
    }
}
impl Backend for Real {
    fn load(&mut self) -> Result<Library, String> {
        self.store.load()
    }
    fn save_profile(&mut self, profile: Profile) -> Result<(Library, String), String> {
        let mut id = String::new();
        let library = self.change(|library| {
            id = library.upsert(profile)?;
            Ok(())
        })?;
        Ok((library, id))
    }
    fn delete_profile(&mut self, id: &str) -> Result<Library, String> {
        self.change(|library| {
            library.remove(id);
            Ok(())
        })
    }
    fn save_preferences(&mut self, preferences: Preferences) -> Result<Library, String> {
        self.change(|library| {
            library.preferences = preferences;
            Ok(())
        })
    }
    fn launch(&mut self, profile: &Profile, password: &str, fullscreen: bool) -> Result<String, String> {
        let secret = Zeroizing::new(password.to_owned());
        if secret.is_empty() || secret.len() > 4096 || secret.contains('\0') {
            return Err("Enter an account password, up to 4096 bytes.".into());
        }
        let bin = session_binary()?;
        let session = uuid::Uuid::new_v4().to_string();
        fs::create_dir_all(&self.log_dir).map_err(|e| e.to_string())?;
        let log = self.log_dir.join(format!("session-{session}.log"));
        let status = status_file(&self.log_dir, &session);
        let _ = fs::remove_file(&status);
        let mut command = session_command(&bin, profile, &secret, fullscreen, &status, &log);
        let child = command
            .spawn()
            .map_err(|e| format!("Could not start the session window: {e}"))?;
        self.children.insert(session.clone(), child);
        Ok(session)
    }
    fn poll(&mut self) -> Status {
        let mut status = Status::default();
        let log_dir = self.log_dir.clone();
        self.children.retain(|id, child| match child.try_wait() {
            Ok(Some(exit)) => {
                let report = read_status(&log_dir, id).and_then(|value| serde_json::from_value(value).ok());
                let _ = fs::remove_file(status_file(&log_dir, id));
                status.ended.push(Ended {
                    session: id.clone(),
                    code: exit.code(),
                    report,
                });
                false
            }
            _ => {
                // A record without a label is a failure notice, not a transport.
                let transport = read_status(&log_dir, id).and_then(|value| serde_json::from_value(value).ok());
                status.live.push(Live {
                    session: id.clone(),
                    transport,
                });
                true
            }
        });
        status
    }
    fn kill(&mut self, session: &str) {
        if let Some(mut child) = self.children.remove(session) {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_file(status_file(&self.log_dir, session));
    }
    fn kill_all(&mut self) {
        let ids: Vec<String> = self.children.keys().cloned().collect();
        for id in ids {
            self.kill(&id);
        }
    }
}

/// The session process for one connection. Credentials travel by environment (visible only to
/// this user), never on the command line.
fn session_command(
    bin: &Path,
    profile: &Profile,
    password: &str,
    fullscreen: bool,
    status: &Path,
    log: &Path,
) -> Command {
    let env_off = |var: &str| std::env::var(var).is_ok_and(|v| v == "0");
    let mut command = Command::new(bin);
    command
        .env("RDP_HOSTNAME", &profile.address)
        .env("RDP_USERNAME", &profile.username)
        .env("RDP_PASSWORD", password)
        // Reliable RDP-UDP is on by default, offering MS-RDPEUDP version 2, which every
        // Windows host measured answers immediately; graphics (EGFX) are Soft-Synced onto
        // the tunnel and verified rendering. WINRDP_UDP=0 keeps a session on TCP. The
        // session falls back to TCP by itself when the UDP bootstrap fails.
        .env("IRONRDP_UDP", if env_off("WINRDP_UDP") { "0" } else { "1" })
        .env(
            "IRONRDP_UDP_OFFER",
            std::env::var("WINRDP_UDP_OFFER").unwrap_or_else(|_| "2".into()),
        )
        // The graphics pipeline (EGFX) is the fast path and is on by default;
        // WINRDP_EGFX=0 forces the legacy bitmap path.
        .env("IRONRDP_EGFX", if env_off("WINRDP_EGFX") { "0" } else { "1" })
        .env("WINRDP_TITLE", &profile.name)
        // The session publishes its transport (TCP / UDP v1-3) here; the tab shows it.
        .env("WINRDP_STATUS_FILE", status)
        .env("WINRDP_FULLSCREEN", if fullscreen { "1" } else { "0" })
        .env(
            "IRONRDP_LOG",
            std::env::var("IRONRDP_LOG").unwrap_or_else(|_| DEFAULT_LOG_FILTER.into()),
        )
        .arg("--log-file")
        .arg(log)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !profile.clipboard {
        command.args(["--clipboard-type", "disable"]);
    }
    if !profile.audio {
        command.arg("--audio-disable");
    }
    if profile.microphone {
        command.arg("--microphone");
    }
    if profile.printer {
        command.env("WINRDP_PRINTER", "default");
    } else {
        // The profile owns redirection: a launcher environment override must not
        // re-enable a printer the user explicitly left disabled for this computer.
        command.env_remove("WINRDP_PRINTER");
    }
    command
}
fn session_binary() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("WINRDP_SESSION_BIN") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("No executable directory")?;
    let candidate = dir.join("winrdp-session");
    if candidate.is_file() {
        return Ok(candidate);
    }
    Err("The winrdp-session program was not found next to winrdp-next. Set WINRDP_SESSION_BIN.".into())
}
/// Deletes `session-*` logs and status files not written to for `max_age`. By age only:
/// another launcher may be running, and its sessions' files must survive.
fn prune_logs(dir: &Path, max_age: Duration) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > max_age);
        if old
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("session-"))
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}
fn status_file(log_dir: &Path, session: &str) -> PathBuf {
    log_dir.join(format!("session-{session}.status.json"))
}
fn read_status(log_dir: &Path, session: &str) -> Option<serde_json::Value> {
    fs::read(status_file(log_dir, session))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// An in-memory backend: fictional computers, sessions that only exist as records.
/// Tests drive it by queueing what the next [`Backend::poll`] reports.
#[derive(Default)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct Demo {
    pub library: Library,
    pub launches: Vec<(String, String, bool)>,
    pub killed: Vec<String>,
    pub pending: Status,
    pub live: HashMap<String, Option<Transport>>,
    next: u32,
}
#[cfg_attr(not(test), allow(dead_code))]
impl Demo {
    pub fn with(library: Library) -> Self {
        Self {
            library,
            ..Self::default()
        }
    }
    /// Sample computers with documentation addresses, as the website screenshots show.
    pub fn sample() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let computer = |id: u128, name: &str, address: &str, user: &str, ago_hours: Option<i64>| Profile {
            id: uuid::Uuid::from_u128(id).to_string(),
            name: name.into(),
            address: address.into(),
            username: user.into(),
            last_connected: ago_hours
                .map(|h| crate::library::format_iso(now - h * 3_600_000))
                .unwrap_or_default(),
            ..Profile::draft()
        };
        let mut office = computer(1, "Office PC", "192.0.2.10", "alex", Some(1));
        office.favorite = true;
        office.printer = true;
        office.group = "Work".into();
        let mut studio = computer(3, "Studio", "192.0.2.30", "alex", Some(72));
        studio.fullscreen = true;
        studio.microphone = true;
        Self::with(Library {
            computers: vec![
                office,
                computer(2, "Lab", "192.0.2.20", "admin", Some(24)),
                studio,
                computer(4, "Home PC", "192.0.2.40", "alex", None),
            ],
            ..Library::default()
        })
    }
}
impl Backend for Demo {
    fn load(&mut self) -> Result<Library, String> {
        Ok(self.library.clone())
    }
    fn save_profile(&mut self, profile: Profile) -> Result<(Library, String), String> {
        let id = self.library.upsert(profile)?;
        Ok((self.library.clone(), id))
    }
    fn delete_profile(&mut self, id: &str) -> Result<Library, String> {
        self.library.remove(id);
        Ok(self.library.clone())
    }
    fn save_preferences(&mut self, preferences: Preferences) -> Result<Library, String> {
        self.library.preferences = preferences;
        Ok(self.library.clone())
    }
    fn launch(&mut self, profile: &Profile, password: &str, fullscreen: bool) -> Result<String, String> {
        if password.is_empty() {
            return Err("Enter an account password, up to 4096 bytes.".into());
        }
        self.next += 1;
        let session = format!("demo-session-{}", self.next);
        self.launches.push((profile.id.clone(), session.clone(), fullscreen));
        self.live.insert(session.clone(), None);
        Ok(session)
    }
    fn poll(&mut self) -> Status {
        let mut status = std::mem::take(&mut self.pending);
        for ended in &status.ended {
            self.live.remove(&ended.session);
        }
        for live in &status.live {
            self.live.insert(live.session.clone(), live.transport.clone());
        }
        status.live = self
            .live
            .iter()
            .map(|(session, transport)| Live {
                session: session.clone(),
                transport: transport.clone(),
            })
            .collect();
        status
    }
    fn kill(&mut self, session: &str) {
        self.live.remove(session);
        self.killed.push(session.into());
    }
    fn kill_all(&mut self) {
        self.killed.extend(self.live.drain().map(|(session, _)| session));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_for(profile: &Profile) -> Command {
        session_command(
            Path::new("winrdp-session"),
            profile,
            "dummy-test-only",
            false,
            Path::new("session.status.json"),
            Path::new("session.log"),
        )
    }

    #[test]
    fn a_disabled_printer_cannot_be_enabled_by_the_launcher_environment() {
        let command = command_for(&Profile::draft());
        let printer = command
            .get_envs()
            .find(|(key, _)| *key == "WINRDP_PRINTER")
            .map(|(_, value)| value);
        assert_eq!(
            printer,
            Some(None),
            "the child must explicitly remove the inherited variable"
        );
    }

    #[test]
    fn enabled_printer_and_microphone_options_reach_the_session() {
        let mut profile = Profile::draft();
        profile.printer = true;
        profile.microphone = true;
        let command = command_for(&profile);
        assert!(
            command
                .get_envs()
                .any(|(key, value)| { key == "WINRDP_PRINTER" && value == Some(std::ffi::OsStr::new("default")) })
        );
        assert!(command.get_args().any(|argument| argument == "--microphone"));
    }

    #[test]
    fn disabled_clipboard_and_audio_options_reach_the_session() {
        let mut profile = Profile::draft();
        profile.clipboard = false;
        profile.audio = false;
        let command = command_for(&profile);
        let arguments: Vec<_> = command.get_args().collect();
        assert!(arguments.windows(2).any(|pair| pair == ["--clipboard-type", "disable"]));
        assert!(arguments.iter().any(|argument| *argument == "--audio-disable"));
        assert!(!arguments.iter().any(|argument| *argument == "--microphone"));
    }

    #[test]
    fn pruning_deletes_only_old_session_files() {
        let dir = std::env::temp_dir().join(format!("winrdp-prune-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        for name in [
            "session-old.log",
            "session-old.status.json",
            "session-new.log",
            "notes.txt",
        ] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        let a_month_ago = std::time::SystemTime::now() - Duration::from_secs(30 * 24 * 60 * 60);
        for name in ["session-old.log", "session-old.status.json", "notes.txt"] {
            fs::File::options()
                .write(true)
                .open(dir.join(name))
                .unwrap()
                .set_modified(a_month_ago)
                .unwrap();
        }
        prune_logs(&dir, LOG_RETENTION);
        let mut left: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["notes.txt", "session-new.log"]);
        fs::remove_dir_all(dir).unwrap();
    }
}
