//! Payload helpers (`usage.py:503-545, 1054-1110`): clock fields, ASCII folding, the attention
//! flag the Claude Code hooks leave, size-limited JSON encoding.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::time::UNIX_EPOCH;

use clawd_config::{detect_hour_format, read_chime_setting, read_clock_setting};
use clawd_contracts::{Clock, UsagePayload};
use serde_json::{json, Value};
use unicode_normalization::UnicodeNormalization;

use crate::util::{is_py_space, local_offset_secs, py_str_or_empty};
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
    if read_chime_setting(&env.paths) == "on" {
        payload.insert("c".into(), json!(1));
    }
}

/// `usage.add_clock_fields(payload)`: unless `clock = off`, add `"t"` (local wall-clock epoch =
/// UTC epoch + tz offset) and `"tf"` (12/24; `clock = auto` asks `detect_hour_format`).
pub fn add_clock_fields(env: &UsageEnv, payload: &mut UsagePayload) {
    let clock = read_clock_setting(&env.paths);
    if clock == "off" {
        return;
    }
    let tf = match clock.as_str() {
        "24" => 24,
        "12" => 12,
        _ => i64::from(detect_hour_format(env.runner.as_ref())),
    };
    let now = env.clock.now();
    payload.insert(
        "t".into(),
        json!(now.floor() as i64 + local_offset_secs(now)),
    );
    payload.insert("tf".into(), json!(tf));
}

/// `usage.to_ascii(text, limit)`: the device fonts are ASCII: drop accents, collapse
/// whitespace, cap at `limit` (with `...`).
pub fn to_ascii(text: &str, limit: usize) -> String {
    let folded: String = text.nfkd().filter(char::is_ascii).collect();
    let collapsed = folded
        .split(is_py_space)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let len = collapsed.chars().count();
    if len <= limit {
        return collapsed;
    }
    // Python slices `text[: limit - 3]`; a negative bound counts from the end.
    let bound = limit as isize - 3;
    let keep = if bound >= 0 {
        bound as usize
    } else {
        len.saturating_sub(bound.unsigned_abs())
    };
    let cut: String = collapsed.chars().take(keep).collect();
    format!("{}...", cut.trim_end_matches(is_py_space))
}

fn mtime_secs(meta: &fs::Metadata) -> Option<f64> {
    let modified = meta.modified().ok()?;
    Some(match modified.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs_f64(),
        Err(e) => -e.duration().as_secs_f64(),
    })
}

/// `usage.take_attention_flag()`: consume the `attention` file. Only a fresh one
/// ([`ATTENTION_MAX_AGE`]) counts; a bare `touch` with no JSON still beeps, without a card.
/// `now` = epoch seconds, `None` = the real time.
pub fn take_attention_flag(env: &UsageEnv, now: Option<f64>) -> Option<Attention> {
    let flag = env.paths.attention_flag();
    let read = || -> io::Result<(f64, String)> {
        let meta = fs::metadata(&flag)?;
        let raw = String::from_utf8_lossy(&fs::read(&flag)?).into_owned();
        fs::remove_file(&flag)?;
        Ok((mtime_secs(&meta).unwrap_or(0.0), raw))
    };
    let (mtime, raw) = match read() {
        Ok(v) => v,
        Err(e) => {
            if e.kind() != io::ErrorKind::NotFound {
                tracing::warn!("attention flag unreadable: {e}");
            }
            return None;
        }
    };
    let now = now.unwrap_or_else(|| env.clock.now());
    if now - mtime >= ATTENTION_MAX_AGE {
        return None;
    }
    let data = serde_json::from_str::<Value>(&raw).ok();
    let field = |name: &str| py_str_or_empty(data.as_ref().and_then(|d| d.get(name)));
    Some(Attention {
        m: to_ascii(&field("m"), 120),
        s: field("s"),
        t: mtime,
    })
}

/// `usage.take_clears()`: consume every `clear-<session id>` file; `{session id: mtime}`.
pub fn take_clears(env: &UsageEnv) -> HashMap<String, f64> {
    let mut clears = HashMap::new();
    let Ok(entries) = fs::read_dir(env.paths.attention_dir()) else {
        return clears;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(session) = name.strip_prefix("clear-") else {
            continue;
        };
        let path = entry.path();
        let Some(mtime) = fs::metadata(&path).ok().as_ref().and_then(mtime_secs) else {
            continue;
        };
        if fs::remove_file(&path).is_ok() {
            clears.insert(session.to_owned(), mtime);
        }
    }
    clears
}

/// `json.dumps(payload, separators=(",", ":"))`: compact, ASCII-only (non-ASCII is `\uXXXX`
/// escaped, as Python's default `ensure_ascii`).
pub(crate) fn dumps(payload: &UsagePayload) -> String {
    let text = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_owned());
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// `usage.encode_payload(payload, limit)`: compact JSON (`separators=(",", ":")`, Python key
/// order) of at most `limit` bytes: shortens the alert message `m`, then drops it, until it fits.
pub fn encode_payload(payload: &UsagePayload, limit: usize) -> Vec<u8> {
    let mut payload = payload.clone();
    let mut data = dumps(&payload);
    let mut msg: String = payload
        .get("m")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    while data.len() > limit && !msg.is_empty() {
        let excess = data.len() - limit;
        let keep = (msg.chars().count() as isize - excess as isize - 3).max(0) as usize;
        let cut: String = msg.chars().take(keep).collect();
        msg = cut.trim_end_matches(is_py_space).to_owned();
        let shown = if msg.is_empty() {
            String::new()
        } else {
            format!("{msg}...")
        };
        payload.insert("m".into(), json!(shown));
        data = dumps(&payload);
    }
    if data.len() > limit {
        let without_m: serde_json::Map<String, Value> = payload
            .iter()
            .filter(|(k, _)| k.as_str() != "m")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        data = dumps(&UsagePayload(without_m));
    }
    data.into_bytes()
}
