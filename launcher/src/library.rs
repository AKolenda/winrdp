// SPDX-License-Identifier: AGPL-3.0-only
//! Saved computers and preferences: `computers.json` in the user's config directory.
//! The format is the one every earlier release wrote, so a library carries over.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::endpoint;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub address: String,
    pub username: String,
    #[serde(default = "group")]
    pub group: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default = "yes")]
    pub clipboard: bool,
    #[serde(default = "yes")]
    pub audio: bool,
    /// Redirect the local microphone into the session (MS-RDPEAI).
    #[serde(default)]
    pub microphone: bool,
    /// Offer a "Win RDP" printer in the session; jobs go to the local default printer,
    /// or to ~/Downloads as PostScript when there is none.
    #[serde(default)]
    pub printer: bool,
    #[serde(default)]
    pub fullscreen: bool,
    // The next four are kept so libraries round-trip through older releases; nothing reads them.
    #[serde(default)]
    pub all_monitors: bool,
    #[serde(default)]
    pub compatibility: bool,
    #[serde(default = "graphics")]
    pub graphics: String,
    #[serde(default = "layout")]
    pub keyboard_layout: u32,
    /// When the computer last answered, as an ISO 8601 UTC timestamp; empty if never.
    #[serde(default)]
    pub last_connected: String,
}
fn yes() -> bool {
    true
}
fn group() -> String {
    "Personal".into()
}
fn graphics() -> String {
    "auto".into()
}
fn layout() -> u32 {
    0x409
}

impl Profile {
    /// A new, unsaved computer with the defaults the dialogs start from.
    pub fn draft() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            address: String::new(),
            username: String::new(),
            group: group(),
            favorite: false,
            clipboard: true,
            audio: true,
            microphone: false,
            printer: false,
            fullscreen: false,
            all_monitors: false,
            compatibility: false,
            graphics: graphics(),
            keyboard_layout: layout(),
            last_connected: String::new(),
        }
    }
    /// The rules of the retired C++ engine's Profile::validationError; lengths count UTF-16 units as Qt did.
    pub fn validate(&self) -> Result<(), String> {
        let units = |s: &str| s.encode_utf16().count();
        if uuid::Uuid::parse_str(&self.id).map_or(true, |id| id.is_nil()) {
            return Err("The computer profile has an invalid identifier.".into());
        }
        if self.name.trim().is_empty() || units(&self.name) > 100 {
            return Err("Give this computer a name (up to 100 characters).".into());
        }
        endpoint::parse(&self.address)?;
        if units(&self.username) > 256 || units(&self.group) > 80 {
            return Err("The username or group name is too long.".into());
        }
        if [&self.name, &self.username, &self.group]
            .iter()
            .any(|s| s.chars().any(|c| c < ' ' || c == '\x7f'))
        {
            return Err("Names cannot contain control characters.".into());
        }
        if !matches!(self.graphics.as_str(), "auto" | "avc420" | "avc444") {
            return Err("Choose a supported graphics mode.".into());
        }
        if self.keyboard_layout == 0 {
            return Err("Choose a Windows keyboard layout.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    Simple,
    Full,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preferences {
    pub dark: bool,
    /// Retired in 0.7.6: the app used to open Settings at startup. Accepted so
    /// libraries written by earlier versions still load, and dropped on save.
    #[serde(default, skip_serializing)]
    open_settings: bool,
    #[serde(default = "layout_simple")]
    pub layout: Layout,
    /// Retired: recorded whether the host side had been checked once, and nothing
    /// ever read it back. Accepted so older libraries still load, and dropped on save.
    #[serde(default, skip_serializing)]
    host_checked: bool,
}
fn layout_simple() -> Layout {
    Layout::Simple
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            dark: false,
            open_settings: false,
            layout: Layout::Simple,
            host_checked: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub version: u32,
    pub computers: Vec<Profile>,
    pub preferences: Preferences,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1,
            computers: vec![],
            preferences: Preferences::default(),
        }
    }
}
impl Library {
    /// Validates and inserts or replaces a computer, giving a new one its identifier.
    /// Returns the identifier it was saved under.
    pub fn upsert(&mut self, mut profile: Profile) -> Result<String, String> {
        if profile.id.is_empty() {
            profile.id = uuid::Uuid::new_v4().to_string();
        }
        profile.validate()?;
        let id = profile.id.clone();
        if let Some(index) = self.computers.iter().position(|p| p.id == profile.id) {
            self.computers[index] = profile;
        } else {
            if self.computers.len() >= 1000 {
                return Err("The library is full.".into());
            }
            self.computers.push(profile);
        }
        Ok(id)
    }
    pub fn remove(&mut self, id: &str) {
        self.computers.retain(|p| p.id != id);
    }
    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.computers.iter().find(|p| p.id == id)
    }
}

