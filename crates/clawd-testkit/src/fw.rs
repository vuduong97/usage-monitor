//! Fakes for `clawd-firmware` tests: byte fixtures, a bundle folder laid out the way
//! `tools/bundle_firmware.py` writes one, and [`FakePacks`], a GitHub Release `packs-api{N}`
//! served on loopback by [`FakeHttpServer`] and signed by a key made for the test.

use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use clawd_firmware::crc32::crc32_hex;
use clawd_firmware::firmware::sha256_hex;
use serde_json::{json, Value};

use crate::core::{FakeHttpServer, FakeResponse, RecordedRequest};

/// A fake firmware image.
pub fn image() -> Vec<u8> {
    [&[0xe9u8][..], &[7u8; 2047]].concat()
}
/// A fake `clawd.pak` (not a real pack: the app only checks size/hash/CRC).
pub fn clawd_pak() -> Vec<u8> {
    [&b"CLW?\x01\x00\x11\x00"[..], &[b'c'; 500]].concat()
}
/// A fake `desk.pak`.
pub fn desk_pak() -> Vec<u8> {
    [&b"CLDK\x03\x00\x1f\x00"[..], &[b'd'; 3000]].concat()
}

/// A style pack as far as the app checks one: magic `CSTY`, CRC-32 of bytes `[16..)` at 12
/// (`tests/test_firmware_ota.py:make_pack`).
pub fn make_style_pack(id: &str) -> Vec<u8> {
    let mut rest = id.as_bytes().to_vec();
    rest.resize(16, 0);
    rest.extend_from_slice(&[0; 64]);
    let mut out = b"CSTY".to_vec();
    out.extend_from_slice(&[1, 0, 96, 0]);
    out.extend_from_slice(&(rest.len() as u32).to_le_bytes());
    out.extend_from_slice(&clawd_firmware::crc32::crc32(&rest).to_le_bytes());
    out.extend(rest);
    out
}

fn title(sid: &str) -> String {
    sid.chars()
        .enumerate()
        .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
        .collect()
}

/// A `styles/index.json` row for the pack `data` of `sid` (`sha256` included: the remote index
/// needs it, the bundled one ignores it).
pub fn style_row(sid: &str, data: &[u8]) -> Value {
    json!({"id": sid, "name": title(sid), "file": format!("{sid}.pak"), "size": data.len(),
           "crc": crc32_hex(&data[16..]), "light": sid == "paper", "sha256": sha256_hex(data)})
}

/// Write a bundle into `dir` the way `tools/bundle_firmware.py` does: `firmware.bin`, both
/// animation packs, `firmware.json`, and (when `styles` is not empty) `styles/` at `rev`.
pub fn write_bundle(dir: &Path, seq: u64, style_api: u32, styles: &[&str], rev: u64) -> PathBuf {
    std::fs::create_dir_all(dir).expect("bundle dir");
    let img = image();
    std::fs::write(dir.join("firmware.bin"), &img).expect("image");
    let mut packs = serde_json::Map::new();
    for (kind, file, data) in [
        ("clawdpak", "clawd.pak", clawd_pak()),
        ("deskpak", "desk.pak", desk_pak()),
    ] {
        std::fs::write(dir.join(file), &data).expect("pack");
        packs.insert(
            kind.to_owned(),
            json!({"file": file, "size": data.len(), "crc": crc32_hex(&data)}),
        );
    }
    let manifest = json!({"file": "firmware.bin", "size": img.len(), "sha256": sha256_hex(&img),
        "variant": "esp8266-clawd", "fwSeq": seq, "layouts": 1, "styleApi": style_api,
        "commit": "c0ffee1", "packs": packs});
    std::fs::write(dir.join("firmware.json"), manifest.to_string()).expect("manifest");
    if !styles.is_empty() {
        let sdir = dir.join("styles");
        std::fs::create_dir_all(&sdir).expect("styles");
        let rows: Vec<Value> = styles
            .iter()
            .map(|sid| {
                let data = make_style_pack(sid);
                std::fs::write(sdir.join(format!("{sid}.pak")), &data).expect("style pack");
                style_row(sid, &data)
            })
            .collect();
        std::fs::write(
            sdir.join("index.json"),
            json!({"api": style_api, "rev": rev, "styles": rows}).to_string(),
        )
        .expect("index");
    }
    dir.to_path_buf()
}

