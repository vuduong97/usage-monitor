//! `feed.parse_manifest` and its row checks: every field of untrusted JSON validated the way the
//! Python does, with the same error texts.

use std::collections::HashSet;

use clawd_contracts::{FeedError, FeedFile};
use serde_json::{Map, Value};

use super::{Manifest, KINDS, MAX_FILE, RENDERER_MAX, RENDERER_PLATFORMS};
use crate::pyval;

fn err(msg: impl Into<String>) -> FeedError {
    FeedError(msg.into())
}

/// `feed._int(v, lo, hi)`: an integer (not a bool) within bounds.
fn int_in(v: Option<&Value>, lo: i64, hi: i64) -> Result<i64, FeedError> {
    if let Some(Value::Number(n)) = v {
        if let Some(i) = n.as_i64() {
            if (lo..=hi).contains(&i) {
                return Ok(i);
            }
        }
    }
    Err(err(format!(
        "bad number {}",
        v.map_or_else(|| "None".to_owned(), pyval::repr)
    )))
}

/// `feed.NAME_RE.fullmatch`: `[a-z0-9][a-z0-9._-]{0,47}(/[a-z0-9][a-z0-9._-]{0,47})?`.
fn name_ok(name: &str) -> bool {
    let mut parts = name.split('/');
    let (Some(first), second, None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let segment = |s: &str| {
        let b = s.as_bytes();
        (1..=48).contains(&b.len())
            && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
            && b[1..].iter().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b'-')
            })
    };
    segment(first) && second.is_none_or(segment)
}

fn lower_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `feed._row(f, names, max_size, kinds)`: one file row, checked.
fn row(
    f: &Value,
    names: &HashSet<String>,
    max_size: i64,
    kinds: &[&str],
) -> Result<FeedFile, FeedError> {
    let Some(f) = f.as_object() else {
        return Err(err("bad file row"));
    };
    let (name, kind) = (f.get("name"), f.get("kind"));
    let name_text = match name {
        Some(Value::String(s)) if name_ok(s) && !s.contains("..") && !names.contains(s) => s.clone(),
        _ => {
            return Err(err(format!(
                "bad file name {}",
                name.map_or_else(|| "None".to_owned(), pyval::repr)
            )))
        }
    };
    let kind_text = match kind {
        Some(Value::String(k)) if kinds.contains(&k.as_str()) => k.clone(),
        _ => {
            return Err(err(format!(
                "unknown kind {}",
                kind.map_or_else(|| "None".to_owned(), pyval::repr)
            )))
        }
    };
    let size = int_in(f.get("size"), 1, max_size)?;
    let (sha, crc) = (f.get("sha256"), f.get("crc"));
    let (Some(Value::String(sha)), Some(Value::String(crc))) = (sha, crc) else {
        return Err(err(format!("bad hash for {name_text}")));
    };
    if !lower_hex(sha, 64) || !lower_hex(crc, 8) {
        return Err(err(format!("bad hash for {name_text}")));
    }
    Ok(FeedFile {
        name: name_text,
        kind: kind_text,
        size: size as u64,
        enc: int_in(f.get("enc"), size + 28, size + 28)? as u64,
        sha256: sha.clone(),
        crc: crc.clone(),
        platform: pyval::str_or_empty(f.get("platform")),
    })
}

/// `feed._renderers(rows, names)`: the release's renderers, one per platform. A row that does not
/// check out drops them all (the preview falls back to its canvas) but never the release: the
/// firmware and the packs do not depend on them.
fn renderers(rows: Option<&Value>, names: &HashSet<String>) -> Vec<FeedFile> {
    let Some(Value::Array(rows)) = rows else {
        return Vec::new();
    };
    let mut taken = names.clone();
    let mut out: Vec<FeedFile> = Vec::new();
    for f in rows {
        let Ok(r) = row(f, &taken, RENDERER_MAX as i64, &["renderer"]) else {
            return Vec::new();
        };
        if !RENDERER_PLATFORMS.contains(&r.platform.as_str())
            || out.iter().any(|o| o.platform == r.platform)
        {
            return Vec::new();
        }
        taken.insert(r.name.clone());
        out.push(r);
    }
    out
}

/// Python's `d.get("v") != 1`: `1`, `1.0` and `true` all equal 1.
fn is_version_one(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Number(n)) => n.as_i64() == Some(1) || n.as_f64() == Some(1.0),
        Some(Value::Bool(b)) => *b,
        _ => false,
    }
}

