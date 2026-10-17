//! Golden tests: the Rust port reproduces what the real Python `firmware.py` / `preview.py` (and,
//! for the pack plan, `feed.py`) produced (`python-0.1.x:tools/golden/export_fw.py` -> `fixtures/fw/*.json`). Byte inputs are
//! specs regenerated here; byte outputs are hex / sha256 digests.
mod common;

use std::path::Path;

use clawd_contracts::{Env, UsagePayload};
use clawd_firmware::crc32::crc32;
use clawd_firmware::firmware::{self, sha256_hex, Bundle, PackFile, StylePack};
use clawd_firmware::packs;
use clawd_firmware::preview;
use clawd_testkit::load_fixture_value;
use serde_json::{json, Value};

fn fixture(name: &str) -> Value {
    load_fixture_value("fw", name)["output"].clone()
}

fn arr(v: &Value) -> &Vec<Value> {
    v.as_array().unwrap_or_else(|| panic!("not an array: {v}"))
}

/// `Ok(value)` / `Err(text)` from the exporter's `{"ok": v}` / `{"error": text}`.
fn expected(v: &Value) -> Result<&Value, &str> {
    match (v.get("ok"), v.get("error")) {
        (Some(ok), None) => Ok(ok),
        (None, Some(e)) => Err(e.as_str().expect("error text")),
        _ => panic!("bad result {v}"),
    }
}

// ---- byte specs ------------------------------------------------------------------------------

fn pattern(n: usize, seed: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 7 + seed) % 251) as u8).collect()
}

fn style_pack(sid: &str, flip_last: bool) -> Vec<u8> {
    let mut rest = sid.as_bytes().to_vec();
    rest.resize(16, 0);
    rest.extend_from_slice(&[0; 64]);
    let mut out = b"CSTY".to_vec();
    out.extend_from_slice(&[1, 0, 96, 0]);
    out.extend_from_slice(&(rest.len() as u32).to_le_bytes());
    out.extend_from_slice(&crc32(&rest).to_le_bytes());
    out.extend(rest);
    if flip_last {
        *out.last_mut().unwrap() ^= 1;
    }
    out
}

fn materialize(spec: &Value) -> Vec<u8> {
    if let Some(p) = spec.get("pattern") {
        pattern(
            p[0].as_u64().unwrap() as usize,
            p[1].as_u64().unwrap() as usize,
        )
    } else if let Some(t) = spec.get("text") {
        t.as_str().unwrap().as_bytes().to_vec()
    } else if let Some(j) = spec.get("json") {
        clawd_firmware::pyval::json_dumps(j, false).into_bytes()
    } else if let Some(s) = spec.get("style_pack") {
        style_pack(s[0].as_str().unwrap(), s[1].as_bool().unwrap_or(false))
    } else {
        panic!("unknown byte spec {spec}")
    }
}

fn sha(data: &[u8]) -> String {
    sha256_hex(data)
}

// ---- heaps, plan, offer ---------------------------------------------------------------------------

fn bundle_of(spec: &Value) -> Bundle {
    Bundle {
        path: "fw.bin".into(),
        size: 1,
        sha256: String::new(),
        variant: spec["variant"].as_str().unwrap().into(),
        seq: spec["seq"].as_u64().unwrap(),
        layouts: 1,
        commit: String::new(),
        style_api: spec["style_api"].as_u64().unwrap() as u32,
        styles: arr(&spec["styles"])
            .iter()
            .map(|id| StylePack {
                id: id.as_str().unwrap().into(),
                name: id.as_str().unwrap().into(),
                path: id.as_str().unwrap().into(),
                size: 1,
                crc: 0,
                light: false,
                data: None,
            })
            .collect(),
        style_rev: 0,
        packs: Vec::new(),
    }
}

#[test]
fn firmware_heap_readers_match_python() {
    let out = fixture("heap_state");
    let bundles: Vec<Bundle> = arr(&out["bundles"]).iter().map(bundle_of).collect();
    for row in arr(&out["rows"]) {
        let (heap, want) = (&row["heap"], &row["result"]);
        assert_eq!(
            &json!(firmware::is_clawd(heap)),
            &want["is_clawd"],
            "{heap}"
        );
        assert_eq!(
            &json!(firmware::clock_seq(heap)),
            &want["clock_seq"],
            "{heap}"
        );
        assert_eq!(
            &json!(firmware::clock_style_api(heap)),
            &want["clock_style_api"],
            "{heap}"
        );
        assert_eq!(
            &json!(firmware::clock_style(heap)),
            &want["clock_style"],
            "{heap}"
        );
        for (i, b) in bundles.iter().enumerate() {
            assert_eq!(
                &json!(firmware::update_state(heap, Some(b))),
                &want["update_state"][i],
                "{heap} bundle {i}"
            );
            assert_eq!(
                &json!(firmware::style_state(heap, Some(b))),
                &want["style_state"][i],
                "{heap} bundle {i}"
            );
        }
        assert_eq!(
            &json!(firmware::update_state(heap, None)),
            &want["update_state_none"]
        );
        assert_eq!(
            &json!(firmware::style_state(heap, None)),
            &want["style_state_none"]
        );
    }
}

