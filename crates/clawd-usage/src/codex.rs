//! Codex CLI usage for the clock, read from Codex's own session logs; no network
//! (`codex_usage.py`).
//!
//! Codex writes a `token_count` event into its session file
//! (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, `CODEX_HOME` moves it) after each model
//! reply, carrying the account's rate limits. The numbers are as fresh as the last Codex turn: a
//! window whose reset time has passed reads as 0 %. Plans with null windows: no data.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use clawd_contracts::{AppPaths, Clock, Env, SystemClock, UsagePayload};
use serde_json::{json, Value};

use crate::util::{expand_user, parse_iso_timestamp, py_float, py_int, round_i64};

/// `codex_usage.TAIL`: bytes read from the end of a session file.
pub const TAIL: u64 = 256 * 1024;
/// `codex_usage.FILES`: newest session files looked at.
pub const FILES: usize = 8;
/// `codex_usage.DAY` (minutes).
const DAY: i64 = 24 * 60;

type CacheKey = (PathBuf, u128, u64);
type CacheEntry = Option<(Value, f64)>;

/// `codex_usage._cache`: parsed limits per (file, mtime, size), so an idle poll re-reads nothing.
fn cache() -> &'static Mutex<HashMap<CacheKey, CacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `codex_usage.sessions_dir(home, env)`: `$CODEX_HOME/sessions` or `<home>/.codex/sessions`.
/// `env` is `UsageEnv.env` (`Env::system()` in the app, a map in tests).
pub fn sessions_dir(paths: &AppPaths, env: &Env) -> PathBuf {
    let base = match env.get("CODEX_HOME").filter(|v| !v.is_empty()) {
        Some(raw) => expand_user(paths, &raw),
        None => paths.home().join(".codex"),
    };
    base.join("sessions")
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    dirs
}

fn mtime_of(path: &Path) -> Option<f64> {
    let m = fs::metadata(path).ok()?.modified().ok()?;
    Some(match m.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs_f64(),
        Err(e) => -e.duration().as_secs_f64(),
    })
}

/// `codex_usage.newest_files(root, limit)`: newest session files, walking `YYYY/MM/DD` folders
/// newest first (no full scan).
pub fn newest_files(root: &Path, limit: usize) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for year in subdirs(root) {
        for month in subdirs(&year) {
            for day in subdirs(&month) {
                let Ok(entries) = fs::read_dir(&day) else {
                    continue;
                };
                let mut files: Vec<(f64, PathBuf)> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .is_some_and(|x| x.eq_ignore_ascii_case("jsonl"))
                    })
                    .filter_map(|p| mtime_of(&p).map(|t| (t, p)))
                    .collect();
                files.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
                out.extend(files.into_iter().map(|(_, p)| p));
                if out.len() >= limit {
                    out.truncate(limit);
                    return out;
                }
            }
        }
    }
    out
}

fn tail_lines(path: &Path) -> Vec<String> {
    let read = || -> std::io::Result<(u64, Vec<u8>)> {
        let mut f = fs::File::open(path)?;
        let size = f.seek(SeekFrom::End(0))?;
        f.seek(SeekFrom::Start(size.saturating_sub(TAIL)))?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        Ok((size, data))
    };
    let Ok((size, data)) = read() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&data);
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    if size > TAIL && !lines.is_empty() {
        lines.remove(0); // the first may be cut
    }
    lines
}

/// `codex_usage._when(ts, path)`: when a row was written.
fn when(ts: Option<&Value>, path: &Path) -> f64 {
    if let Some(Value::String(s)) = ts {
        if let Some(t) = parse_iso_timestamp(s) {
            return t;
        }
    }
    mtime_of(path).unwrap_or_else(|| SystemClock.now())
}