/// A key pair made for one test; the secret never leaves the process.
pub struct TestKey {
    pair: minisign::KeyPair,
}

impl Default for TestKey {
    fn default() -> Self {
        Self {
            pair: minisign::KeyPair::generate_unencrypted_keypair().expect("keypair"),
        }
    }
}

impl TestKey {
    /// The base64 line of the `.pub` file.
    pub fn public(&self) -> String {
        self.pair.pk.to_base64()
    }

    /// The text of the `.minisig` file for `data`.
    pub fn sign(&self, data: &[u8]) -> String {
        minisign::sign(
            Some(&self.pair.pk),
            &self.pair.sk,
            Cursor::new(data),
            Some("trusted comment"),
            Some("untrusted comment"),
        )
        .expect("sign")
        .to_string()
    }
}

#[derive(Default)]
struct State {
    /// Path (`/packs-api2/index.json`) -> body.
    files: std::collections::HashMap<String, Vec<u8>>,
    tamper: HashSet<String>,
}

/// GitHub's `releases/download/` for the packs releases: `/<tag>/<name>`, on a loopback port.
pub struct FakePacks {
    pub server: FakeHttpServer,
    pub key: TestKey,
    state: Arc<Mutex<State>>,
}

impl Default for FakePacks {
    fn default() -> Self {
        Self::start()
    }
}

impl FakePacks {
    pub fn start() -> Self {
        let server = FakeHttpServer::start().expect("fake packs server");
        let state = Arc::new(Mutex::new(State::default()));
        Self {
            server,
            key: TestKey::default(),
            state,
        }
    }

    /// The base URL a `StyleSource` takes: `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        self.server.url()
    }

    /// The public key the index is signed with.
    pub fn public_key(&self) -> String {
        self.key.public()
    }

    fn serve(&self, path: String, body: Vec<u8>) {
        self.state
            .lock()
            .expect("state")
            .files
            .insert(path.clone(), body);
        let st = self.state.clone();
        let p = path.clone();
        self.server.route_fn(
            "GET",
            &path,
            Arc::new(move |_req: &RecordedRequest| {
                let s = st.lock().expect("state");
                match s.files.get(&p) {
                    Some(b) => {
                        let mut b = b.clone();
                        if s.tamper.contains(&p) {
                            if let Some(last) = b.last_mut() {
                                *last ^= 1;
                            }
                        }
                        FakeResponse::bytes(200, b)
                    }
                    None => FakeResponse::status(404),
                }
            }),
        );
    }

    /// Publish (or replace) `packs-api{api}` at `rev` with these style packs; returns the index.
    pub fn publish(&self, api: u32, rev: u64, styles: &[&str]) -> Value {
        let tag = format!("/packs-api{api}");
        let rows: Vec<Value> = styles
            .iter()
            .map(|sid| {
                let data = make_style_pack(sid);
                let row = style_row(sid, &data);
                self.serve(format!("{tag}/{sid}.pak"), data);
                row
            })
            .collect();
        let index = json!({"api": api, "rev": rev, "styles": rows});
        self.publish_index(api, index.to_string().into_bytes(), None);
        index
    }

    /// Serve `index` as the index of `packs-api{api}`, signed by `sig` (default: the test key).
    pub fn publish_index(&self, api: u32, index: Vec<u8>, sig: Option<String>) {
        let tag = format!("/packs-api{api}");
        let sig = sig.unwrap_or_else(|| self.key.sign(&index));
        self.serve(format!("{tag}/index.json"), index);
        self.serve(format!("{tag}/index.json.minisig"), sig.into_bytes());
    }

    /// Serve `/<tag>/<name>` with a flipped last byte from now on.
    pub fn tamper(&self, api: u32, name: &str) {
        self.state
            .lock()
            .expect("state")
            .tamper
            .insert(format!("/packs-api{api}/{name}"));
    }

    /// How often `/<tag>/<name>` was downloaded.
    pub fn got(&self, api: u32, name: &str) -> usize {
        self.server
            .requests_to(&format!("/packs-api{api}/{name}"))
            .len()
    }
}
