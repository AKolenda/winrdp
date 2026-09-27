//! Classifies session failures and publishes connection state for the launcher.
//!
//! The status file is the boundary between the session process and its launcher.
//! Keep its state names, failure tags and transport labels stable.

use tracing::{error, warn};

/// Human-readable transport, as shown in the window title: `UDP v2` when graphics
/// flow over the reliable RDP-UDP tunnel, `TCP` otherwise.
pub(crate) fn transport_label(reliable_udp: bool, udp_version: Option<u16>) -> String {
    match (reliable_udp, udp_version) {
        (true, Some(version)) => format!("UDP v{version}"),
        (true, None) => "UDP".to_owned(),
        (false, _) => "TCP".to_owned(),
    }
}

/// Publish the transport for the launcher, which shows it on the session tab.
/// The file named by `WINRDP_STATUS_FILE` is replaced atomically.
pub(crate) fn write_status_file(reliable_udp: bool, udp_version: Option<u16>) {
    write_status(&transport_status(reliable_udp, udp_version));
}

/// The transport record the launcher reads. It must keep carrying `label`: that
/// is both what the tab shows and how the launcher tells a transport record
/// apart from the failure record written when a session is refused.
fn transport_status(reliable_udp: bool, udp_version: Option<u16>) -> String {
    let version = udp_version.map_or("null".to_owned(), |v| v.to_string());
    format!(
        "{{\"state\":\"connected\",\"transport\":\"{}\",\"udpVersion\":{version},\"label\":\"{}\"}}\n",
        if reliable_udp { "udp" } else { "tcp" },
        transport_label(reliable_udp, udp_version)
    )
}

/// Replaces the file named by `WINRDP_STATUS_FILE` atomically. Nothing to do
/// when the session was started by hand rather than by the launcher.
fn write_status(json: &str) {
    let Some(path) = std::env::var_os("WINRDP_STATUS_FILE") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let tmp = path.with_extension("tmp");
    if let Err(error) = std::fs::write(&tmp, json).and_then(|()| std::fs::rename(&tmp, &path)) {
        warn!(%error, ?path, "Could not write the session status file");
    }
}

/// Minimal JSON string escaping, so a server-supplied reason cannot break the
/// status file the launcher parses.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' => out.push(' '),
            '\t' => out.push(' '),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

/// Why a session could not start, or stopped, in the words the launcher shows.
///
/// `reason` is a stable tag the launcher acts on: `credentials` makes it ask for
/// the password again, `account` reports a sign-in the account itself cannot
/// make, and the rest are informational. `detail` is the engine's own error
/// text, shown under the sentence for the non-account reasons.
pub(crate) struct Failure {
    pub(crate) reason: &'static str,
    pub(crate) message: String,
    pub(crate) detail: String,
}

const WRONG_CREDENTIALS: &str = "The user name or password is incorrect.";

/// The context the engine attaches to a failed TLS handshake. `ConnectorError`
/// exposes its context only through the rendered report, so this is matched
/// against that text.
const TLS_UPGRADE: &str = "TLS upgrade";

/// Windows states exactly why a sign-in was refused in the CredSSP NTSTATUS.
/// Only a mistyped name or password is worth asking again for; the rest name a
/// condition on the account that retyping cannot change.
fn logon_status(code: ironrdp::connector::sspi::credssp::NStatusCode) -> Option<(&'static str, &'static str)> {
    use ironrdp::connector::sspi::credssp::NStatusCode as N;

    let known = if code == N::WRONG_PASSWORD
        || code == N::LOGON_FAILURE
        || code == N::NO_SUCH_USER
        || code == N::INVALID_ACCOUNT_NAME
    {
        ("credentials", WRONG_CREDENTIALS)
    } else if code == N::ACCOUNT_LOCKED_OUT {
        (
            "account",
            "That account is locked out on the Windows computer. Wait, or ask an administrator to unlock it.",
        )
    } else if code == N::PASSWORD_EXPIRED || code == N::PASSWORD_MUST_CHANGE {
        (
            "account",
            "That password has expired. Change it on the Windows computer, then connect again.",
        )
    } else if code == N::ACCOUNT_DISABLED {
        ("account", "That account is disabled on the Windows computer.")
    } else if code == N::INVALID_LOGON_HOURS {
        ("account", "That account is not allowed to sign in at this time of day.")
    } else if code == N::INVALID_WORKSTATION {
        ("account", "That account is not allowed to sign in from this computer.")
    } else if code == N::LOGON_TYPE_NOT_GRANTED || code == N::LOGON_NOT_GRANTED {
        (
            "account",
            "That account may not sign in over Remote Desktop. Add it to the Remote Desktop Users group on the Windows computer.",
        )
    } else if code == N::SMARTCARD_LOGON_REQUIRED {
        ("account", "That account must sign in with a smart card.")
    } else if code == N::ACCOUNT_RESTRICTION {
        (
            "account",
            "Windows refused this account. A blank password is not accepted over Remote Desktop.",
        )
    } else if code == N::NO_LOGON_SERVERS {
        (
            "server",
            "No domain controller answered, so the account could not be checked.",
        )
    } else {
        return None;
    };
    Some(known)
}

