//! Payload helpers (`usage.py:503-545, 1054-1110`): clock fields, ASCII folding, the attention
//! flag the Claude Code hooks leave, size-limited JSON encoding.

use std::collections::HashMap;

use clawd_contracts::UsagePayload;

use crate::UsageEnv;

/// `usage.ATTENTION_MAX_AGE` (seconds): an older attention file does not beep.
pub const ATTENTION_MAX_AGE: f64 = 60.0;
/// `usage.DEFAULT_MAX_WRITE`.
pub const DEFAULT_MAX_WRITE: usize = 180;

/// What `take_attention_flag` returns: `{"m": message, "s": session id, "t": mtime}`.
#[derive(Debug, Clone, PartialEq)]
pub struct Attention {
    /// Alert text (ASCII, at most 120 chars).
    pub m: String,
    /// Session id (may be empty).
    pub s: String,
    /// File mtime (epoch seconds).
    pub t: f64,
}

/// `usage.add_chime_field(payload)`: add `"c": 1` when the config's `chime` is `on`.
pub fn add_chime_field(env: &UsageEnv, payload: &mut UsagePayload) {
    todo!("phase 07")
}

/// `usage.add_clock_fields(payload)`: unless `clock = off`, add `"t"` (local wall-clock epoch =
/// UTC epoch + tz offset) and `"tf"` (12/24; `clock = auto` asks `detect_hour_format`).
pub fn add_clock_fields(env: &UsageEnv, payload: &mut UsagePayload) {
    todo!("phase 07")
}

/// `usage.to_ascii(text, limit)`: the device fonts are ASCII: drop accents, collapse
/// whitespace, cap at `limit` (with `...`).
pub fn to_ascii(text: &str, limit: usize) -> String {
    todo!("phase 07")
}

/// `usage.take_attention_flag()`: consume the `attention` file. Only a fresh one
/// ([`ATTENTION_MAX_AGE`]) counts; a bare `touch` with no JSON still beeps, without a card.
/// `now` = epoch seconds, `None` = the real time.
pub fn take_attention_flag(env: &UsageEnv, now: Option<f64>) -> Option<Attention> {
    todo!("phase 07")
}

/// `usage.take_clears()`: consume every `clear-<session id>` file; `{session id: mtime}`.
pub fn take_clears(env: &UsageEnv) -> HashMap<String, f64> {
    todo!("phase 07")
}

/// `usage.encode_payload(payload, limit)`: compact JSON (`separators=(",", ":")`, Python key
/// order) of at most `limit` bytes: shortens the alert message `m`, then drops it, until it fits.
pub fn encode_payload(payload: &UsagePayload, limit: usize) -> Vec<u8> {
    todo!("phase 07")
}
