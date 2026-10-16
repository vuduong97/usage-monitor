//! Fakes for `clawd-firmware` tests (phase 09 owns this file): a fake feed Worker (releases
//! encrypted the way clawd-home's `feed/release.mjs` does, served on loopback by
//! [`FakeHttpServer`]) and the byte fixtures it publishes. Port of `tests/feedkit.py`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use clawd_firmware::crc32::crc32_hex;
use clawd_firmware::feed::encrypt;
use clawd_firmware::firmware::sha256_hex;
use serde_json::{json, Value};

use crate::core::{FakeHttpServer, FakeResponse, RecordedRequest};

/// The feed app token the fake Worker accepts (not a secret).
pub const TOKEN: &str = "tok";
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

/// One published file: `(name, kind, plaintext)`.
pub type FeedBlob = (String, String, Vec<u8>);

/// `release_files()` of `feedkit.py`: firmware, both packs and (optionally) style packs.
pub fn release_files(styles: &[&str], api: u32) -> Vec<FeedBlob> {
    let mut files = vec![
        ("firmware.bin".to_owned(), "fw".to_owned(), image()),
        ("clawd.pak".to_owned(), "clawdpak".to_owned(), clawd_pak()),
        ("desk.pak".to_owned(), "deskpak".to_owned(), desk_pak()),
    ];
    if !styles.is_empty() {
        let (mut rows, mut packs) = (Vec::new(), Vec::new());
        for sid in styles {
            let data = make_style_pack(sid);
            let title: String = sid
                .chars()
                .enumerate()
                .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
                .collect();
            rows.push(
                json!({"id": sid, "name": title, "file": format!("{sid}.pak"),
                "size": data.len(), "crc": crc32_hex(&data[16..]), "light": *sid == "paper"}),
            );
            packs.push((format!("styles/{sid}.pak"), "style".to_owned(), data));
        }
        files.push((
            "styles/index.json".to_owned(),
            "styles".to_owned(),
            json!({"api": api, "styles": rows}).to_string().into_bytes(),
        ));
        files.extend(packs);
    }
    files
}

struct Release {
    manifest: Value,
    key: [u8; 32],
    blobs: HashMap<String, Vec<u8>>,
}

#[derive(Default)]
struct State {
    releases: HashMap<u64, Release>,
    channels: HashMap<String, u64>,
    tamper: HashSet<String>,
    /// `false`: a Worker from before renderers (its grant skips them).
    unsigned_renderers: bool,
    nonce: u64,
}

/// The Worker: `/v1/latest`, `/v1/grant`, `/v1/file/<seq>/<name>`, on a loopback port.
pub struct FakeFeed {
    pub server: FakeHttpServer,
    state: Arc<Mutex<State>>,
}

