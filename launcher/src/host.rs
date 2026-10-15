// SPDX-License-Identifier: AGPL-3.0-only
//! Letting Windows connect *into* this computer. Win RDP is a client; the host side is
//! GNOME Remote Desktop, driven through `grdctl`, `gsettings` and `systemctl --user`.
//! Every call here blocks for up to ten seconds, so the window runs them off its thread.
use std::io::Write;
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

/// Sends a single secret line over a private pipe. Neither the timeout wrapper nor
/// the child receives the secret in its command line, and child output is discarded.
fn sh_input(program: &str, args: &[&str], secret: &str) -> Result<(), String> {
    let mut child = Command::new("timeout")
        .arg("10")
        .arg(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let sent = child
        .stdin
        .take()
        .ok_or_else(|| "Could not open the sharing password input.".to_owned())
        .and_then(|mut input| {
            input
                .write_all(secret.as_bytes())
                .and_then(|()| input.write_all(b"\n"))
                .map_err(|_| "Could not send the sharing password.".to_owned())
        });
    // Always reap the child, including when it rejects stdin. The timeout also
    // covers a keyring prompt or a child that stops reading.
    let status = child.wait().map_err(|e| format!("{program}: {e}"))?;
    sent?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed ({status})."))
    }
}
fn user_unit_active(unit: &str) -> bool {
    sh("systemctl", &["--user", "is-active", unit]).is_ok_and(|s| s.trim() == "active")
}

fn status_field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix(key).map(str::trim))
}

fn credentials_present(text: &str) -> bool {
    ["Username:", "Password:"].into_iter().all(|key| {
        status_field(text, key)
            .is_some_and(|value| !value.is_empty() && !matches!(value, "(none)" | "(empty)" | "(null)"))
    })
}

/// Which of GNOME Remote Desktop's modes answers on the RDP port, so Settings can say so truthfully.
pub fn status() -> Status {
    let Ok(text) = sh("grdctl", &["status"]) else {
        return Status {
            port: 3389,
            ..Status::default()
        };
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
        credentials: credentials_present(&text),
        port: status_field(&text, "Port:")
            .and_then(|p| p.parse().ok())
            .unwrap_or(3389),
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
        if username.contains(['\0', '\r', '\n']) {
            return Err("The sharing name cannot contain line breaks or NUL characters.".into());
        }
        // `grdctl status` reports one-character values as `(empty)`, so Settings
        // would treat them as missing after sharing starts.
        if username.trim().chars().count() < 2 {
            return Err("Enter a sharing name of at least 2 characters.".into());
        }
        if password.chars().count() < 2 || password.len() > 512 || password.contains(['\0', '\r', '\n']) {
            return Err("Enter a sharing password of 2 to 512 bytes without line breaks.".into());
        }
        // Current grdctl reads an omitted password from stdin. Older versions
        // fail safely; never fall back to exposing it in process arguments.
        sh_input("grdctl", &["rdp", "set-credentials", username.trim()], password).map_err(|error| {
            format!(
                "{error} If your grdctl cannot read passwords from standard input, set the sharing password in GNOME Settings, then refresh."
            )
        })?;
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
    use super::{Mode, credentials_present, enable, sh, sh_input};

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

    #[test]
    fn a_secret_arrives_on_stdin_without_becoming_an_argument() {
        let script = "IFS= read -r value; test \"$value\" = dummy-secret-regression-test && test \"$#\" = 0";
        assert_eq!(sh_input("sh", &["-c", script, "sh"], SECRET), Ok(()));
    }

    #[test]
    fn a_failed_stdin_command_does_not_repeat_its_secret() {
        let script = "IFS= read -r value; printf '%s' \"$value\"; printf '%s' \"$value\" >&2; exit 7";
        let error = sh_input("sh", &["-c", script, "sh"], SECRET).unwrap_err();
        assert!(!error.contains(SECRET));
        assert!(error.contains('7'));
    }

    #[test]
    fn sharing_credentials_cannot_inject_another_input_line() {
        for password in ["first\nsecond", "first\rsecond", "first\0second"] {
            assert!(enable("audit-user", password, Mode::Mirror).is_err());
        }
        for username in ["first\nsecond", "first\rsecond", "first\0second"] {
            assert!(enable(username, SECRET, Mode::Mirror).is_err());
        }
    }

    #[test]
    fn one_character_sharing_credentials_are_rejected() {
        assert!(enable("audit-user", "x", Mode::Mirror).is_err());
        assert!(enable(" a ", SECRET, Mode::Mirror).is_err());
    }

    #[test]
    fn empty_or_partial_sharing_credentials_are_not_treated_as_configured() {
        for missing in ["", "(none)", "(empty)", "(null)"] {
            assert!(!credentials_present(&format!(
                "RDP:\n\tUsername: {missing}\n\tPassword: (hidden)\n"
            )));
            assert!(!credentials_present(&format!(
                "RDP:\n\tUsername: (hidden)\n\tPassword: {missing}\n"
            )));
        }
        assert!(!credentials_present("RDP:\n\tUsername: (hidden)\n"));
        assert!(!credentials_present("RDP:\n\tPassword: (hidden)\n"));
        // GNOME 46.2 and current releases both mask configured values this way.
        assert!(credentials_present(
            "RDP:\n\tUsername: (hidden)\n\tPassword: (hidden)\n"
        ));
    }
}
