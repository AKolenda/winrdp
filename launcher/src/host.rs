// SPDX-License-Identifier: AGPL-3.0-only
//! Letting Windows connect *into* this computer. Win RDP is a client; the host side is
//! GNOME Remote Desktop, driven through `grdctl`, `gsettings` and `systemctl --user`.
//! Every call here blocks for up to ten seconds, so the window runs them off its thread.
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// `mirror-primary`: Windows sees the physical screen, which needs a monitor that is on.
    #[default]
    Mirror,
    /// `extend`: a virtual screen in the same signed-in session; works with no monitor attached.
    Extend,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// GNOME Remote Desktop is installed.
    pub available: bool,
    /// Desktop Sharing, which mirrors the desktop that is signed in, is running.
    pub sharing: bool,
    /// Headless "Remote Login", which gives Windows a separate session with its own login, is running.
    pub headless: bool,
    pub extend: bool,
    pub credentials: bool,
    pub port: u16,
}

/// One command with a timeout. Never involves a shell.
fn sh(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("timeout")
        .arg("10")
        .arg(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
    // Arguments and child diagnostics may contain a sharing password. Never surface them.
    else {
        Err(format!("{program} failed ({}).", output.status))
    }
}
fn user_unit_active(unit: &str) -> bool {
    sh("systemctl", &["--user", "is-active", unit]).is_ok_and(|s| s.trim() == "active")
}

/// Which of GNOME Remote Desktop's modes answers on the RDP port, so Settings can say so truthfully.
pub fn status() -> Status {
    let Ok(text) = sh("grdctl", &["status"]) else {
        return Status {
            port: 3389,
            ..Status::default()
        };
    };
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_owned()))
    };
    let mode = sh(
        "gsettings",
        &["get", "org.gnome.desktop.remote-desktop.rdp", "screen-share-mode"],
    )
    .unwrap_or_default();
    Status {
        available: true,
        sharing: user_unit_active("gnome-remote-desktop.service"),
        headless: user_unit_active("gnome-remote-desktop-headless.service"),
        extend: mode.contains("extend"),
        credentials: field("Username:").is_some_and(|u| !u.is_empty() && u != "(none)"),
        port: field("Port:").and_then(|p| p.parse().ok()).unwrap_or(3389),
    }
}

/// Makes this desktop reachable from Windows the way the user expects: share the signed-in
/// desktop on the RDP port, on now and at every login, and turn the separate-session headless
/// mode off so it releases the port. Credentials are only replaced when a name is given.
pub fn enable(username: &str, password: &str, mode: Mode) -> Result<(), String> {
    let share_mode = match mode {
        Mode::Extend => "extend",
        Mode::Mirror => "mirror-primary",
    };
    if !username.trim().is_empty() {
        if password.is_empty() || password.len() > 512 || password.contains('\0') {
            return Err("Enter a sharing password.".into());
        }
        // `grdctl rdp set-credentials` takes the password as a positional argument and
        // offers no stdin or file alternative, so it is readable in /proc/<pid>/cmdline
        // for as long as that one call runs. sh() still never involves a shell, so the
        // value cannot leak into a history file or a child's environment, and the
        // command line is never echoed back into an error message.
        sh("grdctl", &["rdp", "set-credentials", username.trim(), password])?;
    }
    let _ = sh(
        "systemctl",
        &["--user", "disable", "--now", "gnome-remote-desktop-headless.service"],
    );
    sh(
        "gsettings",
        &[
            "set",
            "org.gnome.desktop.remote-desktop.rdp",
            "screen-share-mode",
            share_mode,
        ],
    )?;
    sh("grdctl", &["rdp", "enable"])?;
    sh("grdctl", &["rdp", "disable-view-only"])?;
    sh(
        "systemctl",
        &["--user", "enable", "--now", "gnome-remote-desktop.service"],
    )?;
    // A restart picks up the mode and credential changes when the service was already running.
    let _ = sh("systemctl", &["--user", "restart", "gnome-remote-desktop.service"]);
    Ok(())
}

/// Stops accepting connections into this desktop, now and at login.
pub fn disable() -> Result<(), String> {
    sh(
        "systemctl",
        &["--user", "disable", "--now", "gnome-remote-desktop.service"],
    )
    .map(|_| ())
}

/// Opens GNOME's Remote Desktop settings, where sharing this desktop is switched on.
pub fn open_settings() -> Result<(), String> {
    for panel in ["remote-desktop", "sharing"] {
        if Command::new("gnome-control-center")
            .arg(panel)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
    }
    Err("Could not open GNOME Settings. Open Settings, then System, then Remote Desktop.".into())
}

#[cfg(test)]
mod tests {
    use super::sh;

    const SECRET: &str = "dummy-secret-regression-test";

    /// A command that fails after printing its argument, a stand-in for a password, to
    /// both of its outputs.
    fn failing_with_the_secret() -> String {
        let script = "printf '%s' \"$1\"; printf '%s' \"$1\" >&2; exit 7";
        sh("sh", &["-c", script, "sh", SECRET]).expect_err("the command fails")
    }

    #[test]
    fn a_failed_command_never_repeats_what_it_printed() {
        assert!(
            !failing_with_the_secret().contains(SECRET),
            "child output can carry a password"
        );
    }

    #[test]
    fn a_failed_command_never_repeats_its_arguments() {
        assert!(!failing_with_the_secret().contains("printf"));
    }

    #[test]
    fn a_failed_command_keeps_its_exit_status() {
        assert!(failing_with_the_secret().contains('7'));
    }

    #[test]
    fn a_successful_command_returns_its_output() {
        assert_eq!(sh("printf", &["%s", "ok"]), Ok("ok".to_owned()));
    }
}
