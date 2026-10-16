//! The clock firmware that ships inside the app, and when to install it (`firmware.py`).
//!
//! A public build carries none (private, git-ignored in Python): [`load_bundle`] is `None` and
//! the feed ([`crate::feed`]) is the source. `fwSeq`, `layouts` and `styleApi` are what an image
//! reports in `/api/heap` once it runs; the app pushes an image only to a clawd clock whose
//! `fwSeq` is lower (missing = 0), so it never downgrades one. `heap` arguments are the clock's
//! `/api/heap` JSON.
//!
//! Next to the image, `styles/` holds the style packs built with that firmware (`index.json` +
//! `<id>.pak`). A pack is sent to the clock (`POST /api/style`) only when the clock draws its
//! API, either already or once the bundled image is installed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clawd_contracts::Env;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::crc32::crc32;
use crate::pyval;

pub use clawd_contracts::CLAWD_VARIANT;

/// `firmware.StylePack`: one style pack from a bundle's `styles/index.json` (or a feed release,
/// with `data` in memory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StylePack {
    /// `[a-z0-9_-]{1,15}`.
    pub id: String,
    /// At most 23 chars.
    pub name: String,
    pub path: PathBuf,
    pub size: usize,
    /// CRC-32 of bytes `[16..)`, from the pack header.
    pub crc: u32,
    pub light: bool,
    /// In memory (feed releases); `None` = read `path`.
    pub data: Option<Vec<u8>>,
}

impl StylePack {
    /// `StylePack.read()`: the pack, checked against the index and its own header (`CSTY`
    /// magic, size, CRC); a damaged one is never sent (`Err`).
    pub fn read(&self) -> Result<Vec<u8>, String> {
        let data = match &self.data {
            Some(d) => d.clone(),
            None => std::fs::read(&self.path).map_err(|e| e.to_string())?,
        };
        let header_crc = data
            .get(12..16)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let body_crc = crc32(data.get(16..).unwrap_or(&[]));
        if data.len() != self.size
            || data.get(..4) != Some(STYLE_MAGIC.as_slice())
            || header_crc != self.crc
            || body_crc != self.crc
        {
            return Err(format!("style pack {} does not match the index", self.id));
        }
        Ok(data)
    }
}

/// `firmware.STYLE_MAGIC`.
pub const STYLE_MAGIC: &[u8; 4] = b"CSTY";
/// `firmware.STYLE_MAX`: the pack compiler's cap.
pub const STYLE_MAX: usize = 20_480;

/// `firmware.STYLE_ID.fullmatch(id)`: `[a-z0-9_-]{1,15}`.
pub fn is_valid_style_id(id: &str) -> bool {
    (1..=15).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// `firmware.bundle_dir()`: `env` var `CLAWD_FIRMWARE_DIR`, else `firmware/` in the app's
/// resources dir ([`crate::resources_dir`]).
pub fn bundle_dir(env: &Env) -> PathBuf {
    bundle_dir_in(env, &crate::resources_dir())
}

/// [`bundle_dir`] with the resources dir given (the app passes Tauri's `resource_dir()`).
pub fn bundle_dir_in(env: &Env, resource_dir: &Path) -> PathBuf {
    match env.get("CLAWD_FIRMWARE_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => resource_dir.join("firmware"),
    }
}

/// `firmware.load_styles(folder, api)`: the packs in `folder/index.json` built for style API
/// `api`; empty when none or unreadable.
pub fn load_styles(folder: &Path, api: u32) -> Vec<StylePack> {
    load_styles_inner(folder, api).unwrap_or_default()
}

fn load_styles_inner(folder: &Path, api: u32) -> Option<Vec<StylePack>> {
    let text = std::fs::read_to_string(folder.join("index.json")).ok()?;
    let m: Value = serde_json::from_str(&text).ok()?;
    if pyval::int_of(m.get("api")?)? != i64::from(api) || api < 1 {
        return None;
    }
    let mut out = Vec::new();
    for e in m.get("styles")?.as_array()? {
        let sid = pyval::str_of(e.get("id")?);
        let file = pyval::str_of(e.get("file")?);
        let size = pyval::int_of(e.get("size")?)?;
        if !is_valid_style_id(&sid)
            || file.contains('/')
            || file.contains('\\')
            || !(0 < size && size <= STYLE_MAX as i64)
        {
            return None;
        }
        out.push(StylePack {
            id: sid,
            name: pyval::str_of(e.get("name")?).chars().take(23).collect(),
            path: folder.join(&file),
            size: size as usize,
            crc: u32::try_from(pyval::int_from_str(&pyval::str_of(e.get("crc")?), 16)?).ok()?,
            light: e.get("light").is_some_and(pyval::truthy),
            data: None,
        });
    }
    let ids: HashSet<&str> = out.iter().map(|p| p.id.as_str()).collect();
    if ids.len() != out.len() || !out.iter().all(|p| p.path.is_file()) {
        return None;
    }
    Some(out)
}

/// `firmware.Bundle`: firmware image + manifest (`firmware.json`: `file size sha256 variant fwSeq
/// layouts styleApi commit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub path: PathBuf,
    pub size: usize,
    pub sha256: String,
    pub variant: String,
    /// `fwSeq`.
    pub seq: u64,
    pub layouts: u32,
    pub commit: String,
    /// Style packs the image draws (0: none).
    pub style_api: u32,
    pub styles: Vec<StylePack>,
}