impl FakeFeed {
    pub fn start() -> Self {
        let server = FakeHttpServer::start().expect("fake feed server");
        let state = Arc::new(Mutex::new(State::default()));
        let st = state.clone();
        server.route_fn(
            "GET",
            "/v1/latest",
            Arc::new(move |req: &RecordedRequest| {
                if req.header("authorization") != Some(&format!("Bearer {TOKEN}")) {
                    return FakeResponse::status(401);
                }
                let channel = req
                    .query()
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("channel="))
                    .unwrap_or("stable");
                let s = st.lock().expect("state");
                match s.channels.get(channel) {
                    Some(seq) => FakeResponse::json(200, &s.releases[seq].manifest),
                    None => FakeResponse::status(404),
                }
            }),
        );
        let st = state.clone();
        server.route_fn(
            "POST",
            "/v1/grant",
            Arc::new(move |req: &RecordedRequest| {
                if req.header("authorization") != Some(&format!("Bearer {TOKEN}")) {
                    return FakeResponse::status(401);
                }
                let s = st.lock().expect("state");
                let Some(rel) = req
                    .body_json()
                    .and_then(|b| b["seq"].as_u64())
                    .and_then(|seq| s.releases.get(&seq).map(|r| (seq, r)))
                else {
                    return FakeResponse::status(404);
                };
                let (seq, rel) = rel;
                let urls: serde_json::Map<String, Value> = rel
                    .blobs
                    .keys()
                    .filter(|n| !s.unsigned_renderers || !n.starts_with("renderer/"))
                    .map(|n| (n.clone(), json!(format!("/v1/file/{seq}/{n}?exp=1&sig=ok"))))
                    .collect();
                let key = {
                    use std::fmt::Write as _;
                    // standard base64 without a dependency: 32 bytes -> 44 chars
                    let mut out = String::new();
                    let alphabet =
                        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
                    for chunk in rel.key.chunks(3) {
                        let n = chunk
                            .iter()
                            .enumerate()
                            .fold(0u32, |a, (i, b)| a | (u32::from(*b) << (16 - 8 * i)));
                        for i in 0..4 {
                            if i <= chunk.len() {
                                let _ = out.write_char(
                                    alphabet[((n >> (18 - 6 * i)) & 63) as usize] as char,
                                );
                            } else {
                                out.push('=');
                            }
                        }
                    }
                    out
                };
                FakeResponse::json(
                    200,
                    &json!({"seq": seq, "key": key, "expires": 0, "urls": urls}),
                )
            }),
        );
        Self { server, state }
    }

    /// `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        self.server.url()
    }

    /// Serve a Worker that does not sign renderers (grant skips them).
    pub fn sign_renderers(&self, on: bool) {
        self.state.lock().expect("state").unsigned_renderers = !on;
    }

    /// Serve `name` with a flipped last byte from now on.
    pub fn tamper(&self, name: &str) {
        self.state
            .lock()
            .expect("state")
            .tamper
            .insert(name.to_owned());
    }

    pub fn untamper(&self) {
        self.state.lock().expect("state").tamper.clear();
    }

    /// Change a row of a published manifest (e.g. make its hash lie).
    pub fn edit_manifest(&self, seq: u64, f: impl FnOnce(&mut Value)) {
        let mut s = self.state.lock().expect("state");
        f(&mut s.releases.get_mut(&seq).expect("release").manifest);
    }

    /// `FakeFeed.publish(seq, files, channel, notes, style_api, renderers)`: encrypt and serve a
    /// release; returns its manifest JSON. `renderers` are `(platform, zip bytes)`.
    pub fn publish(
        &self,
        seq: u64,
        files: Vec<FeedBlob>,
        channel: &str,
        notes: &str,
        style_api: u32,
        renderers: &[(&str, Vec<u8>)],
    ) -> Value {
        let mut s = self.state.lock().expect("state");
        let key: [u8; 32] =
            std::array::from_fn(|i| (seq as u8).wrapping_mul(31).wrapping_add(i as u8 * 7 + 1));
        let (mut blobs, mut rows, mut rrows) = (HashMap::new(), Vec::new(), Vec::new());
        let extra = renderers.iter().map(|(p, d)| {
            (
                format!("renderer/{p}.zip"),
                "renderer".to_owned(),
                d.clone(),
                Some(*p),
            )
        });
        let all = files
            .into_iter()
            .map(|(n, k, d)| (n, k, d, None))
            .chain(extra);
        for (name, kind, data, platform) in all {
            s.nonce += 1;
            let mut nonce = [0u8; 12];
            nonce[..8].copy_from_slice(&s.nonce.to_le_bytes());
            let blob = encrypt(&key, seq, &name, nonce, &data).expect("encrypt");
            let mut row = json!({"name": name, "kind": kind, "size": data.len(), "enc": blob.len(),
                "sha256": sha256_hex(&data), "crc": crc32_hex(&data)});
            blobs.insert(name.clone(), blob);
            match platform {
                Some(p) => {
                    row["platform"] = json!(p);
                    rrows.push(row);
                }
                None => rows.push(row),
            }
        }
        let mut manifest = json!({"v": 1, "seq": seq, "date": "2026-10-14T08:00:00Z", "notes": notes,
            "variant": "esp8266-clawd", "layouts": 1, "styleApi": style_api,
            "commit": format!("c{seq:06}"), "files": rows});
        if !rrows.is_empty() {
            manifest["renderers"] = Value::Array(rrows);
        }
        for (name, blob) in &blobs {
            let st = self.state.clone();
            let (n, blob) = (name.clone(), blob.clone());
            self.server.route_fn(
                "GET",
                &format!("/v1/file/{seq}/{name}"),
                Arc::new(move |req: &RecordedRequest| {
                    if !req.query().split('&').any(|kv| kv == "sig=ok") {
                        return FakeResponse::status(403);
                    }
                    let mut b = blob.clone();
                    if st.lock().expect("state").tamper.contains(&n) {
                        *b.last_mut().expect("blob") ^= 1;
                    }
                    FakeResponse::bytes(200, b)
                }),
            );
        }
        s.channels.insert(channel.to_owned(), seq);
        s.releases.insert(
            seq,
            Release {
                manifest: manifest.clone(),
                key,
                blobs,
            },
        );
        manifest
    }

    /// `publish` with the default files and options (`feedkit.publish(seq)`).
    pub fn publish_default(&self, seq: u64) -> Value {
        self.publish(
            seq,
            release_files(&["synth", "paper"], 1),
            "stable",
            "",
            1,
            &[],
        )
    }

    /// How often the file `name` was downloaded (`FakeFeed.got`).
    pub fn got(&self, name: &str) -> usize {
        self.server
            .requests()
            .iter()
            .filter(|r| {
                r.method == "GET"
                    && r.path().starts_with("/v1/file/")
                    && r.path().ends_with(&format!("/{name}"))
            })
            .count()
    }

    /// Every file download so far, as `GET /v1/file/<seq>/<name>`.
    pub fn file_calls(&self) -> Vec<String> {
        self.server
            .requests()
            .iter()
            .filter(|r| r.path().starts_with("/v1/file/"))
            .map(|r| format!("{} {}", r.method, r.path()))
            .collect()
    }

    pub fn grants(&self) -> usize {
        self.server.requests_to("/v1/grant").len()
    }
}
