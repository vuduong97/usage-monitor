//! What the settings page may save and the answer shapes: `SCHEMA`, `clean`, `_clock_view`,
//! `_fail`, `_err` of `settings_api.py:41-130`. The page is untrusted input: a save is filtered
//! through this schema, anything it does not know is dropped.

use clawd_contracts::ClockError;
use serde_json::{json, Map, Value};

/// One checked value (`_int`, `_bool`, `_str`, `_wifi`) or a group of them.
enum Rule {
    Int(i64, i64),
    Bool,
    Text {
        max: usize,
        min: usize,
        pattern: Option<fn(&str) -> bool>,
    },
    Wifi,
    Group(&'static [(&'static str, Rule)]),
}

/// `HOSTNAME = ^[a-z0-9](?:[a-z0-9-]{0,30}[a-z0-9])?$`.
fn hostname(s: &str) -> bool {
    let b = s.as_bytes();
    let edge = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty()
        && b.len() <= 32
        && edge(b[0])
        && edge(b[b.len() - 1])
        && b.iter().all(|&c| edge(c) || c == b'-')
}

/// `HHMM = ^(?:[01]\d|2[0-3]):[0-5]\d$`.
fn hhmm(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 5
        && b[2] == b':'
        && matches!(
            (b[0], b[1]),
            (b'0' | b'1', b'0'..=b'9') | (b'2', b'0'..=b'3')
        )
        && matches!(b[3], b'0'..=b'5')
        && b[4].is_ascii_digit()
}

/// The settings the window edits (`SCHEMA`), in Python's key order.
const SCHEMA: &[(&str, Rule)] = &[
    (
        "hostname",
        Rule::Text {
            max: 32,
            min: 1,
            pattern: Some(hostname),
        },
    ),
    ("brightness", Rule::Int(0, 100)),
    ("rotation", Rule::Int(0, 3)),
    ("wifi", Rule::Wifi),
    (
        "usage",
        Rule::Group(&[
            ("screen", Rule::Int(0, 2)),
            ("anim", Rule::Int(-1, 63)),
            ("layout", Rule::Int(0, 15)),
        ]),
    ),
    (
        "clock",
        Rule::Group(&[
            (
                "tz",
                Rule::Text {
                    max: 48,
                    min: 0,
                    pattern: None,
                },
            ),
            (
                "tzPosix",
                Rule::Text {
                    max: 64,
                    min: 0,
                    pattern: None,
                },
            ),
            ("nightEnabled", Rule::Bool),
            (
                "nightStart",
                Rule::Text {
                    max: 5,
                    min: 0,
                    pattern: Some(hhmm),
                },
            ),
            (
                "nightEnd",
                Rule::Text {
                    max: 5,
                    min: 0,
                    pattern: Some(hhmm),
                },
            ),
            ("nightLevel", Rule::Int(0, 100)),
        ]),
    ),
];

fn check_text(
    v: &Value,
    max: usize,
    min: usize,
    pattern: Option<fn(&str) -> bool>,
) -> Result<Value, String> {
    match v.as_str() {
        Some(s) => {
            let len = s.chars().count();
            if len < min || len > max || pattern.is_some_and(|p| !p(s)) {
                Err("bad text".into())
            } else {
                Ok(v.clone())
            }
        }
        None => Err("bad text".into()),
    }
}

fn check_wifi(v: &Value) -> Result<Value, String> {
    let rows = match v.as_array() {
        Some(rows) if (1..=4).contains(&rows.len()) => rows,
        _ => return Err("1 to 4 networks".into()),
    };
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for row in rows {
        let row = row.as_object().ok_or("bad network row")?;
        let ssid = check_text(row.get("ssid").unwrap_or(&Value::Null), 32, 1, None)?;
        let name = ssid.as_str().unwrap_or_default().to_owned();
        if seen.contains(&name.as_str()) {
            return Err(format!("network {name} twice"));
        }
        // `seen` borrows from `rows`, so keep the owned name's text from the input row.
        seen.push(row.get("ssid").and_then(Value::as_str).unwrap_or_default());
        let mut entry = Map::new();
        entry.insert("ssid".into(), ssid);
        // A blank password keeps the stored one (firmware rule).
        if let Some(pw) = row.get("pass").filter(|p| truthy(p)) {
            entry.insert("pass".into(), check_text(pw, 64, 8, None)?);
        }
        out.push(Value::Object(entry));
    }
    Ok(Value::Array(out))
}