/// `codex_usage._limits_in(path)`: the last `rate_limits` with a window in one file, and when
/// they were written.
fn limits_in(path: &Path) -> Option<(Value, f64)> {
    for line in tail_lines(path).iter().rev() {
        if !line.contains("\"rate_limits\"") {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(rl) = row.get("payload").and_then(|p| p.get("rate_limits")) else {
            continue;
        };
        let Some(rl_obj) = rl.as_object() else {
            continue;
        };
        if !matches!(rl_obj.get("limit_id"), None | Some(Value::Null))
            && rl_obj.get("limit_id").and_then(Value::as_str) != Some("codex")
        {
            continue;
        }
        if rl_obj.get("primary").is_some_and(Value::is_object)
            || rl_obj.get("secondary").is_some_and(Value::is_object)
        {
            return Some((rl.clone(), when(row.get("timestamp"), path)));
        }
    }
    None
}

/// `codex_usage._window(w, written, now)`: `(percent, minutes to reset or None, window minutes)`.
fn window(w: Option<&Value>, written: f64, now: f64) -> Option<(i64, Option<i64>, i64)> {
    let w = w?.as_object()?;
    let pct = py_float(w.get("used_percent"))?;
    let span = py_int(w.get("window_minutes"))?;
    let mut reset = w.get("resets_at").and_then(Value::as_f64);
    if reset.is_none() {
        if let Some(secs) = w.get("resets_in_seconds").and_then(Value::as_f64) {
            reset = Some(written + secs); // older Codex versions
        }
    }
    let mut left = None;
    if let Some(reset) = reset {
        if reset <= now {
            return Some((0, None, span)); // that window has started over
        }
        left = Some(round_i64((reset - now) / 60.0)?);
    }
    Some((round_i64(pct)?.clamp(0, 100), left, span))
}

/// `codex_usage.payload_from(rl, written, now)`: map Codex `rate_limits` to the clock's usage
/// fields (`s sr w wr st acct ok src`); `None` when no window is usable.
pub fn payload_from(rate_limits: &Value, written: f64, now: Option<f64>) -> Option<UsagePayload> {
    let now = now.unwrap_or_else(|| SystemClock.now());
    let wins: Vec<_> = [
        window(rate_limits.get("primary"), written, now),
        window(rate_limits.get("secondary"), written, now),
    ]
    .into_iter()
    .flatten()
    .collect();
    if wins.is_empty() {
        return None;
    }
    let short = wins.iter().find(|w| w.2 != 0 && w.2 < DAY);
    let long = wins.iter().find(|w| w.2 == 0 || w.2 >= DAY);
    let mut s = 0;
    let mut w = 0;
    let mut out = UsagePayload::new();
    out.insert("s".into(), json!(0));
    out.insert("w".into(), json!(0));
    out.insert("acct".into(), json!("pro"));
    out.insert("ok".into(), json!(true));
    out.insert("src".into(), json!("codex"));
    if let Some(win) = short {
        s = win.0;
        out.insert("s".into(), json!(s));
        if let Some(left) = win.1 {
            out.insert("sr".into(), json!(left));
        }
    }
    if let Some(win) = long {
        w = win.0;
        out.insert("w".into(), json!(w));
        if let Some(left) = win.1 {
            out.insert("wr".into(), json!(left));
        }
    }
    let reached = rate_limits
        .get("rate_limit_reached_type")
        .is_some_and(crate::util::truthy);
    let st = if reached {
        "rejected"
    } else if s.max(w) >= 90 {
        "allowed_warning"
    } else {
        "allowed"
    };
    out.insert("st".into(), json!(st));
    Some(out)
}

/// `codex_usage.read(home, env, now)`: the payload from the newest session log with limits, or
/// `None`.
pub fn read(paths: &AppPaths, env: &Env, now: Option<f64>) -> Option<UsagePayload> {
    for file in newest_files(&sessions_dir(paths, env), FILES) {
        let Ok(meta) = fs::metadata(&file) else {
            continue;
        };
        let mtime_ns = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        let key = (file.clone(), mtime_ns, meta.len());
        let cached = cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned();
        let hit = match cached {
            Some(hit) => hit,
            None => {
                let parsed = limits_in(&file);
                let mut guard = cache().lock().unwrap_or_else(|e| e.into_inner());
                if guard.len() > 64 {
                    guard.clear();
                }
                guard.insert(key, parsed.clone());
                parsed
            }
        };
        if let Some((rl, at)) = hit {
            return payload_from(&rl, at, now);
        }
    }
    None
}

/// `codex_usage.available(home, env)`: Codex has left session logs on this machine (the setup
/// wizard offers the source).
pub fn available(paths: &AppPaths, env: &Env) -> bool {
    !newest_files(&sessions_dir(paths, env), 1).is_empty()
}