/// `hashlib.sha256(data).hexdigest()`.
pub(crate) fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

impl Bundle {
    /// `Bundle.read()`: the image, checked against the manifest (size and SHA-256).
    pub fn read(&self) -> Result<Vec<u8>, String> {
        let data = std::fs::read(&self.path).map_err(|e| e.to_string())?;
        if data.len() != self.size || sha256_hex(&data) != self.sha256 {
            return Err("bundled firmware does not match its manifest".to_owned());
        }
        Ok(data)
    }
}

/// `firmware.load_bundle(folder)`: `None` when missing, damaged, or the image is not a file.
/// `folder` defaults to [`bundle_dir`].
pub fn load_bundle(env: &Env, folder: Option<&Path>) -> Option<Bundle> {
    let folder = folder.map_or_else(|| bundle_dir(env), Path::to_path_buf);
    let text = std::fs::read_to_string(folder.join("firmware.json")).ok()?;
    let m: Value = serde_json::from_str(&text).ok()?;
    let style_api = match m.get("styleApi") {
        Some(v) => u32::try_from(pyval::int_of(v)?).ok()?,
        None => 0,
    };
    let mut b = Bundle {
        path: folder.join(pyval::str_of(m.get("file")?)),
        size: usize::try_from(pyval::int_of(m.get("size")?)?).ok()?,
        sha256: pyval::str_of(m.get("sha256")?),
        variant: pyval::str_of(m.get("variant")?),
        seq: u64::try_from(pyval::int_of(m.get("fwSeq")?)?).ok()?,
        layouts: u32::try_from(pyval::int_of(m.get("layouts")?)?).ok()?,
        commit: m.get("commit").map(pyval::str_of).unwrap_or_default(),
        style_api,
        styles: Vec::new(),
    };
    b.styles = load_styles(&folder.join("styles"), b.style_api);
    b.path.is_file().then_some(b)
}

/// `firmware.is_clawd(heap)`: the clawd firmware: says so (`variant`), or predates that field
/// but has the desk player (`desk` and `pak` keys).
pub fn is_clawd(heap: &Value) -> bool {
    let Some(h) = heap.as_object() else {
        return false;
    };
    match h.get("variant") {
        Some(v) if !v.is_null() => v.as_str() == Some(CLAWD_VARIANT),
        _ => h.contains_key("desk") && h.contains_key("pak"),
    }
}

/// `firmware.clock_seq(heap)`: `fwSeq` (an integer, not a bool), else 0.
pub fn clock_seq(heap: &Value) -> u64 {
    match heap.get("fwSeq") {
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => n.as_u64().unwrap_or(0),
        _ => 0,
    }
}

/// `firmware.update_state(heap, bundle)`: `"available"` | `"current"` | `"none"` (no bundle) |
/// `"incompatible"` (not a clawd clock).
pub fn update_state(heap: &Value, bundle: Option<&Bundle>) -> &'static str {
    let Some(bundle) = bundle else {
        return "none";
    };
    if !is_clawd(heap) || bundle.variant != CLAWD_VARIANT {
        "incompatible"
    } else if clock_seq(heap) < bundle.seq {
        "available"
    } else {
        "current"
    }
}

