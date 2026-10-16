//! Small Python-semantics helpers the ports need (banker's rounding, truthiness, `float()` /
//! `int()` of JSON values, `datetime.fromisoformat`, `Path.expanduser`).

use std::path::PathBuf;

use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use clawd_contracts::AppPaths;
use serde_json::Value;

/// `int(round(x))`: Python rounds half to even. `None` for NaN/inf (Python raises there).
pub(crate) fn round_i64(x: f64) -> Option<i64> {
    x.is_finite().then(|| x.round_ties_even() as i64)
}

/// Python truthiness of a JSON value.
pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `float(v or 0)`; `None` where Python raises (`ValueError` / `TypeError`).
pub(crate) fn py_float(v: Option<&Value>) -> Option<f64> {
    let v = match v {
        Some(v) if truthy(v) => v,
        _ => return Some(0.0),
    };
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(_) => Some(1.0),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// `int(v or 0)`; `None` where Python raises.
pub(crate) fn py_int(v: Option<&Value>) -> Option<i64> {
    let v = match v {
        Some(v) if truthy(v) => v,
        _ => return Some(0),
    };
    match v {
        Value::Number(n) => match n.as_i64() {
            Some(i) => Some(i),
            None => n
                .as_f64()
                .filter(|f| f.is_finite())
                .map(|f| f.trunc() as i64),
        },
        Value::Bool(_) => Some(1),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// `str(v)` for the JSON scalars the attention file carries (`str(data.get("m") or "")`).
pub(crate) fn py_str_or_empty(v: Option<&Value>) -> String {
    match v {
        Some(v) if truthy(v) => match v {
            Value::String(s) => s.clone(),
            Value::Bool(_) => "True".to_owned(),
            other => other.to_string(),
        },
        _ => String::new(),
    }
}

/// Seconds-east offset of the local zone at `epoch` (`time.localtime().tm_gmtoff`).
pub(crate) fn local_offset_secs(epoch: f64) -> i64 {
    Local
        .timestamp_opt(epoch.floor() as i64, 0)
        .single()
        .map_or(0, |dt| i64::from(dt.offset().local_minus_utc()))
}

/// `datetime.fromisoformat(ts.replace("Z", "+00:00")).timestamp()`: an offset is honoured, a
/// naive time is local time like Python.
pub(crate) fn parse_iso_timestamp(raw: &str) -> Option<f64> {
    let s = raw.replace('Z', "+00:00");
    let s = s.trim();
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%d %H:%M:%S%.f%:z",
        "%Y-%m-%dT%H:%M:%S%.f%z",
        "%Y-%m-%d %H:%M:%S%.f%z",
    ] {
        if let Ok(dt) = chrono::DateTime::parse_from_str(s, fmt) {
            return Some(dt.timestamp() as f64 + f64::from(dt.timestamp_subsec_nanos()) / 1e9);
        }
    }
    let naive = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
    ]
    .iter()
    .find_map(|fmt| NaiveDateTime::parse_from_str(s, fmt).ok())
    .or_else(|| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
    })?;
    let dt = Local.from_local_datetime(&naive).earliest()?;
    Some(dt.timestamp() as f64 + f64::from(dt.timestamp_subsec_nanos()) / 1e9)
}

/// `Path(raw).expanduser()` against the injected home.
pub(crate) fn expand_user(paths: &AppPaths, raw: &str) -> PathBuf {
    if raw == "~" {
        return paths.home().to_path_buf();
    }
    match raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        Some(rest) => paths.home().join(rest),
        None => PathBuf::from(raw),
    }
}

/// Run blocking work (file reads, `security`) off the async worker; `None` if it panicked.
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

/// Python's `str.split()` whitespace (adds `\x1c`-`\x1f` to ASCII whitespace).
pub(crate) fn is_py_space(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1f'
    )
}
