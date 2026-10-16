//! Typed addresses (`discovery.py:46-103`); the type itself is
//! [`clawd_contracts::ClockTarget`].

use std::net::Ipv4Addr;

use clawd_contracts::{ClockSource, ClockTarget};

/// `discovery.manual_clock(raw)`: `192.168.1.48`, `clawd.local:8080` or `http://10.0.0.5` ->
/// a `Manual` clock; `None` for empty/invalid input. A malformed name (IDNA failure in
/// Python, commit `c6f18fe`) is `None`, never a panic.
pub fn manual_clock(raw: &str) -> Option<ClockTarget> {
    let mut raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some((_, rest)) = raw.split_once("://") {
        raw = rest;
    }
    raw = raw.split('/').next().unwrap_or("");
    let (host, port) = match raw.split_once(':') {
        Some((host, port)) => (host, port),
        None => (raw, ""),
    };
    if !valid_host(host) {
        return None;
    }
    let port = if port.is_empty() {
        80
    } else {
        py_int(port)?
    };
    if !(0 < port && port < 65536) {
        return None;
    }
    let mut clock = ClockTarget::new(host, ClockSource::Manual);
    clock.port = u16::try_from(port).ok()?;
    Some(clock)
}

/// `int(text)` for ASCII digits: surrounding whitespace, one sign, single `_` between digits.
fn py_int(text: &str) -> Option<i64> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let well_formed = !digits.is_empty()
        && !digits.starts_with('_')
        && !digits.ends_with('_')
        && !digits.contains("__")
        && digits.chars().all(|c| c.is_ascii_digit() || c == '_');
    if !well_formed {
        return None;
    }
    let value: i64 = digits.replace('_', "").parse().ok()?;
    Some(if negative { -value } else { value })
}

/// One DNS label: 1..=63 letters, digits or hyphens, not starting or ending with a hyphen
/// (`discovery._LABEL`).
fn valid_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    (1..=63).contains(&bytes.len())
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
}

/// `discovery.valid_host(host)`: an IP address or a DNS name of valid labels.
///
/// A full IPv4 address or a DNS name (`clawd.local`). Rejects a typo such as `10..97.40.207`
/// (an empty label) and a short dotted number such as `10.97.40` (a resolver would read it as
/// `10.97.0.40`).
pub fn valid_host(host: &str) -> bool {
    if host.is_empty() || host.chars().count() > 253 {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels
        .iter()
        .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()))
    {
        return host.parse::<Ipv4Addr>().is_ok();
    }
    labels.iter().all(|l| valid_label(l))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_addresses() {
        assert_eq!(manual_clock("clawd.local:8080").unwrap().base(), "http://clawd.local:8080");
        assert_eq!(
            manual_clock("http://10.0.0.5/whatever").unwrap().usage_url(),
            "http://10.0.0.5/api/usage"
        );
        assert!(manual_clock("").is_none() && manual_clock(":80").is_none());
        assert!(manual_clock("h:abc").is_none() && manual_clock("h:0").is_none());
        assert!(manual_clock("h:65536").is_none());
        assert_eq!(manual_clock("h: 8_0 ").unwrap().port, 80);
    }

    #[test]
    fn malformed_hosts_are_rejected() {
        for bad in [
            "10..97.40.207",
            "10.97.40",
            "10.97.40.256",
            ".clawd",
            "clawd.",
            "a b",
            "-x.local",
            &"x".repeat(64),
        ] {
            assert!(manual_clock(bad).is_none(), "{bad}");
        }
        assert!(valid_host("clawd-home.local") && valid_host("clawd") && valid_host("10.97.40.207"));
    }
}
