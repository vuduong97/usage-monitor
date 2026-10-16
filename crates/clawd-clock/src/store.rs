//! Persistence of the remembered clock: `~/.config/claude-usage-monitor/clock.json`
//! (`discovery.ClockStore`, `stored_clock`). Same schema as Python
//! (`{name, host, port, version}` plus `manual_host` when pinned by hand).

use std::path::PathBuf;

use clawd_contracts::{ClockSource, ClockTarget};
use serde_json::{json, Value};

use crate::clock::manual_clock;
use crate::pyval;

/// `discovery.ClockStore(path)`.
pub struct ClockStore {
    pub path: PathBuf,
}

/// `json.dumps(data, indent=2)`: two-space indent and `ensure_ascii` (every character above `~`
/// is a `\uXXXX` escape, astral ones as a surrogate pair), so the file is byte-identical to the
/// one Python writes.
fn dumps_indented(data: &Value) -> String {
    let text = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".into());
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c <= '~' {
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

impl ClockStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// `ClockStore.load()`: the JSON object, `{}` when missing, unreadable or not an object.
    pub fn load(&self) -> Value {
        let parsed = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        match parsed {
            Some(data @ Value::Object(_)) => data,
            _ => json!({}),
        }
    }

    /// `ClockStore.save(data)`: atomic (temp file + replace), 2-space indent; errors are logged,
    /// not raised.
    pub fn save(&self, data: &Value) {
        let write = || -> std::io::Result<()> {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = self.path.with_extension("tmp");
            std::fs::write(&tmp, dumps_indented(data))?;
            std::fs::rename(&tmp, &self.path)
        };
        if let Err(e) = write() {
            tracing::warn!("Could not save {}: {e}", self.path.display());
        }
    }

    /// `ClockStore.unpin()`: turn a typed address into a remembered one (mDNS may now replace
    /// it) and return the clock to try; `None` when nothing was ever saved.
    pub fn unpin(&self) -> Option<ClockTarget> {
        let mut data = self.load();
        if let Some(typed) = typed_clock(&data) {
            data = json!({"name": "", "host": typed.host, "port": typed.port, "version": ""});
            self.save(&data);
        }
        stored_clock(&data)
    }
}

/// `manual_clock(str(data.get("manual_host") or ""))`.
fn typed_clock(data: &Value) -> Option<ClockTarget> {
    manual_clock(&pyval::str_or_empty(data.get("manual_host")))
}

/// `discovery.stored_clock(data)`: the clock `clock.json` points at: the typed address
/// (`manual_host`, source `Manual`), else the remembered one (source `Saved`).
pub fn stored_clock(data: &Value) -> Option<ClockTarget> {
    if let Some(typed) = typed_clock(data) {
        return Some(typed);
    }
    let host = data.get("host").filter(|h| pyval::truthy(h))?;
    let port = match data.get("port") {
        Some(p) if pyval::truthy(p) => pyval::to_port(p).unwrap_or(80),
        _ => 80,
    };
    let mut clock = ClockTarget::new(pyval::to_str(host), ClockSource::Saved);
    clock.port = port;
    clock.name = pyval::str_or_empty(data.get("name"));
    clock.version = pyval::str_or_empty(data.get("version"));
    Some(clock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumps_like_python() {
        let text = dumps_indented(&json!({"name": "Đồng \u{1F600}", "port": 80, "x": [], "y": {}}));
        assert_eq!(
            text,
            "{\n  \"name\": \"\\u0110\\u1ed3ng \\ud83d\\ude00\",\n  \"port\": 80,\n  \"x\": [],\n  \"y\": {}\n}"
        );
    }

    #[test]
    fn stored_clock_reads_clock_json() {
        assert!(stored_clock(&json!({})).is_none());
        let typed = stored_clock(&json!({"manual_host": "10.0.0.5:8080", "host": "10.0.0.6"}));
        assert_eq!(typed.unwrap().base(), "http://10.0.0.5:8080");
        let c = stored_clock(&json!({"host": "10.0.0.6", "port": "x", "name": "clawd"})).unwrap();
        assert_eq!(
            (c.base().as_str(), c.name.as_str(), c.source),
            ("http://10.0.0.6", "clawd", ClockSource::Saved)
        );
        let bad =
            stored_clock(&json!({"manual_host": "10..97.40.207", "host": "10.0.0.6"})).unwrap();
        assert_eq!(
            (bad.host.as_str(), bad.source),
            ("10.0.0.6", ClockSource::Saved)
        );
    }
}