/// The bundle a feed manifest described (Python planned against the release; the app now plans
/// against the bundle, with the same rule).
fn bundle_of_manifest(m: &Value) -> Bundle {
    let packs = arr(&m["files"])
        .iter()
        .filter(|f| packs::PACK_KINDS.contains(&f["kind"].as_str().unwrap()))
        .map(|f| PackFile {
            kind: f["kind"].as_str().unwrap().into(),
            path: f["name"].as_str().unwrap().into(),
            size: f["size"].as_u64().unwrap() as usize,
            crc: f["crc"].as_str().unwrap().into(),
        })
        .collect();
    Bundle {
        path: "firmware.bin".into(),
        size: 1,
        sha256: String::new(),
        variant: m["variant"].as_str().unwrap().into(),
        seq: m["seq"].as_u64().unwrap(),
        layouts: m["layouts"].as_u64().unwrap() as u32,
        commit: m["commit"].as_str().unwrap().into(),
        style_api: m["styleApi"].as_u64().unwrap() as u32,
        styles: Vec::new(),
        style_rev: 0,
        packs,
    }
}

#[test]
fn plan_worth_it_and_pack_state_match_python() {
    let out = fixture("feed_state");
    let b = bundle_of_manifest(&out["manifest"]);
    for row in arr(&out["rows"]) {
        let (heap, want) = (&row["heap"], &row["result"]);
        // key order is part of the contract (fw, clawdpak, deskpak): compare the text
        assert_eq!(
            serde_json::to_string(&packs::plan(heap, Some(&b))).unwrap(),
            serde_json::to_string(&want["plan"]).unwrap(),
            "{heap}"
        );
        assert_eq!(
            json!(packs::worth_it(heap, Some(&b))),
            want["worth_it"],
            "{heap}"
        );
        let offered = if want["worth_it"].as_bool().unwrap() {
            b.seq
        } else {
            0
        };
        assert_eq!(packs::offer(heap, Some(&b)), offered, "{heap}");
        assert!(
            packs::plan(heap, None).is_empty() && want["plan_none"].as_object().unwrap().is_empty()
        );
    }
    for row in arr(&out["pack_state_rows"]) {
        for kind in ["clawdpak", "deskpak"] {
            assert_eq!(
                json!(packs::pack_state(&row["heap"], kind, "aaaaaaaa")),
                row["result"][kind],
                "{} {kind}",
                row["heap"]
            );
        }
    }
}

// ---- bundle on disk, style packs ------------------------------------------------------------------

fn write_spec_files(folder: &Path, files: &Value) {
    for (rel, spec) in files.as_object().unwrap() {
        let p = folder.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, materialize(spec)).unwrap();
    }
}

fn read_result(r: Result<Vec<u8>, String>) -> Value {
    match r {
        Ok(d) => json!(sha(&d)),
        Err(e) => json!({"error": e}),
    }
}

#[test]
fn bundles_on_disk_match_python() {
    let out = fixture("bundle");
    let t = tempfile::tempdir().unwrap();
    for c in arr(&out["bundles"]) {
        let name = c["name"].as_str().unwrap();
        let folder = t.path().join(name);
        write_spec_files(&folder, &c["files"]);
        let got = firmware::load_bundle(&Env::empty(), Some(&folder));
        match (&c["result"], got) {
            (Value::Null, None) => {}
            (want, Some(b)) => {
                let styles: Vec<Value> = b
                    .styles
                    .iter()
                    .map(|p| {
                        json!({"id": p.id, "name": p.name, "size": p.size, "crc": p.crc, "light": p.light,
                               "read": read_result(p.read())})
                    })
                    .collect();
                let got = json!({"seq": b.seq, "layouts": b.layouts, "commit": b.commit,
                    "style_api": b.style_api, "size": b.size, "variant": b.variant,
                    "image": read_result(b.read()), "styles": styles});
                // OS error texts differ; the exporter already blanked the folder name
                assert_eq!(&got, want, "{name}");
            }
            (want, got) => panic!("{name}: python {want}, rust {got:?}"),
        }
    }
}