/// `firmware.clock_style_api(heap)`: the style pack API the clock draws (0: an image from before
/// style packs).
pub fn clock_style_api(heap: &Value) -> u32 {
    match heap.get("styleApi") {
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => {
            n.as_u64().map_or(0, |v| u32::try_from(v).unwrap_or(u32::MAX))
        }
        _ => 0,
    }
}

/// `firmware.clock_style(heap)`: the id of the pack the clock shows, `""` for the classic screen.
pub fn clock_style(heap: &Value) -> String {
    if clock_style_api(heap) == 0 {
        return String::new();
    }
    heap.get("style")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// `firmware.style_state(heap, bundle)`: for the bundled packs `"ready"` (the clock draws them)
/// | `"update"` (after installing the bundled image) | `"unavailable"`.
pub fn style_state(heap: &Value, bundle: Option<&Bundle>) -> &'static str {
    let Some(b) = bundle.filter(|b| !b.styles.is_empty()) else {
        return "unavailable";
    };
    if clock_style_api(heap) == b.style_api {
        "ready"
    } else if update_state(heap, Some(b)) == "available" {
        "update"
    } else {
        "unavailable"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pack(sid: &str) -> Vec<u8> {
        let mut rest = sid.as_bytes().to_vec();
        rest.resize(16, 0);
        rest.extend_from_slice(&[0; 64]);
        let mut out = b"CSTY".to_vec();
        out.extend_from_slice(&[1, 0, 96, 0]);
        out.extend_from_slice(&(rest.len() as u32).to_le_bytes());
        out.extend_from_slice(&crc32(&rest).to_le_bytes());
        out.extend_from_slice(&rest);
        out
    }

    fn write_styles(folder: &Path, ids: &[&str], api: u32) {
        std::fs::create_dir_all(folder).unwrap();
        let mut rows = Vec::new();
        for sid in ids {
            let data = pack(sid);
            std::fs::write(folder.join(format!("{sid}.pak")), &data).unwrap();
            rows.push(json!({"id": sid, "name": sid, "file": format!("{sid}.pak"),
                "size": data.len(), "crc": format!("{:08x}", crc32(&data[16..])),
                "light": *sid == "paper"}));
        }
        std::fs::write(
            folder.join("index.json"),
            json!({"api": api, "styles": rows}).to_string(),
        )
        .unwrap();
    }

    fn make_bundle(dir: &Path, seq: u64, style_api: u32, styles: &[&str]) -> Option<Bundle> {
        std::fs::create_dir_all(dir).unwrap();
        let image = [&[0xe9u8][..], &[0u8; 999]].concat();
        std::fs::write(dir.join("clawd-firmware.bin"), &image).unwrap();
        std::fs::write(
            dir.join("firmware.json"),
            json!({"file": "clawd-firmware.bin", "size": image.len(), "sha256": sha256_hex(&image),
                "variant": "esp8266-clawd", "fwSeq": seq, "layouts": 1, "styleApi": style_api,
                "commit": "abc1234"})
            .to_string(),
        )
        .unwrap();
        if !styles.is_empty() {
            write_styles(&dir.join("styles"), styles, 1);
        }
        load_bundle(&Env::empty(), Some(dir))
    }

    #[test]
    fn load_and_damage_check() {
        let t = tempfile::tempdir().unwrap();
        let b = make_bundle(t.path(), 1, 0, &[]).unwrap();
        assert_eq!((b.seq, b.layouts, b.read().unwrap().len()), (1, 1, 1000));
        assert!(b.styles.is_empty());
        std::fs::write(t.path().join("clawd-firmware.bin"), [0xe9u8; 1000]).unwrap();
        assert!(b.read().is_err());
        assert!(load_bundle(&Env::empty(), Some(&t.path().join("missing"))).is_none());
    }

    #[test]
    fn update_states_like_python() {
        let t = tempfile::tempdir().unwrap();
        let b = make_bundle(t.path(), 1, 0, &[]).unwrap();
        let old = json!({"version": "2.16.0", "pak": 17, "desk": 31});
        let new = json!({"variant": "esp8266-clawd", "fwSeq": 1});
        assert_eq!(update_state(&old, Some(&b)), "available");
        assert_eq!(update_state(&new, Some(&b)), "current");
        assert_eq!(update_state(&json!({"variant": "esp8266-clawd", "fwSeq": 2}), Some(&b)), "current");
        assert_eq!(update_state(&json!({"version": "2.16.0"}), Some(&b)), "incompatible");
        assert_eq!(update_state(&json!({"variant": "esp8266", "desk": 1, "pak": 1}), Some(&b)), "incompatible");
        assert_eq!(update_state(&old, None), "none");
    }

    #[test]
    fn style_packs_and_states() {
        let t = tempfile::tempdir().unwrap();
        let b = make_bundle(t.path(), 2, 1, &["synth", "paper"]).unwrap();
        assert_eq!(b.styles.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["synth", "paper"]);
        assert!(b.styles[1].light);
        assert_eq!(&b.styles[0].read().unwrap()[..4], b"CSTY");
        let new = json!({"variant": "esp8266-clawd", "fwSeq": 1});
        let styled = json!({"variant": "esp8266-clawd", "fwSeq": 2, "styleApi": 1, "style": "synth"});
        assert_eq!(style_state(&new, Some(&b)), "update");
        assert_eq!(style_state(&styled, Some(&b)), "ready");
        assert_eq!(style_state(&json!({"variant": "esp8266-clawd", "fwSeq": 2, "styleApi": 2}), Some(&b)), "unavailable");
        assert_eq!(style_state(&new, None), "unavailable");
        assert_eq!(clock_style(&styled), "synth");
        assert_eq!(clock_style(&new), "");
        assert_eq!(clock_style(&json!({"styleApi": true, "style": "x"})), "");
        let pak = t.path().join("styles").join("synth.pak");
        let mut data = std::fs::read(&pak).unwrap();
        *data.last_mut().unwrap() = b'x';
        std::fs::write(&pak, data).unwrap();
        assert!(b.styles[0].read().is_err());
    }

    #[test]
    fn style_index_must_match_the_image() {
        let t = tempfile::tempdir().unwrap();
        assert!(make_bundle(&t.path().join("a"), 2, 2, &["synth"]).unwrap().styles.is_empty());
        assert!(make_bundle(&t.path().join("b"), 2, 0, &["synth"]).unwrap().styles.is_empty());
        let c = t.path().join("c");
        make_bundle(&c, 2, 1, &["synth"]).unwrap();
        let idx_path = c.join("styles").join("index.json");
        let mut idx: Value = serde_json::from_slice(&std::fs::read(&idx_path).unwrap()).unwrap();
        idx["styles"][0]["file"] = json!("../firmware.json");
        std::fs::write(&idx_path, idx.to_string()).unwrap();
        assert!(load_bundle(&Env::empty(), Some(&c)).unwrap().styles.is_empty());
        assert!(make_bundle(&t.path().join("d"), 2, 1, &["Bad Id"]).unwrap().styles.is_empty());
    }

    #[test]
    fn bundle_dir_prefers_the_environment() {
        let env = Env::from_fn(|k| (k == "CLAWD_FIRMWARE_DIR").then(|| "/x/fw".to_owned()));
        assert_eq!(bundle_dir_in(&env, Path::new("/res")), PathBuf::from("/x/fw"));
        assert_eq!(
            bundle_dir_in(&Env::empty(), Path::new("/res")),
            Path::new("/res").join("firmware")
        );
    }

    #[test]
    fn heap_readers() {
        assert_eq!(clock_seq(&json!({"fwSeq": true})), 0);
        assert_eq!(clock_seq(&json!({"fwSeq": 1.0})), 0);
        assert_eq!(clock_seq(&json!({"fwSeq": 4})), 4);
        assert!(is_clawd(&json!({"desk": 1, "pak": 1})));
        assert!(!is_clawd(&json!({"variant": "x", "desk": 1, "pak": 1})));
        assert!(is_clawd(&json!({"variant": null, "desk": 1, "pak": 1})));
        assert_eq!(clock_style_api(&json!({"styleApi": 0})), 0);
        assert!(is_valid_style_id("a_b-9") && !is_valid_style_id("") && !is_valid_style_id(&"a".repeat(16)));
    }
}
