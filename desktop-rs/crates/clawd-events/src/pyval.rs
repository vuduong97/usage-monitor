//! Python's truthiness and `str()` for JSON values. The hook/poke payloads are untyped dicts in
//! Python and `events.py` leans on `x or ""`, `str(x)` and `bool(x)` on them; the Rust port keeps
//! those semantics exactly so odd payloads (`{"text": 5}`) behave the same.

use serde_json::Value;

/// `bool(value)` for a JSON value (`None` = key absent).
pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// `str(value or "")`: empty for a falsy value, else Python's `str()`.
pub(crate) fn text_or_empty(value: Option<&Value>) -> String {
    if truthy(value) {
        value.map(py_str).unwrap_or_default()
    } else {
        String::new()
    }
}

/// Python's `str(value)`.
pub(crate) fn py_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => py_repr(other),
    }
}

/// Python's `repr(value)` (what `str()` gives for everything but a string).
pub(crate) fn py_repr(value: &Value) -> String {
    match value {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                float_repr(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => str_repr(s),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(py_repr).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", str_repr(k), py_repr(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

fn float_repr(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

/// `repr(str)`: single quotes unless the text holds a `'` and no `"`.
fn str_repr(s: &str) -> String {
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

/// `float(value)` for the poke fields: numbers, bools (`float(True)`) and numeric strings.
pub(crate) fn to_float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Python's `str.split()` whitespace (ASCII range: the clock text is ASCII by then).
pub(crate) fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn truthiness_follows_python() {
        assert!(!truthy(None));
        assert!(!truthy(Some(&json!(0))));
        assert!(!truthy(Some(&json!(0.0))));
        assert!(!truthy(Some(&json!(""))));
        assert!(!truthy(Some(&json!([]))));
        assert!(truthy(Some(&json!("0"))));
        assert!(truthy(Some(&json!(-1))));
    }

    #[test]
    fn str_matches_python_repr() {
        assert_eq!(py_str(&json!(5)), "5");
        assert_eq!(py_str(&json!(2.5)), "2.5");
        assert_eq!(py_str(&json!(1.0)), "1.0");
        assert_eq!(py_str(&json!(true)), "True");
        assert_eq!(py_str(&json!(null)), "None");
        assert_eq!(py_str(&json!([1, "a"])), "[1, 'a']");
        assert_eq!(py_str(&json!({"k": null})), "{'k': None}");
        assert_eq!(py_repr(&json!("it's")), "\"it's\"");
        assert_eq!(text_or_empty(Some(&json!(0))), "");
    }
}