// ---- preview ----------------------------------------------------------------------------------------

fn clip_from_spec(frames: &[Value]) -> Vec<u8> {
    let mut out = b"CLP1".to_vec();
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    for f in frames {
        let n = |k: &str| f[k].as_u64().unwrap() as usize;
        for k in ["delay", "x", "y", "w", "h"] {
            out.extend_from_slice(&(n(k) as u16).to_le_bytes());
        }
        out.extend(pattern(n("w") * n("h") * 3, n("seed")));
    }
    out
}

fn frame_summary(f: &preview::Frame) -> Value {
    json!({"delay_ms": f.delay_ms, "x": f.x, "y": f.y, "w": f.w, "h": f.h, "rgb_sha256": sha(&f.rgb)})
}

/// The exporter's `normalized_apng`: chunk kinds and the inflated IDAT/fdAT payloads.
fn normalized_apng(png: &[u8]) -> String {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let (mut off, mut parts) = (8usize, Vec::new());
    while off < png.len() {
        let n = u32::from_be_bytes(png[off..off + 4].try_into().unwrap()) as usize;
        let kind = &png[off + 4..off + 8];
        let mut body = png[off + 8..off + 8 + n].to_vec();
        let inflate =
            |z: &[u8]| miniz_oxide::inflate::decompress_to_vec_zlib(z).expect("valid zlib");
        match kind {
            b"IDAT" => body = inflate(&body),
            b"fdAT" => body = [body[..4].to_vec(), inflate(&body[4..])].concat(),
            _ => {}
        }
        parts.extend_from_slice(kind);
        parts.extend_from_slice(&(body.len() as u32).to_be_bytes());
        parts.extend(body);
        off += 12 + n;
    }
    sha(&parts)
}

#[test]
fn clips_and_apng_match_python() {
    let out = fixture("preview");
    for c in arr(&out["clips"]) {
        let name = c["name"].as_str().unwrap();
        let mut data = match (c["frames"].as_array(), c["raw_hex"].as_str()) {
            (Some(f), _) => clip_from_spec(f),
            (None, Some(h)) => hex::decode(h).unwrap(),
            _ => panic!("{name}"),
        };
        match c["mutate"].as_str() {
            Some("cut") => {
                data.pop();
            }
            Some("trailing") => data.push(b'x'),
            Some("count_too_high") => {
                data = [b"CLP1\x02\x00".to_vec(), data[6..].to_vec()].concat()
            }
            _ => {}
        }
        let got = preview::parse_clip(&data);
        match (expected(&c["result"]), got) {
            (Ok(want), Ok(frames)) => {
                let summary: Vec<Value> = frames.iter().map(frame_summary).collect();
                assert_eq!(&Value::Array(summary), want, "{name}");
                let png = preview::try_apng(&frames).unwrap();
                assert_eq!(
                    normalized_apng(&png),
                    c["apng_normalized_sha256"].as_str().unwrap(),
                    "{name}"
                );
                let url = preview::try_apng_data_url(&frames).unwrap();
                assert_eq!(&url[..22], c["apng_url_prefix"].as_str().unwrap());
                assert!(url.len() > 22 && c["apng_url_len_ge"].as_bool().unwrap());
            }
            (Err(want), Err(e)) => assert_eq!(e.to_string(), want, "{name}"),
            (want, got) => panic!("{name}: python {want:?}, rust {got:?}"),
        }
    }
    for c in arr(&out["apng_errors"]) {
        let frames: Vec<preview::Frame> = if c["name"] == "no_frames" {
            Vec::new()
        } else {
            vec![preview::Frame {
                delay_ms: 1,
                x: 0,
                y: 0,
                w: 2,
                h: 2,
                rgb: vec![0; 12],
            }]
        };
        let e = preview::try_apng(&frames).unwrap_err().to_string();
        assert_eq!(Err(e.as_str()), expected(&c["result"]), "{}", c["name"]);
    }
}

#[test]
fn bmp_matches_python() {
    for c in arr(&fixture("preview")["bmp"]) {
        let rgb = pattern(
            c["len"].as_u64().unwrap() as usize,
            c["seed"].as_u64().unwrap() as usize,
        );
        match (expected(&c["result"]), preview::bmp_data_url(&rgb)) {
            (Ok(want), Ok(url)) => assert_eq!(sha(url.as_bytes()), want.as_str().unwrap()),
            (Err(want), Err(e)) => assert_eq!(e.to_string(), want),
            (want, got) => panic!("python {want:?}, rust {got:?}"),
        }
    }
}

