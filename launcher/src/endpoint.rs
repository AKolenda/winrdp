// SPDX-License-Identifier: AGPL-3.0-only
//! Computer address validation: the rules of the retired C++ engine's core::parseEndpoint.
use std::net::{Ipv4Addr, Ipv6Addr};

fn alnum(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}
fn ipv6(host: &str) -> bool {
    let (address, zone) = match host.split_once('%') {
        Some((a, z)) => (a, Some(z)),
        None => (host, None),
    };
    if let Some(zone) = zone
        && (zone.is_empty() || zone.len() > 64 || !zone.bytes().all(|c| alnum(c) || matches!(c, b'_' | b'-' | b'.')))
    {
        return false;
    }
    address.parse::<Ipv6Addr>().is_ok()
}
fn hostname(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    if host.contains('.') && host.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return host.parse::<Ipv4Addr>().is_ok();
    }
    // One trailing dot (a fully qualified name) is allowed; empty labels are not.
    host.strip_suffix('.').unwrap_or(host).split('.').all(|label| {
        let b = label.as_bytes();
        !b.is_empty()
            && b.len() <= 63
            && alnum(b[0])
            && alnum(b[b.len() - 1])
            && b.iter().all(|&c| alnum(c) || c == b'-')
    })
}
/// Returns the host and port (3389 unless given) of a computer address.
pub fn parse(input: &str) -> Result<(&str, u16), &'static str> {
    let input = input.trim_matches([' ', '\t', '\n', '\r']);
    if input.is_empty() {
        return Err("Enter a computer name or IP address.");
    }
    if input.len() > 320 {
        return Err("That address is too long.");
    }
    if input.bytes().any(|c| c <= 32 || c >= 127) {
        return Err("Use a hostname or IP address without spaces. Use punycode for international hostnames.");
    }
    if input.contains(['/', '@', '\\', '?', '#']) {
        return Err("Enter just the computer address, not a URL or a username.");
    }
    let (host, port) = if let Some(rest) = input.strip_prefix('[') {
        let end = rest.find(']').ok_or("Close the IPv6 address with ].")?;
        let host = &rest[..end];
        if !ipv6(host) {
            return Err("That IPv6 address is not valid.");
        }
        match &rest[end + 1..] {
            "" => (host, None),
            after => (host, Some(after.strip_prefix(':').ok_or("Use [IPv6 address]:port.")?)),
        }
    } else if input.matches(':').count() > 1 {
        if !ipv6(input) {
            return Err("That IPv6 address is not valid. Use [address]:port for a custom port.");
        }
        (input, None)
    } else {
        let (host, port) = match input.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (input, None),
        };
        if !hostname(host) {
            return Err("That computer name or IPv4 address is not valid.");
        }
        (host, port)
    };
    let port = match port {
        None => 3389,
        Some("") => return Err("Enter a port number after the colon."),
        // Digits only: u16's parser would also accept a leading '+'.
        Some(p) => match p
            .bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| p.parse::<u16>().ok())
            .flatten()
        {
            Some(n) if n >= 1 => n,
            _ => return Err("The port must be between 1 and 65535."),
        },
    };
    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::parse;
    // The retired C++ engine's core_tests cases.
    #[test]
    fn accepts() {
        for s in [
            "desktop",
            "MY-PC",
            "work.example",
            "192.168.1.10",
            "192.168.1.10:3390",
            "[::1]",
            "::1",
            "[fe80::1%eth0]:3390",
            " example.com. ",
        ] {
            assert!(parse(s).is_ok(), "{s:?}");
        }
    }
    #[test]
    fn rejects() {
        for s in [
            "",
            "  ",
            "host:",
            "host:0",
            "host:65536",
            "host:+33",
            "host:3x",
            "host:-1",
            "host:99999999999999999999",
            "host name",
            "https://host",
            "/p:secret",
            "user@host",
            "[xyz]",
            "[::1]x",
            "[::1]:",
            "[::1]:1:2",
            "[::1",
            "-host",
            "a..b",
            "999.1.1.1",
            "host;touch",
            "::gg",
            "fe80::1%",
            "::1%%abc",
            "host\nname",
        ] {
            assert!(parse(s).is_err(), "{s:?}");
        }
    }
    #[test]
    fn ports() {
        assert_eq!(parse("[2001:db8::1]:3391"), Ok(("2001:db8::1", 3391)));
        assert_eq!(parse("pc:3389"), Ok(("pc", 3389)));
        assert_eq!(parse("pc"), Ok(("pc", 3389)));
    }
}