fn truncated(v: Option<&Value>, max_chars: usize) -> String {
    pyval::str_or_empty(v).chars().take(max_chars).collect()
}

/// `feed.parse_manifest(d)`: the manifest, every field checked (`v == 1`, names, kinds, sizes,
/// `enc == size + 28`, hashes, exactly one `fw`, `renderers` validated apart; a bad renderer row
/// drops the renderers, not the release). `Err` carries the Python `ValueError` text.
pub fn parse_manifest(d: &Value) -> Result<Manifest, FeedError> {
    let Some(obj) = d.as_object().filter(|o| is_version_one(o.get("v"))) else {
        return Err(err("not a v1 manifest"));
    };
    let mut files: Vec<FeedFile> = Vec::new();
    let mut names: HashSet<String> = HashSet::new();
    match obj.get("files") {
        Some(Value::Array(rows)) => {
            for f in rows {
                let r = row(f, &names, MAX_FILE as i64, &KINDS)?;
                names.insert(r.name.clone());
                files.push(r);
            }
        }
        Some(v) if pyval::truthy(v) => return Err(err("bad file row")),
        _ => {}
    }
    let m = build(obj, files, &names)?;
    if m.file("fw").is_none() || m.files.iter().filter(|f| f.kind == "fw").count() != 1 {
        return Err(err("a release has exactly one firmware image"));
    }
    Ok(m)
}

