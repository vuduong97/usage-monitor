//! Typed addresses (`discovery.py:46-103`); the type itself is
//! [`clawd_contracts::ClockTarget`].

use clawd_contracts::ClockTarget;

/// `discovery.manual_clock(raw)`: `192.168.1.48`, `clawd.local:8080` or `http://10.0.0.5` ->
/// a `Manual` clock; `None` for empty/invalid input. A malformed name (IDNA failure in
/// Python, commit `c6f18fe`) is `None`, never a panic.
pub fn manual_clock(raw: &str) -> Option<ClockTarget> {
    todo!("phase 08")
}

/// `discovery.valid_host(host)`: an IP address or a DNS name of valid labels.
pub fn valid_host(host: &str) -> bool {
    todo!("phase 08")
}
