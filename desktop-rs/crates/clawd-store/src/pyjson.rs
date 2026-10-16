//! `json.dumps` as Python writes it, so the files this crate writes are byte-identical to the
//! ones the Python app wrote (`desktop.json`, `pause.json`, `usage_payload.json`):
//!
//! * `ensure_ascii=True`: every char outside `' '..='~'` becomes `\uXXXX` (lowercase hex,
//!   surrogate pairs above U+FFFF), so `0x7f` is escaped too;
//! * floats use Python's `repr` (`1e-05`, `1e+16`, `1.0`), which differs from `serde_json`'s;
//! * separators: [`Style::Indent2`] is `json.dumps(x, indent=2)`, [`Style::Default`] is
//!   `json.dumps(x)` (`", "` and `": "`), [`Style::Compact`] is `separators=(",", ":")`.

use std::io::{self, Write};

use serde::Serialize;
use serde_json::ser::Formatter;

/// Which `json.dumps` call to imitate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// `json.dumps(x, indent=2)`
    Indent2,
    /// `json.dumps(x)`
    Default,
    /// `json.dumps(x, separators=(",", ":"))`
    Compact,
}

/// `json.dumps(value, ...)` in the given [`Style`].
pub fn dumps<T: Serialize + ?Sized>(value: &T, style: Style) -> io::Result<String> {
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, PyFormatter::new(style));
    value.serialize(&mut ser).map_err(io::Error::from)?;
    String::from_utf8(out).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Python's `repr(float)` for a finite value.
pub fn py_float_repr(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    // `{:e}` gives the shortest round-trip digits: `-1.2345e-7`, `5e0`.
    let sci = format!("{value:e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let n = digits.len() as i32;
    let decpt = exp + 1;
    let body = if decpt <= -4 || decpt > 16 {
        let mut s = digits[..1].to_owned();
        if n > 1 {
            s.push('.');
            s.push_str(&digits[1..]);
        }
        let e = decpt - 1;
        format!("{s}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    } else if decpt <= 0 {
        format!("0.{}{digits}", "0".repeat((-decpt) as usize))
    } else if decpt >= n {
        format!("{digits}{}.0", "0".repeat((decpt - n) as usize))
    } else {
        format!(
            "{}.{}",
            &digits[..decpt as usize],
            &digits[decpt as usize..]
        )
    };
    format!("{sign}{body}")
}

struct PyFormatter {
    style: Style,
    depth: usize,
    has_value: bool,
}

impl PyFormatter {
    fn new(style: Style) -> Self {
        Self {
            style,
            depth: 0,
            has_value: false,
        }
    }

    fn separator<W: ?Sized + Write>(&self, w: &mut W, first: bool) -> io::Result<()> {
        match self.style {
            Style::Indent2 => {
                w.write_all(if first { b"\n" } else { b",\n" })?;
                for _ in 0..self.depth {
                    w.write_all(b"  ")?;
                }
                Ok(())
            }
            Style::Default if !first => w.write_all(b", "),
            Style::Compact if !first => w.write_all(b","),
            _ => Ok(()),
        }
    }

    fn close<W: ?Sized + Write>(&mut self, w: &mut W, bracket: &[u8]) -> io::Result<()> {
        self.depth -= 1;
        if self.style == Style::Indent2 && self.has_value {
            w.write_all(b"\n")?;
            for _ in 0..self.depth {
                w.write_all(b"  ")?;
            }
        }
        self.has_value = true;
        w.write_all(bracket)
    }
}

impl Formatter for PyFormatter {
    fn write_f64<W: ?Sized + Write>(&mut self, w: &mut W, value: f64) -> io::Result<()> {
        w.write_all(py_float_repr(value).as_bytes())
    }

    fn write_string_fragment<W: ?Sized + Write>(
        &mut self,
        w: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        let mut run_start = 0;
        for (i, c) in fragment.char_indices() {
            if (' '..='~').contains(&c) {
                continue;
            }
            w.write_all(&fragment.as_bytes()[run_start..i])?;
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                write!(w, "\\u{unit:04x}")?;
            }
            run_start = i + c.len_utf8();
        }
        w.write_all(&fragment.as_bytes()[run_start..])
    }

    fn begin_array<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.depth += 1;
        self.has_value = false;
        w.write_all(b"[")
    }

    fn end_array<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.close(w, b"]")
    }

    fn begin_array_value<W: ?Sized + Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        self.separator(w, first)
    }

    fn end_array_value<W: ?Sized + Write>(&mut self, _w: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }

    fn begin_object<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.depth += 1;
        self.has_value = false;
        w.write_all(b"{")
    }

    fn end_object<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.close(w, b"}")
    }

    fn begin_object_key<W: ?Sized + Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        self.separator(w, first)
    }

    fn begin_object_value<W: ?Sized + Write>(&mut self, w: &mut W) -> io::Result<()> {
        w.write_all(if self.style == Style::Compact {
            b":"
        } else {
            b": "
        })
    }

    fn end_object_value<W: ?Sized + Write>(&mut self, _w: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn float_repr_matches_python() {
        let cases = [
            (1.0, "1.0"),
            (2.5, "2.5"),
            (-0.5, "-0.5"),
            (0.0, "0.0"),
            (1e-5, "1e-05"),
            (0.0001, "0.0001"),
            (1.5e-7, "1.5e-07"),
            (1e16, "1e+16"),
            (1234567890123456.0, "1234567890123456.0"),
            (1.5e22, "1.5e+22"),
            (123456.789, "123456.789"),
            (1760000000.123456, "1760000000.123456"),
            (0.1 + 0.2, "0.30000000000000004"),
        ];
        for (v, want) in cases {
            assert_eq!(py_float_repr(v), want, "{v:?}");
        }
    }

    #[test]
    fn indent2_matches_python_layout_and_escapes() {
        let v = json!({"a": [], "b": {}, "n": "Đ\u{7f}\u{1F600}\n\"q\"", "f": 1.0, "l": [1, {"x": null}]});
        let want = "{\n  \"a\": [],\n  \"b\": {},\n  \"n\": \"\\u0110\\u007f\\ud83d\\ude00\\n\\\"q\\\"\",\n  \"f\": 1.0,\n  \"l\": [\n    1,\n    {\n      \"x\": null\n    }\n  ]\n}";
        assert_eq!(dumps(&v, Style::Indent2).unwrap(), want);
    }

    #[test]
    fn default_and_compact_separators() {
        let v: Value = json!({"until": 1.5, "reason": "x", "l": [1, 2]});
        assert_eq!(
            dumps(&v, Style::Default).unwrap(),
            r#"{"until": 1.5, "reason": "x", "l": [1, 2]}"#
        );
        assert_eq!(
            dumps(&v, Style::Compact).unwrap(),
            r#"{"until":1.5,"reason":"x","l":[1,2]}"#
        );
        assert_eq!(dumps(&json!({}), Style::Indent2).unwrap(), "{}");
    }
}
