//! Python value semantics the ported checks depend on (`int(x)`, `str(x)`, truthiness, `repr`,
//! `json.dumps`). The Python modules validate untrusted JSON with these builtins, so their error
//! texts and accept/reject decisions are part of the behaviour to keep.

use serde_json::Value;

/// Python truthiness of a JSON value.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i != 0
            } else if let Some(u) = n.as_u64() {
                u != 0
            } else {
                n.as_f64().is_some_and(|f| f != 0.0)
            }
        }
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `str(v)` for a JSON value.
pub fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => repr(other),
    }
}

/// `str(v or "")`.
pub fn str_or_empty(v: Option<&Value>) -> String {
    match v {
        Some(v) if truthy(v) => str_of(v),
        _ => String::new(),
    }
}

/// `repr(v)` for a JSON value (what `f"... {v!r}"` prints).
pub fn repr(v: &Value) -> String {
    match v {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                n.to_string()
            } else {
                format!("{:?}", n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => str_repr(s),
        Value::Array(a) => {
            let items: Vec<String> = a.iter().map(repr).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(o) => {
            let items: Vec<String> = o
                .iter()
                .map(|(k, v)| format!("{}: {}", str_repr(k), repr(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

/// `repr(str)`: single quotes unless the text holds `'` and no `"`.
pub fn str_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `int(v)` for a JSON value: ints, bools, finite floats (truncated) and numeric strings.
pub fn int_of(v: &Value) -> Option<i64> {
    match v {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i)
            } else if n.is_u64() {
                None
            } else {
                let f = n.as_f64()?;
                (f.is_finite() && f.abs() < 9.2e18).then(|| f.trunc() as i64)
            }
        }
        Value::String(s) => int_from_str(s, 10),
        _ => None,
    }
}

/// `int(text, radix)`: surrounding whitespace, a sign, a `0x`/`0o`/`0b` prefix matching the radix
/// and single underscores between digits are accepted.
pub fn int_from_str(text: &str, radix: u32) -> Option<i64> {
    let t = text.trim();
    let (neg, body) = match t.as_bytes().first()? {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    let lower = body.to_ascii_lowercase();
    let digits = match (radix, lower.get(..2)) {
        (16, Some("0x")) | (8, Some("0o")) | (2, Some("0b")) => &lower[2..],
        _ => lower.as_str(),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
    {
        return None;
    }
    let clean: String = digits.chars().filter(|c| *c != '_').collect();
    let n = i64::from_str_radix(&clean, radix).ok()?;
    Some(if neg { -n } else { n })
}

/// `json.dumps(v)` (`", "` and `": "` separators, `ensure_ascii`), optionally with `sort_keys`.
pub fn json_dumps(v: &Value, sort_keys: bool) -> String {
    let mut out = String::new();
    dump(v, sort_keys, &mut out);
    out
}

fn dump(v: &Value, sort_keys: bool, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                out.push_str(&n.to_string());
            } else {
                out.push_str(&format!("{:?}", n.as_f64().unwrap_or(0.0)));
            }
        }
        Value::String(s) => dump_str(s, out),
        Value::Array(a) => {
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                dump(item, sort_keys, out);
            }
            out.push(']');
        }
        Value::Object(o) => {
            out.push('{');
            let mut entries: Vec<(&String, &Value)> = o.iter().collect();
            if sort_keys {
                entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            }
            for (i, (k, item)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                dump_str(k, out);
                out.push_str(": ");
                dump(item, sort_keys, out);
            }
            out.push('}');
        }
    }
}

fn dump_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (' '..='~').contains(&c) => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn truthiness_and_str() {
        assert!(
            !truthy(&json!(0))
                && !truthy(&json!(""))
                && !truthy(&json!([]))
                && !truthy(&Value::Null)
        );
        assert!(truthy(&json!("a")) && truthy(&json!(0.5)) && truthy(&json!({"a": 1})));
        assert_eq!(str_or_empty(Some(&json!(null))), "");
        assert_eq!(str_or_empty(Some(&json!(5))), "5");
        assert_eq!(str_or_empty(Some(&json!(true))), "True");
    }

    #[test]
    fn repr_matches_python() {
        assert_eq!(repr(&json!("6")), "'6'");
        assert_eq!(repr(&json!("it's")), "\"it's\"");
        assert_eq!(repr(&json!(null)), "None");
        assert_eq!(repr(&json!([1, "a"])), "[1, 'a']");
        assert_eq!(repr(&json!(1.5)), "1.5");
        assert_eq!(str_repr("a\nb\\"), "'a\\nb\\\\'");
    }

    #[test]
    fn int_semantics() {
        assert_eq!(int_of(&json!("12")), Some(12));
        assert_eq!(int_of(&json!(" 7 ")), Some(7));
        assert_eq!(int_of(&json!(2.9)), Some(2));
        assert_eq!(int_of(&json!(true)), Some(1));
        assert_eq!(int_of(&json!(null)), None);
        assert_eq!(int_of(&json!("x")), None);
        assert_eq!(int_from_str("0x1F", 16), Some(31));
        assert_eq!(int_from_str("deadbeef", 16), Some(0xdead_beef));
        assert_eq!(int_from_str("1_0", 10), Some(10));
        assert_eq!(int_from_str("_1", 10), None);
    }

    #[test]
    fn dumps_matches_python() {
        let v = json!({"b": 1, "a": [1, "é"], "c": null});
        assert_eq!(
            json_dumps(&v, false),
            r#"{"b": 1, "a": [1, "\u00e9"], "c": null}"#
        );
        assert_eq!(
            json_dumps(&v, true),
            r#"{"a": [1, "\u00e9"], "b": 1, "c": null}"#
        );
        assert_eq!(json_dumps(&json!("😀"), false), "\"\\ud83d\\ude00\"");
    }
}