fn check(rule: &Rule, v: &Value) -> Result<Value, String> {
    match rule {
        Rule::Int(lo, hi) => match v.as_i64() {
            Some(n) if (*lo..=*hi).contains(&n) => Ok(v.clone()),
            _ => Err(format!("needs a whole number {lo}..{hi}")),
        },
        Rule::Bool => {
            if v.is_boolean() {
                Ok(v.clone())
            } else {
                Err("needs true/false".into())
            }
        }
        Rule::Text { max, min, pattern } => check_text(v, *max, *min, *pattern),
        Rule::Wifi => check_wifi(v),
        Rule::Group(_) => unreachable!("groups are walked by clean_group"),
    }
}

fn clean_group(
    data: &Value,
    schema: &[(&str, Rule)],
    strict: bool,
) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    let Some(obj) = data.as_object() else {
        return Ok(out);
    };
    for (key, rule) in schema {
        let Some(value) = obj.get(*key) else { continue };
        let checked = match rule {
            Rule::Group(inner) => clean_group(value, inner, strict).map(Value::Object),
            _ => check(rule, value),
        };
        match checked {
            Ok(v) => {
                out.insert((*key).into(), v);
            }
            Err(e) if strict => return Err(format!("{key}: {e}")),
            Err(_) => {}
        }
    }
    Ok(out)
}

/// `clean(data)`: the part of `data` the schema knows, checked. `Err` carries Python's
/// `ValueError` text (`"hostname: bad text"`).
pub fn clean(data: &Value) -> Result<Value, String> {
    clean_group(data, SCHEMA, true).map(Value::Object)
}

/// `clean(data, strict=False)`: values that fail are skipped instead of raising.
pub fn clean_lenient(data: &Value) -> Value {
    Value::Object(clean_group(data, SCHEMA, false).unwrap_or_default())
}

/// Python truthiness of a JSON value.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `_clock_view(cfg)`: the clock's config in the shape the window saves (wifi rows lose
/// `passSet`).
pub fn clock_view(cfg: &Value) -> Value {
    let mut view = cfg.clone();
    if let (Some(obj), Some(rows)) = (
        view.as_object_mut(),
        cfg.get("wifi").and_then(Value::as_array),
    ) {
        let kept: Vec<Value> = rows
            .iter()
            .filter_map(Value::as_object)
            .filter(|r| r.get("ssid").is_some_and(truthy))
            .map(|r| json!({"ssid": r["ssid"]}))
            .collect();
        obj.insert("wifi".into(), Value::Array(kept));
    }
    clean_lenient(&view)
}

/// `{"ok": false, "code", "error"}`.
pub fn fail(code: &str, msg: &str) -> Value {
    json!({"ok": false, "code": code, "error": msg})
}

/// `_err(e)`: a clock error as the page's failure dict.
pub fn err(e: &ClockError) -> Value {
    if matches!(e, ClockError::AuthNeeded) {
        return fail("auth", "Đồng hồ cần mật khẩu trang cài đặt.");
    }
    if e.to_string().starts_with("no answer") {
        return fail(
            "offline",
            "Đồng hồ không trả lời. Đồng hồ còn bật và cùng mạng Wi\u{2011}Fi không?",
        );
    }
    fail("error", &format!("Lỗi: {e}"))
}

/// `{**base, **extra}` for two JSON objects (`extra` wins).
pub fn merged(mut base: Value, extra: Value) -> Value {
    if let (Some(into), Value::Object(from)) = (base.as_object_mut(), extra) {
        for (k, v) in from {
            into.insert(k, v);
        }
    }
    base
}

/// `Giá trị không hợp lệ (...)` for a schema refusal.
pub fn bad_value(e: &str) -> Value {
    fail("bad", &format!("Giá trị không hợp lệ ({e})."))
}