/// True when the failure bottoms out in an I/O error, which is what an
/// unreachable or refusing host looks like from here.
fn is_io_failure(error: &(dyn core::error::Error + 'static)) -> bool {
    let mut next = error.source();
    while let Some(source) = next {
        if source.is::<std::io::Error>() {
            return true;
        }
        next = source.source();
    }
    false
}

/// Turns a connect-time error into something worth showing a person.
pub(crate) fn connect_failure(error: &ironrdp::connector::ConnectorError) -> Failure {
    use ironrdp::connector::{ConnectorErrorKind as Kind, sspi};

    let detail = error.report().to_string();
    let (reason, message) = match error.kind() {
        Kind::AccessDenied => ("credentials", WRONG_CREDENTIALS.to_owned()),
        Kind::Credssp(source) => source
            .nstatus
            .and_then(logon_status)
            .map(|(reason, message)| (reason, message.to_owned()))
            .unwrap_or_else(|| {
                if source.error_type == sspi::ErrorKind::LogonDenied {
                    ("credentials", WRONG_CREDENTIALS.to_owned())
                } else {
                    ("server", format!("Windows refused the sign-in: {}.", source.description))
                }
            }),
        Kind::Negotiation(failure) => (
            "server",
            format!("The Windows computer refused the connection: {failure}."),
        ),
        Kind::Reason(reason) => (
            "server",
            format!("The Windows computer refused the connection: {reason}."),
        ),
        Kind::Encode(_) | Kind::Decode(_) => (
            "protocol",
            "The Windows computer sent something this client could not read.".to_owned(),
        ),
        // The host answered on the RDP port and then failed the secure handshake,
        // so the "is it switched on?" advice below would send the user the wrong way.
        _ if detail.contains(TLS_UPGRADE) => (
            "protocol",
            "The Windows computer answered, but the secure connection could not be set up.".to_owned(),
        ),
        _ if is_io_failure(error) => (
            "network",
            "Could not reach the Windows computer. Check that it is switched on, reachable, and has Remote Desktop turned on.".to_owned(),
        ),
        kind => ("other", format!("The connection failed: {kind}.")),
    };
    Failure {
        reason,
        message,
        detail,
    }
}

/// Publishes why the session is stopping, for the launcher to show.
pub(crate) fn write_failure_status(failure: &Failure) {
    write_status(&format!(
        "{{\"state\":\"failed\",\"reason\":\"{}\",\"message\":\"{}\",\"detail\":\"{}\"}}\n",
        failure.reason,
        escape(&failure.message),
        escape(&failure.detail)
    ));
}

/// Publishes an ordinary end of session, so the launcher stays quiet about it.
pub(crate) fn write_closed_status() {
    write_status("{\"state\":\"closed\"}\n");
}

/// What a session that never got as far as a window exits with. `EX_CONFIG` from
/// `sysexits`, so a caller can tell it from a refused sign-in.
pub const STARTUP_FAILURE_EXIT: i32 = 78;

/// Publishes a failure that happened before there was a window to show it in.
///
/// Everything the launcher knows about a session comes from the status file and
/// the log. A configuration error that only reaches stderr is discarded, and the
/// launcher then tells the user to read a log that was never opened. Returns the
/// exit code the process should use.
pub fn report_startup_failure(error: &anyhow::Error) -> i32 {
    let detail = format!("{error:#}");
    error!(%detail, "The session could not start");
    write_failure_status(&Failure {
        reason: "config",
        message: "This computer's connection settings could not be used.".to_owned(),
        detail,
    });
    STARTUP_FAILURE_EXIT
}

#[cfg(test)]
mod tests {
    use ironrdp::connector::sspi::credssp::NStatusCode;
    use ironrdp::connector::{ConnectorError, ConnectorErrorKind, sspi};

    use super::{WRONG_CREDENTIALS, connect_failure, escape, transport_status};

    #[test]
    fn a_connected_session_still_publishes_the_transport_the_launcher_shows() {
        assert_eq!(
            transport_status(true, Some(2)),
            "{\"state\":\"connected\",\"transport\":\"udp\",\"udpVersion\":2,\"label\":\"UDP v2\"}\n"
        );
        assert_eq!(
            transport_status(false, None),
            "{\"state\":\"connected\",\"transport\":\"tcp\",\"udpVersion\":null,\"label\":\"TCP\"}\n"
        );
    }

    fn credssp(error: sspi::Error) -> ConnectorError {
        ConnectorError::new("CredSSP", ConnectorErrorKind::Credssp(error))
    }

    #[test]
    fn early_user_auth_denial_is_a_credentials_failure() {
        let failure = connect_failure(&ConnectorError::new("CredSSP", ConnectorErrorKind::AccessDenied));
        assert_eq!(failure.reason, "credentials");
        assert_eq!(failure.message, WRONG_CREDENTIALS);
    }

    #[test]
    fn a_mistyped_password_is_worth_asking_again_for() {
        for code in [
            NStatusCode::WRONG_PASSWORD,
            NStatusCode::LOGON_FAILURE,
            NStatusCode::NO_SUCH_USER,
            NStatusCode::INVALID_ACCOUNT_NAME,
        ] {
            let failure = connect_failure(&credssp(sspi::Error::new_with_nstatus(
                sspi::ErrorKind::LogonDenied,
                "logon failed",
                code,
            )));
            assert_eq!(failure.reason, "credentials", "{code}");
            assert_eq!(failure.message, WRONG_CREDENTIALS);
        }
    }

    #[test]
    fn a_condition_on_the_account_is_reported_but_not_retried() {
        let failure = connect_failure(&credssp(sspi::Error::new_with_nstatus(
            sspi::ErrorKind::LogonDenied,
            "locked out",
            NStatusCode::ACCOUNT_LOCKED_OUT,
        )));
        assert_eq!(failure.reason, "account");
        assert!(failure.message.contains("locked out"), "{}", failure.message);
    }

    #[test]
    fn credssp_without_a_status_still_names_the_credentials() {
        let failure = connect_failure(&credssp(sspi::Error::new(sspi::ErrorKind::LogonDenied, "denied")));
        assert_eq!(failure.reason, "credentials");
    }

    #[test]
    fn an_unreachable_host_is_a_network_failure_not_a_password_one() {
        let error = ConnectorError::new("connect", ConnectorErrorKind::Custom)
            .with_source(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));
        let failure = connect_failure(&error);
        assert_eq!(failure.reason, "network");
    }

    #[test]
    fn a_refusal_reason_from_the_server_is_passed_on() {
        let failure = connect_failure(&ConnectorError::new(
            "negotiation",
            ConnectorErrorKind::Reason("SSL required by server".to_owned()),
        ));
        assert_eq!(failure.reason, "server");
        assert!(
            failure.message.contains("SSL required by server"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn status_text_cannot_break_out_of_the_json_the_launcher_parses() {
        assert_eq!(escape("say \"hi\"\\ now"), "say \\\"hi\\\"\\\\ now");
        assert_eq!(escape("two\nlines\u{1}"), "two lines");
    }

    #[test]
    fn a_failed_secure_handshake_is_not_reported_as_an_unreachable_computer() {
        let error = ConnectorError::new("TLS upgrade", ConnectorErrorKind::Custom)
            .with_source(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        let failure = connect_failure(&error);
        assert_eq!(failure.reason, "protocol");
        assert!(failure.message.contains("secure connection"), "{}", failure.message);
    }
}
