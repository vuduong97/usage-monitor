//! Helpers shared by the integration tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use clawd_firmware::crc32::crc32_hex;
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