/// Standard base64 (RFC 4648, padded): the `/clawd.pak` bytes go to the page as text.
pub fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_drops_unknown_keys_and_checks_values() {
        // test_settings_api.py::test_clean_drops_unknown_keys_and_checks_values
        let got = clean(&json!({
            "hostname": "clawd", "brightness": 50, "evil": 1,
            "usage": {"screen": 1, "bogus": 2},
        }))
        .unwrap();
        assert_eq!(
            got,
            json!({"hostname": "clawd", "brightness": 50, "usage": {"screen": 1}})
        );
        assert_eq!(
            clean(&json!({"brightness": 101})).unwrap_err(),
            "brightness: needs a whole number 0..100"
        );
        assert_eq!(
            clean(&json!({"brightness": true})).unwrap_err(),
            "brightness: needs a whole number 0..100"
        );
        assert_eq!(
            clean(&json!({"brightness": 5.0})).unwrap_err(),
            "brightness: needs a whole number 0..100"
        );
        assert_eq!(
            clean(&json!({"hostname": "Bad Name"})).unwrap_err(),
            "hostname: bad text"
        );
        assert_eq!(
            clean(&json!({"usage": {"anim": 64}})).unwrap_err(),
            "usage: anim: needs a whole number -1..63"
        );
        assert_eq!(
            clean(&json!({"clock": {"nightStart": "24:00"}})).unwrap_err(),
            "clock: nightStart: bad text"
        );
        assert_eq!(
            clean(&json!({"clock": {"nightEnabled": 1}})).unwrap_err(),
            "clock: nightEnabled: needs true/false"
        );
        assert_eq!(clean(&json!("not a dict")).unwrap(), json!({}));
    }

    #[test]
    fn hostnames_and_times_follow_the_python_patterns() {
        assert!(hostname("a") && hostname("clawd-2") && hostname(&"a".repeat(32)));
        assert!(
            !hostname("")
                && !hostname("-a")
                && !hostname("a-")
                && !hostname("A")
                && !hostname(&"a".repeat(33))
        );
        assert!(hhmm("00:00") && hhmm("19:59") && hhmm("23:59"));
        assert!(!hhmm("24:00") && !hhmm("7:00") && !hhmm("12:60") && !hhmm("12-30"));
    }

    #[test]
    fn wifi_rows_are_checked() {
        let ok = clean(&json!({"wifi": [{"ssid": "Home", "pass": "", "passSet": true}, {"ssid": "Phone", "pass": "password1"}]}))
            .unwrap();
        assert_eq!(
            ok,
            json!({"wifi": [{"ssid": "Home"}, {"ssid": "Phone", "pass": "password1"}]})
        );
        assert_eq!(
            clean(&json!({"wifi": []})).unwrap_err(),
            "wifi: 1 to 4 networks"
        );
        assert_eq!(
            clean(&json!({"wifi": [{"ssid": "A"}, {"ssid": "A"}]})).unwrap_err(),
            "wifi: network A twice"
        );
        assert_eq!(
            clean(&json!({"wifi": [{"ssid": "A", "pass": "short"}]})).unwrap_err(),
            "wifi: bad text"
        );
        assert_eq!(
            clean(&json!({"wifi": ["x"]})).unwrap_err(),
            "wifi: bad network row"
        );
    }

    #[test]
    fn the_clock_view_loses_pass_set_and_skips_bad_values() {
        let cfg = json!({
            "hostname": "clawd", "brightness": 999, "uptime": 5,
            "wifi": [{"ssid": "Home", "passSet": true}, {"ssid": ""}, "junk"],
        });
        assert_eq!(
            clock_view(&cfg),
            json!({"hostname": "clawd", "wifi": [{"ssid": "Home"}]})
        );
    }

    #[test]
    fn clock_errors_become_the_pages_failures() {
        assert_eq!(err(&ClockError::AuthNeeded)["code"], "auth");
        let off = err(&ClockError::NoAnswer {
            base: "http://x".into(),
            kind: "ConnectError".into(),
        });
        assert_eq!(off["code"], "offline");
        let other = err(&ClockError::Http {
            status: 500,
            body: "boom".into(),
        });
        assert_eq!(
            other,
            json!({"ok": false, "code": "error", "error": "Lỗi: HTTP 500: boom"})
        );
    }

    #[test]
    fn merged_lets_the_second_object_win() {
        assert_eq!(
            merged(json!({"a": 1, "b": 2}), json!({"b": 3, "c": 4})),
            json!({"a": 1, "b": 3, "c": 4})
        );
    }

    #[test]
    fn base64_matches_the_rfc_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn python_truthiness() {
        assert!(
            !truthy(&json!(0))
                && !truthy(&json!(""))
                && !truthy(&Value::Null)
                && !truthy(&json!([]))
        );
        assert!(truthy(&json!("x")) && truthy(&json!(1)) && truthy(&json!({"a": 1})));
    }
}
