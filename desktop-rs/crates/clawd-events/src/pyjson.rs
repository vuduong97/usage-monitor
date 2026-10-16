//! `json.dumps(value)` byte for byte: `", "` / `": "` separators and `ensure_ascii` escapes.
//! The event server answers with it so a client (or a diff against the Python daemon) sees the
//! same bytes.

use std::io;

use serde::Serialize;
use serde_json::ser::{Formatter, Serializer};

struct PyFormatter;

impl Formatter for PyFormatter {
    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            w.write_all(b", ")
        }
    }

    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            w.write_all(b", ")
        }
    }

    fn begin_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        w.write_all(b": ")
    }

    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        let mut start = 0;
        for (i, c) in fragment.char_indices() {
            if c.is_ascii() {
                continue;
            }
            w.write_all(&fragment.as_bytes()[start..i])?;
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                write!(w, "\\u{unit:04x}")?;
            }
            start = i + c.len_utf8();
        }
        w.write_all(&fragment.as_bytes()[start..])
    }
}

/// Python's `json.dumps(value)`.
pub(crate) fn to_py_json<T: Serialize>(value: &T) -> String {
    let mut out = Vec::new();
    let mut serializer = Serializer::with_formatter(&mut out, PyFormatter);
    if value.serialize(&mut serializer).is_err() {
        return "null".to_owned();
    }
    String::from_utf8(out).unwrap_or_else(|_| "null".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_python_dumps() {
        let v = json!({"ok": true, "list": [1, 2, {"a": null}], "t": "Đồng \"hồ\"\n", "e": [], "o": {}});
        assert_eq!(
            to_py_json(&v),
            r#"{"ok": true, "list": [1, 2, {"a": null}], "t": "\u0110\u1ed3ng \"h\u1ed3\"\n", "e": [], "o": {}}"#
        );
        assert_eq!(to_py_json(&json!("😀")), r#""\ud83d\ude00""#);
    }
}
