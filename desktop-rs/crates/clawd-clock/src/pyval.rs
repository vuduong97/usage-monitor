//! Python value semantics the clock code leans on: `clock.json` and the clock's answers are
//! untyped JSON, and the Python module compares, truth-tests, converts and prints them with
//! Python's rules (`1 == 1.0`, `str(5)`, `int("80")`, `{...!r}` in error messages).

use serde_json::{Number, Value};

/// Python truthiness of a JSON value (`if x:`).
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

/// `str(x)` for the shapes `clock.json` holds. `None` is not reached (callers write
/// `str(x or "")` or `x or ""` first); containers are printed like Python's `repr`.
pub(crate) fn to_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => repr(other),
    }
}

/// `x or ""` then `str(...)`: the empty string for every falsy value.
pub(crate) fn str_or_empty(v: Option<&Value>) -> String {
    match v {
        Some(v) if truthy(v) => to_str(v),
        _ => String::new(),
    }
}

fn number_f64(n: &Number) -> f64 {
    n.as_f64().unwrap_or(f64::NAN)
}

fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => Some(number_f64(n)),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

/// Python `==` on two JSON values: numbers (and bools) compare by value, objects ignore key
/// order.
pub(crate) fn eq(a: &Value, b: &Value) -> bool {
    if let (Some(x), Some(y)) = (as_number(a), as_number(b)) {
        return x == y;
    }
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|other| eq(v, other)))
        }
        _ => a == b,
    }
}

/// `int(x)` as a TCP port: an int, a float, a bool or a numeric string; `None` when Python's
/// `int()` raises or the value does not fit a port (`u16`).
pub(crate) fn to_port(v: &Value) -> Option<u16> {
    let n: i64 = match v {
        Value::Bool(b) => i64::from(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i,
            None => {
                let f = n.as_f64()?;
                if !f.is_finite() {
                    return None;
                }
                f.trunc() as i64
            }
        },
        Value::String(s) => s.trim().replace('_', "").parse::<i64>().ok()?,
        _ => return None,
    };
    u16::try_from(n).ok()
}

/// Python `repr()` of a JSON value: `{'ok': False, 'n': None}`.
pub(crate) fn repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => repr_str(s),
        Value::Array(a) => {
            let items: Vec<String> = a.iter().map(repr).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(o) => {
            let items: Vec<String> = o
                .iter()
                .map(|(k, v)| format!("{}: {}", repr_str(k), repr(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

/// Python `repr(str)`: single quotes unless the text has a `'` and no `"`.
pub(crate) fn repr_str(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn python_equality_and_truth() {
        assert!(eq(&json!(1), &json!(1.0)));
        assert!(eq(&json!({"a": [1, 2]}), &json!({"a": [1.0, 2]})));
        assert!(!eq(&json!("1"), &json!(1)));
        assert!(!truthy(&json!(0)) && !truthy(&json!("")) && !truthy(&Value::Null));
        assert!(truthy(&json!({"a": 1})));
    }

    #[test]
    fn ports_and_repr() {
        assert_eq!(to_port(&json!("80")), Some(80));
        assert_eq!(to_port(&json!(8080.0)), Some(8080));
        assert_eq!(to_port(&json!("x")), None);
        assert_eq!(to_port(&json!(70000)), None);
        assert_eq!(repr(&json!({"ok": false, "n": null})), "{'ok': False, 'n': None}");
        assert_eq!(repr_str("it's"), "\"it's\"");
        assert_eq!(repr_str("a\nb"), "'a\\nb'");
    }
}