/// The library file. Every change re-reads it first, so a file edited or broken
/// elsewhere is never silently overwritten with a stale copy.
pub struct Store {
    pub path: PathBuf,
}
impl Store {
    /// The saved layout, read before the window opens so it opens at the right size.
    pub fn layout_hint() -> Layout {
        Store {
            path: crate::backend::config_dir().join("computers.json"),
        }
        .load()
        .map_or(Layout::Simple, |l| l.preferences.layout)
    }
    pub fn load(&self) -> Result<Library, String> {
        if !self.path.exists() {
            return Ok(Library::default());
        }
        if fs::metadata(&self.path).map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024 {
            return Err("The library exceeds the 2 MB limit; it was not changed.".into());
        }
        let value: Library = serde_json::from_slice(&fs::read(&self.path).map_err(|e| e.to_string())?)
            .map_err(|_| "The saved library is invalid; it was not changed.".to_string())?;
        if value.version != 1 || value.computers.len() > 1000 {
            return Err("Unsupported library format.".into());
        }
        Ok(value)
    }
    pub fn save(&self, library: &Library) -> Result<(), String> {
        let parent = self.path.parent().ok_or("Invalid library directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        let tmp = parent.join(format!(".library-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), String> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)
                .map_err(|e| e.to_string())?;
            let bytes = serde_json::to_vec_pretty(library).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

/// The current time as the ISO 8601 UTC string earlier releases stored, e.g. `2026-09-27T00:41:46.875Z`.
pub fn now_iso() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    format_iso(ms)
}
pub fn format_iso(ms: i64) -> String {
    let (days, rest) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1000 % 60,
        rest % 1000
    )
}
/// Milliseconds since the epoch of an ISO 8601 UTC timestamp as [`format_iso`] writes it.
pub fn parse_iso(s: &str) -> Option<i64> {
    let (date, time) = s.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.splitn(3, '-').map(str::parse::<i64>);
    let (y, m, d) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut hms = hms.splitn(3, ':').map(str::parse::<i64>);
    let (h, mi, sec) = (hms.next()?.ok()?, hms.next()?.ok()?, hms.next()?.ok()?);
    let ms: i64 = format!("{frac:0<3}").get(..3)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400_000 + ((h * 60 + mi) * 60 + sec) * 1000 + ms)
}
// Howard Hinnant's civil calendar algorithms.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iso_round_trip() {
        assert_eq!(format_iso(0), "1970-01-01T00:00:00.000Z");
        let s = "2026-09-27T00:41:46.875Z";
        assert_eq!(format_iso(parse_iso(s).unwrap()), s);
        assert_eq!(
            parse_iso("2024-02-29T23:59:59Z").map(format_iso).as_deref(),
            Some("2024-02-29T23:59:59.000Z")
        );
        assert_eq!(parse_iso("yesterday"), None);
    }
    #[test]
    fn older_libraries_load_and_retired_fields_drop() {
        let text = r#"{"version":1,"computers":[{"id":"6f1c2c1e-6c2b-4e8b-9f55-0c1a3c2b7d10","name":"Office PC","address":"192.0.2.10","username":"alex"}],"preferences":{"dark":true,"openSettings":true,"hostChecked":true}}"#;
        let library: Library = serde_json::from_str(text).unwrap();
        assert_eq!(library.preferences.layout, Layout::Simple);
        assert!(library.computers[0].clipboard && library.computers[0].audio);
        let saved = serde_json::to_string(&library).unwrap();
        assert!(!saved.contains("openSettings") && !saved.contains("hostChecked"));
        assert!(saved.contains(r#""layout":"simple""#) && saved.contains(r#""keyboardLayout":1033"#));
        assert!(serde_json::from_str::<Library>(&text.replace("\"dark\"", "\"unknown\":1,\"dark\"")).is_err());
    }
    #[test]
    fn upsert_validates_and_assigns_an_identifier() {
        let mut library = Library::default();
        let mut p = Profile::draft();
        p.name = "Lab".into();
        p.address = "host:0".into();
        assert_eq!(
            library.upsert(p.clone()),
            Err("The port must be between 1 and 65535.".into())
        );
        p.address = "192.0.2.20".into();
        let id = library.upsert(p).unwrap();
        assert!(uuid::Uuid::parse_str(&id).is_ok());
        assert_eq!(library.get(&id).unwrap().name, "Lab");
    }
}