#[test]
fn live_payload_frame_key_and_commits_match_python() {
    let out = fixture("preview");
    let as_payload = |v: &Value| v.as_object().map(|o| UsagePayload(o.clone()));
    for c in arr(&out["live_payload"]) {
        let got = preview::live_payload_at(
            as_payload(&c["payload"]).as_ref(),
            c["now"].as_f64().unwrap(),
            c["gmtoff"].as_i64().unwrap(),
        );
        let got = got.map_or(Value::Null, |p| Value::Object(p.0));
        assert_eq!(
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&c["result"]).unwrap(),
            "{c}"
        );
    }
    let keys = arr(&out["frame_key"]);
    assert!(keys.len() > 30);
    for c in keys {
        let got = preview::frame_key(
            c["style"].as_str().unwrap(),
            as_payload(&c["payload"]).as_ref(),
            c["minute"].as_u64().unwrap(),
            c["kind"].as_str().unwrap(),
        );
        assert_eq!(&got, c["result"].as_str().unwrap(), "{c}");
    }
    for c in arr(&out["same_commit"]) {
        let got = preview::same_commit(c["a"].as_str().unwrap(), c["b"].as_str().unwrap());
        assert_eq!(json!(got), c["result"], "{c}");
    }
}

fn make_pak(count: usize, w: usize, h: usize, frames: usize) -> Vec<u8> {
    let fbytes = (w * h).div_ceil(2);
    let mut head = b"CLWD".to_vec();
    head.extend_from_slice(&[1, 0, count as u8, 0, 0, 0, 0, 0]);
    let (mut descs, mut body) = (Vec::new(), Vec::new());
    let base = 12 + 72 * count;
    for i in 0..count {
        let frames_off = base + body.len();
        body.extend(vec![0x12u8; fbytes * frames]);
        let holds_off = base + body.len();
        body.extend(std::iter::repeat_n([100u8, 0], frames).flatten());
        let mut d = format!("anim{i}").into_bytes();
        d.resize(16, 0);
        d.extend_from_slice(&[w as u8, h as u8, 0, 0]);
        for n in [frames, 0, frames.saturating_sub(1), fbytes] {
            d.extend_from_slice(&(n as u16).to_le_bytes());
        }
        d.extend(vec![0u8; 32]);
        d.extend_from_slice(&(frames_off as u32).to_le_bytes());
        d.extend_from_slice(&(holds_off as u32).to_le_bytes());
        d.extend_from_slice(&[0; 4]);
        descs.extend(d);
    }
    [head, descs, body].concat()
}

#[test]
fn pak_count_matches_python() {
    for c in arr(&fixture("preview")["pak_count"]) {
        let b = &c["build"];
        let get = |k: &str, d: u64| b.get(k).and_then(Value::as_u64).unwrap_or(d) as usize;
        let count = get("count", 2);
        let mut data = if c["mutate"].is_null() && (count == 0 || count >= 64) {
            let mut head = b"CLWD".to_vec();
            head.extend_from_slice(&[1, 0, count as u8, 0, 0, 0, 0, 0]);
            head.extend(vec![0u8; 72 * count.min(64)]);
            head
        } else {
            make_pak(count, get("w", 4), get("h", 3), get("frames", 2))
        };
        let patch = |d: &mut Vec<u8>, at: usize, v: &[u8]| d[at..at + v.len()].copy_from_slice(v);
        match c["mutate"].as_str() {
            Some("bad_magic") => patch(&mut data, 0, b"NOPE"),
            Some("truncated") => {
                data.pop();
            }
            Some("version_two") => patch(&mut data, 4, &[2, 0]),
            Some("short") => data.truncate(8),
            Some("empty") => data.clear(),
            Some("too_big") => data.extend(vec![0u8; preview::PAK_MAX]),
            Some("frames_off_beyond") => patch(&mut data, 12 + 60, &10_000_000u32.to_le_bytes()),
            Some("holds_off_beyond") => {
                let at = data.len() as u32 - 1;
                patch(&mut data, 12 + 64, &at.to_le_bytes());
            }
            Some("fbytes_small") => patch(&mut data, 12 + 26, &1u16.to_le_bytes()),
            _ => {}
        }
        match (expected(&c["result"]), preview::pak_count(&data)) {
            (Ok(want), Ok(n)) => assert_eq!(json!(n), *want, "{}", c["name"]),
            (Err(want), Err(e)) => assert_eq!(e.to_string(), want, "{}", c["name"]),
            (want, got) => panic!("{}: python {want:?}, rust {got:?}", c["name"]),
        }
    }
}