fn build(
    obj: &Map<String, Value>,
    files: Vec<FeedFile>,
    names: &HashSet<String>,
) -> Result<Manifest, FeedError> {
    let seq = int_in(obj.get("seq"), 1, 1_000_000)?;
    let variant = pyval::str_or_empty(obj.get("variant"));
    let default = |key: &str, d: i64| obj.get(key).cloned().unwrap_or(Value::from(d));
    let layouts = int_in(Some(&default("layouts", 1)), 1, 0xFFFF)?;
    let style_api = int_in(Some(&default("styleApi", 0)), 0, 255)?;
    Ok(Manifest {
        v: 1,
        seq: seq as u64,
        variant,
        layouts: layouts as u32,
        style_api: style_api as u32,
        files,
        date: truncated(obj.get("date"), 40),
        notes: truncated(obj.get("notes"), 2000),
        commit: truncated(obj.get("commit"), 40),
        renderers: renderers(obj.get("renderers"), names),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file(name: &str, kind: &str, size: u64) -> Value {
        json!({"name": name, "kind": kind, "size": size, "enc": size + 28,
               "sha256": "a".repeat(64), "crc": "0badc0de"})
    }

    fn good() -> Value {
        json!({"v": 1, "seq": 6, "date": "2026-10-14T08:00:00Z", "notes": "n", "variant": "esp8266-clawd",
               "layouts": 1, "styleApi": 1, "commit": "c000006",
               "files": [file("firmware.bin", "fw", 2048), file("clawd.pak", "clawdpak", 508)]})
    }

    fn with(change: impl FnOnce(&mut Value)) -> Value {
        let mut m = good();
        change(&mut m);
        m
    }

    #[test]
    fn a_good_manifest_parses() {
        let m = parse_manifest(&good()).unwrap();
        assert_eq!((m.seq, m.layouts, m.style_api), (6, 1, 1));
        assert_eq!(m.file("clawdpak").unwrap().enc, 536);
        assert!(m.renderers.is_empty());
    }

    #[test]
    fn manifest_checks_refuse_what_python_refuses() {
        let bad = [
            with(|m| m["files"].as_array_mut().unwrap().push(file("../x", "fw", 1))),
            with(|m| m["files"].as_array_mut().unwrap().push(file("clawd.pak", "clawdpak", 1))),
            with(|m| m["files"][1]["kind"] = json!("exe")),
            with(|m| m["files"][1]["enc"] = json!(508)),
            with(|m| m["files"][1]["crc"] = json!("xyz")),
            with(|m| m["v"] = json!(2)),
            with(|m| m["files"] = json!([file("clawd.pak", "clawdpak", 5)])),
            with(|m| m["seq"] = json!("6")),
            with(|m| m["seq"] = json!(true)),
            with(|m| m["seq"] = json!(1_000_001)),
            with(|m| m["files"][0]["size"] = json!(4 * 1024 * 1024 + 1)),
            with(|m| m["files"].as_array_mut().unwrap().push(file("b.bin", "fw", 9))),
            with(|m| m["layouts"] = json!(null)),
            json!([1]),
        ];
        for m in bad {
            assert!(parse_manifest(&m).is_err(), "{m}");
        }
    }

    #[test]
    fn error_texts_match_python() {
        let msg = |m: Value| parse_manifest(&m).unwrap_err().0;
        assert_eq!(msg(with(|m| m["v"] = json!(2))), "not a v1 manifest");
        assert_eq!(msg(with(|m| m["seq"] = json!("6"))), "bad number '6'");
        assert_eq!(msg(with(|m| m["files"][0]["name"] = json!("../x"))), "bad file name '../x'");
        assert_eq!(msg(with(|m| m["files"][1]["kind"] = json!("exe"))), "unknown kind 'exe'");
        assert_eq!(msg(with(|m| m["files"][1]["crc"] = json!("xyz"))), "bad hash for clawd.pak");
        assert_eq!(
            msg(with(|m| m["files"] = json!([file("clawd.pak", "clawdpak", 5)]))),
            "a release has exactly one firmware image"
        );
        assert_eq!(msg(with(|m| m["files"][1] = json!(3))), "bad file row");
    }

    #[test]
    fn v_accepts_what_python_equality_accepts() {
        assert!(parse_manifest(&with(|m| m["v"] = json!(1.0))).is_ok());
        assert!(parse_manifest(&with(|m| m["v"] = json!(true))).is_ok());
        assert!(parse_manifest(&with(|m| m["v"] = json!("1"))).is_err());
    }

    #[test]
    fn long_text_is_cut() {
        let m = parse_manifest(&with(|m| {
            m["notes"] = json!("é".repeat(3000));
            m["date"] = json!("d".repeat(60));
            m["commit"] = json!("c".repeat(60));
        }))
        .unwrap();
        assert_eq!((m.notes.chars().count(), m.date.len(), m.commit.len()), (2000, 40, 40));
    }

    #[test]
    fn names_follow_the_pattern() {
        for ok in ["a", "styles/synth.pak", "a.b_c-d", "0x"] {
            assert!(name_ok(ok), "{ok}");
        }
        for bad in ["", "A", "-a", "a/", "/a", "a/b/c", ".a", "a b", &"a".repeat(49)] {
            assert!(!name_ok(bad), "{bad}");
        }
    }

    fn renderer(name: &str, platform: &str) -> Value {
        let mut r = file(name, "renderer", 400);
        r["platform"] = json!(platform);
        r
    }

    #[test]
    fn renderers_are_kept_apart_and_validated_apart() {
        let m = parse_manifest(&with(|m| {
            m["renderers"] = json!([renderer("renderer/win-x64.zip", "win-x64"), renderer("renderer/mac.zip", "mac")])
        }))
        .unwrap();
        assert_eq!(m.renderers.iter().map(|r| r.platform.as_str()).collect::<Vec<_>>(), ["win-x64", "mac"]);
        assert!(m.renderer("linux-x64").is_none() && m.files.iter().all(|f| f.kind != "renderer"));
        let changes: [fn(&mut Value); 9] = [
            |r| r["platform"] = json!("amiga"),
            |r| r["kind"] = json!("fw"),
            |r| r["kind"] = json!([]),
            |r| r["name"] = json!("../x.zip"),
            |r| r["name"] = json!("firmware.bin"),
            |r| r["sha256"] = json!("nope"),
            |r| r["enc"] = json!(400),
            |r| r["size"] = json!(RENDERER_MAX + 1),
            |r| *r = json!(7),
        ];
        for change in changes {
            let parsed = parse_manifest(&with(|m| {
                m["renderers"] = json!([renderer("renderer/win-x64.zip", "win-x64"), renderer("renderer/mac.zip", "mac")]);
                change(&mut m["renderers"][0]);
            }))
            .unwrap();
            assert!(parsed.renderers.is_empty());
            assert!(parsed.file("fw").is_some());
        }
        let dup = parse_manifest(&with(|m| {
            m["renderers"] = json!([renderer("renderer/mac.zip", "mac"), renderer("renderer/mac2.zip", "mac")])
        }))
        .unwrap();
        assert!(dup.renderers.is_empty());
        let not_a_list = parse_manifest(&with(|m| m["renderers"] = json!({"a": 1}))).unwrap();
        assert!(not_a_list.renderers.is_empty());
    }
}
