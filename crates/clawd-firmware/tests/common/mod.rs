//! Helpers shared by the integration tests.
#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};

use clawd_firmware::crc32::{crc32, crc32_hex};
use clawd_testkit::fw::make_style_pack;
use serde_json::{json, Value};

/// A style index + packs in `folder` (`tests/test_firmware_ota.py:write_styles`).
pub fn write_styles(folder: &Path, ids: &[&str], api: u32) {
    std::fs::create_dir_all(folder).unwrap();
    let mut rows = Vec::new();
    for sid in ids {
        let data = make_style_pack(sid);
        std::fs::write(folder.join(format!("{sid}.pak")), &data).unwrap();
        rows.push(
            json!({"id": sid, "name": sid, "file": format!("{sid}.pak"), "size": data.len(),
            "crc": crc32_hex(&data[16..]), "light": *sid == "paper"}),
        );
    }
    std::fs::write(
        folder.join("index.json"),
        json!({"api": api, "styles": rows}).to_string(),
    )
    .unwrap();
}

pub fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (n, d) in files {
        w.start_file(*n, opts).unwrap();
        w.write_all(d).unwrap();
    }
    w.finish().unwrap().into_inner()
}

/// A renderer zip as `tools/build_preview.py --zip` packs it, around a fake program.
pub fn renderer_zip(commit: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let synth = make_style_pack("synth");
    let paper = make_style_pack("paper");
    let row = |id: &str, d: &[u8]| {
        json!({"id": id, "name": id, "file": format!("{id}.pak"), "size": d.len(),
               "crc": format!("{:08x}", crc32(&d[16..])), "light": id == "paper"})
    };
    let index =
        json!({"api": 1, "styles": [row("synth", &synth), row("paper", &paper)]}).to_string();
    let preview = json!({"exe": "clawd_render", "pak": "clawd.pak", "commit": commit,
        "layouts": 1, "styleApi": 1, "fwSeq": 6})
    .to_string();
    let mut files: Vec<(&str, &[u8])> = vec![
        ("preview.json", preview.as_bytes()),
        ("clawd_render", b"program"),
        ("clawd.pak", b"pak"),
        ("styles/index.json", index.as_bytes()),
        ("styles/synth.pak", &synth),
        ("styles/paper.pak", &paper),
    ];
    files.extend_from_slice(extra);
    zip_of(&files)
}

/// A folder with the fake renderer program (`fake_clawd_render`) as `clawd_render`, the way
/// `load_renderer` expects it; `mode` selects what the program does.
pub fn fake_renderer_folder(root: &Path, mode: &str, commit: &str) -> PathBuf {
    let d = root.join("renderer");
    std::fs::create_dir_all(&d).unwrap();
    write_styles(&d.join("styles"), &["synth", "paper"], 1);
    let exe = d.join(if cfg!(windows) {
        "clawd_render.exe"
    } else {
        "clawd_render"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_fake_clawd_render"), &exe).unwrap();
    std::fs::write(d.join("clawd.pak"), b"pak").unwrap();
    std::fs::write(d.join("mode"), mode).unwrap();
    let name = exe.file_name().unwrap().to_string_lossy().into_owned();
    std::fs::write(
        d.join("preview.json"),
        json!({"exe": name, "pak": "clawd.pak", "commit": commit, "layouts": 1, "styleApi": 1,
               "fwSeq": 2})
        .to_string(),
    )
    .unwrap();
    d
}

pub fn json_text(v: &Value) -> String {
    v.to_string()
}
