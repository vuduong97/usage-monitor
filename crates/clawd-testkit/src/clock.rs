//! Fakes for `clawd-clock` tests (phase 08): a fake clock that speaks the firmware's HTTP API
//! with an optional Digest login ([`FakeClockDevice`], [`DigestGate`]), and a fake mDNS source
//! ([`FakeMdns`]).
//!
//! Per the testkit rule these fakes of `clawd-clock`'s own trait ([`MdnsSource`]) are used from
//! `crates/clawd-clock/tests/*.rs` only.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_clock::MdnsSource;
use clawd_contracts::ClockTarget;
use serde_json::{json, Value};

use crate::core::{FakeHttpServer, FakeResponse, RecordedRequest};

// ---- MD5 (RFC 1321): the Digest login of the firmware uses it ------------------------

/// MD5 of `data` as lower-case hex.
pub fn md5_hex(data: &[u8]) -> String {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let table: Vec<u32> = (0..64)
        .map(|i| (((i + 1) as f64).sin().abs() * 4_294_967_296.0) as u32)
        .collect();
    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_le_bytes());
    for block in message.chunks(64) {
        let words: Vec<u32> = block
            .chunks(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let sum = a
                .wrapping_add(f)
                .wrapping_add(table[i])
                .wrapping_add(words[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(sum.rotate_left(SHIFTS[i]));
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
    state
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ---- Digest challenge ---------------------------------------------------------------

/// The `response` of an RFC 2617 Digest answer with `qop=auth`.
#[allow(clippy::too_many_arguments)]
pub fn digest_response(
    user: &str,
    realm: &str,
    password: &str,
    method: &str,
    uri: &str,
    nonce: &str,
    nc: &str,
    cnonce: &str,
) -> String {
    let ha1 = md5_hex(format!("{user}:{realm}:{password}").as_bytes());
    let ha2 = md5_hex(format!("{method}:{uri}").as_bytes());
    md5_hex(format!("{ha1}:{nonce}:{nc}:{cnonce}:auth:{ha2}").as_bytes())
}

/// `key="value", key2=value2` pairs of an `Authorization: Digest ...` header.
fn digest_fields(header: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let Some(rest) = header.strip_prefix("Digest ") else {
        return fields;
    };
    let mut chars = rest.chars().peekable();
    loop {
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' {
                break;
            }
            chars.next();
            if c != ',' && !c.is_whitespace() {
                key.push(c);
            }
        }
        if chars.next().is_none() {
            break;
        }
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                value.push(c);
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ',' {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }
        fields.insert(key, value);
    }
    fields
}

/// A Digest login (`qop=auth`, MD5) in front of a fake clock: [`Self::authorized`] checks the
/// answer's hash, [`Self::challenge`] makes the `401`.
pub struct DigestGate {
    user: String,
    password: String,
    realm: String,
    nonce: String,
    challenges: AtomicUsize,
}

impl DigestGate {
    /// Realm `clawd`, nonce `n0nce`, like the firmware.
    pub fn new(user: &str, password: &str) -> Self {
        Self {
            user: user.into(),
            password: password.into(),
            realm: "clawd".into(),
            nonce: "n0nce".into(),
            challenges: AtomicUsize::new(0),
        }
    }

    /// Does `request` carry a correct Digest answer for its own method and target?
    pub fn authorized(&self, request: &RecordedRequest) -> bool {
        let Some(header) = request.header("authorization") else {
            return false;
        };
        let f = digest_fields(header);
        let get = |k: &str| f.get(k).map(String::as_str).unwrap_or("");
        get("username") == self.user
            && get("realm") == self.realm
            && get("nonce") == self.nonce
            && get("uri") == request.target
            && get("qop") == "auth"
            && get("response")
                == digest_response(
                    &self.user,
                    &self.realm,
                    &self.password,
                    &request.method,
                    &request.target,
                    &self.nonce,
                    get("nc"),
                    get("cnonce"),
                )
    }

    /// The `401` with a `WWW-Authenticate` challenge (counted).
    pub fn challenge(&self) -> FakeResponse {
        self.challenges.fetch_add(1, Ordering::SeqCst);
        FakeResponse::text(401, "login").with_header(
            "WWW-Authenticate",
            &format!(
                "Digest realm=\"{}\", nonce=\"{}\", qop=\"auth\", algorithm=MD5, opaque=\"op\"",
                self.realm, self.nonce
            ),
        )
    }

    /// How many `401` challenges were sent.
    pub fn challenges(&self) -> usize {
        self.challenges.load(Ordering::SeqCst)
    }
}

// ---- the fake clock -----------------------------------------------------------------

/// One request the fake clock saw (`FakeClock.calls` in `tests/test_clockapi.py`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCall {
    pub method: String,
    pub path: String,
    /// The request carried an `Authorization: Digest` header.
    pub signed: bool,
}

struct DeviceState {
    gate: Option<DigestGate>,
    calls: Mutex<Vec<DeviceCall>>,
    config: Mutex<Value>,
    saved: Mutex<Vec<Value>>,
    /// `None`: `/api/heap` closes the connection.
    uptime: Mutex<Option<u64>>,
    /// The next N requests are answered only after `down_delay` (a clock that is too slow).
    down: AtomicUsize,
    down_delay: Mutex<Duration>,
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    overrides: Mutex<BTreeMap<String, FakeResponse>>,
    pack_answer: Mutex<Value>,
}

/// A fake clock firmware on `127.0.0.1:<port>`: the same routes and answers as the Python
/// tests' `FakeClock`. `password` turns Digest login (user `admin`) on for everything but
/// `/api/heap`.
pub struct FakeClockDevice {
    server: FakeHttpServer,
    state: Arc<DeviceState>,
}

impl FakeClockDevice {
    pub fn start(password: Option<&str>) -> Self {
        let state = Arc::new(DeviceState {
            gate: password.map(|p| DigestGate::new("admin", p)),
            calls: Mutex::new(Vec::new()),
            config: Mutex::new(json!({
                "brightness": 45,
                "usage": {"screen": 0, "layout": 0},
                "wifi": [{"ssid": "Home", "passSet": true}],
            })),
            saved: Mutex::new(Vec::new()),
            uptime: Mutex::new(Some(900)),
            down: AtomicUsize::new(0),
            down_delay: Mutex::new(Duration::from_millis(800)),
            files: Mutex::new(BTreeMap::from([(
                "/clawd.pak".to_owned(),
                [b"PAK0".as_slice(), &[0u8; 96]].concat(),
            )])),
            overrides: Mutex::new(BTreeMap::new()),
            pack_answer: Mutex::new(json!({"ok": true, "anims": 12, "crc": 1234})),
        });
        let server = FakeHttpServer::start().expect("fake clock server");
        for path in [
            "/api/heap",
            "/api/config",
            "/update",
            "/api/status",
            "/api/scan",
            "/api/reboot",
            "/api/factory",
            "/api/import",
            "/api/export",
            "/api/style",
            "/api/fs/get",
            "/api/clawdpak",
            "/api/deskpak",
        ] {
            let st = Arc::clone(&state);
            server.route_fn("", path, Arc::new(move |req| st.answer(req)));
        }
        Self { server, state }
    }

    /// `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        self.server.url()
    }

    /// The `/api/usage` URL of this clock.
    pub fn usage_url(&self) -> String {
        format!("{}/api/usage", self.server.url())
    }

    /// The underlying server (raw recorded requests, extra routes for other paths).
    pub fn server(&self) -> &FakeHttpServer {
        &self.server
    }

    /// `/api/heap`'s uptime; `None` makes the clock stop answering there.
    pub fn set_uptime(&self, uptime: Option<u64>) {
        *self.state.uptime.lock().expect("uptime") = uptime;
    }

    /// The next `n` requests are answered `delay` late (a slow clock: client timeouts).
    pub fn set_down(&self, n: usize, delay: Duration) {
        *self.state.down_delay.lock().expect("delay") = delay;
        self.state.down.store(n, Ordering::SeqCst);
    }

    /// A file served by `GET /api/fs/get?path=`.
    pub fn set_file(&self, path: &str, bytes: &[u8]) {
        self.state
            .files
            .lock()
            .expect("files")
            .insert(path.into(), bytes.to_vec());
    }

    /// Replace the answer of one path for every method (checked after the login).
    pub fn override_path(&self, path: &str, response: FakeResponse) {
        self.state
            .overrides
            .lock()
            .expect("overrides")
            .insert(path.into(), response);
    }

    /// The JSON the pack uploads answer.
    pub fn set_pack_answer(&self, answer: Value) {
        *self.state.pack_answer.lock().expect("pack") = answer;
    }

    /// Every request in arrival order.
    pub fn calls(&self) -> Vec<DeviceCall> {
        self.state.calls.lock().expect("calls").clone()
    }

    /// The bodies of `POST /api/config`.
    pub fn saved(&self) -> Vec<Value> {
        self.state.saved.lock().expect("saved").clone()
    }

    /// How many Digest challenges were sent (a preemptive client sends one per session).
    pub fn challenges(&self) -> usize {
        self.state.gate.as_ref().map_or(0, DigestGate::challenges)
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

impl DeviceState {
    fn answer(&self, req: &RecordedRequest) -> FakeResponse {
        let signed = req
            .header("authorization")
            .is_some_and(|h| h.starts_with("Digest "));
        self.calls.lock().expect("calls").push(DeviceCall {
            method: req.method.clone(),
            path: req.path().to_owned(),
            signed,
        });
        // Take one from `down` if it is not 0 (`fetch_update` is deprecated after 1.91, and its
        // replacement `try_update` is newer than the workspace's rust-version).
        let mut left = self.down.load(Ordering::SeqCst);
        let slow = loop {
            if left == 0 {
                break false;
            }
            match self
                .down
                .compare_exchange(left, left - 1, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => break true,
                Err(now) => left = now,
            }
        };
        if slow {
            let delay = *self.down_delay.lock().expect("delay");
            return FakeResponse::text(200, "late").with_delay(delay);
        }
        let path = req.path();
        if path == "/api/heap" {
            if let Some(over) = self.overrides.lock().expect("overrides").get(path) {
                return over.clone();
            }
            return match *self.uptime.lock().expect("uptime") {
                Some(up) => FakeResponse::json(
                    200,
                    &json!({"uptime": up, "fwSeq": 1, "layouts": 3, "version": "2.16.0"}),
                ),
                None => FakeResponse::dropped(),
            };
        }
        if let Some(gate) = &self.gate {
            if !gate.authorized(req) {
                return gate.challenge();
            }
        }
        if let Some(over) = self.overrides.lock().expect("overrides").get(path) {
            return over.clone();
        }
        let method = req.method.as_str();
        match (method, path) {
            ("GET", "/api/config") => {
                FakeResponse::json(200, &self.config.lock().expect("config").clone())
            }
            ("POST", "/api/config") => {
                let body = req.body_json().unwrap_or(Value::Null);
                let reboot = body.get("wifi").is_some();
                self.saved.lock().expect("saved").push(body);
                FakeResponse::json(200, &json!({"ok": true, "reboot": reboot}))
            }
            ("POST", "/update") => {
                if contains(&req.body, b"name=\"firmware\"") {
                    FakeResponse::text(200, "OK")
                } else {
                    FakeResponse::text(400, "no firmware part")
                }
            }
            ("GET", "/api/status") => FakeResponse::json(
                200,
                &json!({"mode": "sta", "connected": true, "ssid": "Home", "rssi": -50}),
            ),
            ("GET", "/api/scan") => {
                FakeResponse::json(200, &json!([{"ssid": "Home", "rssi": -50, "enc": true}]))
            }
            ("POST", "/api/reboot" | "/api/factory" | "/api/import") => {
                FakeResponse::json(200, &json!({"ok": true}))
            }
            ("GET", "/api/export") => {
                FakeResponse::json(200, &json!({"wifi": [{"ssid": "Home", "pass": "x"}]}))
            }
            ("GET", "/api/style") => {
                FakeResponse::json(200, &json!({"api": 1, "active": false, "slot": 32768}))
            }
            ("POST", "/api/style") => {
                if !contains(&req.body, b"name=\"file\"") {
                    FakeResponse::text(400, "no file part")
                } else if !contains(&req.body, b"CSTY") {
                    FakeResponse::json(
                        400,
                        &json!({"ok": false, "error": "magic", "style": {"active": false}}),
                    )
                } else {
                    FakeResponse::json(
                        200,
                        &json!({"ok": true, "style": {"active": true, "id": "synth"}}),
                    )
                }
            }
            ("DELETE", "/api/style") => {
                FakeResponse::json(200, &json!({"ok": true, "style": {"active": false}}))
            }
            ("GET", "/api/fs/get") => {
                let wanted = req
                    .query()
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("path="))
                    .map(|p| p.replace("%2F", "/").replace("%2f", "/"));
                let file = wanted.and_then(|p| self.files.lock().expect("files").get(&p).cloned());
                match file {
                    Some(bytes) => FakeResponse::bytes(200, bytes),
                    None => FakeResponse::status(404),
                }
            }
            ("POST", "/api/clawdpak" | "/api/deskpak") => {
                if contains(&req.body, b"name=\"file\"") {
                    FakeResponse::json(200, &self.pack_answer.lock().expect("pack").clone())
                } else {
                    FakeResponse::text(400, "no file part")
                }
            }
            _ => FakeResponse::status(404),
        }
    }
}

// ---- the fake mDNS source -----------------------------------------------------------

#[derive(Default)]
struct MdnsState {
    clocks: BTreeMap<String, ClockTarget>,
    changed: u64,
    starts: usize,
    stops: usize,
    resolved: Vec<String>,
    refuse_start: bool,
}

/// A hand-driven `_clawd._tcp` browser (`FakeBrowser` of `tests/test_discovery.py`): instance
/// names are `<clock name>._clawd._tcp.local.`.
pub struct FakeMdns {
    state: Mutex<MdnsState>,
}

impl FakeMdns {
    pub fn new(clocks: &[ClockTarget]) -> Self {
        let fake = Self {
            state: Mutex::new(MdnsState::default()),
        };
        fake.install(clocks, false);
        fake
    }

    fn install(&self, clocks: &[ClockTarget], bump: bool) {
        let mut st = self.state.lock().expect("mdns");
        st.clocks = clocks
            .iter()
            .map(|c| (format!("{}._clawd._tcp.local.", c.name), c.clone()))
            .collect();
        if bump {
            st.changed += 1;
        }
    }

    /// The clocks mDNS sees from now on (bumps `changed`, like an add/update/remove).
    pub fn set(&self, clocks: &[ClockTarget]) {
        self.install(clocks, true);
    }

    /// `start()` answers false (no multicast-capable interface).
    pub fn refuse_start(&self) {
        self.state.lock().expect("mdns").refuse_start = true;
    }

    pub fn start_count(&self) -> usize {
        self.state.lock().expect("mdns").starts
    }

    pub fn stop_count(&self) -> usize {
        self.state.lock().expect("mdns").stops
    }

    /// The instance names `resolve` was asked for, in order.
    pub fn resolve_calls(&self) -> Vec<String> {
        self.state.lock().expect("mdns").resolved.clone()
    }
}

impl MdnsSource for FakeMdns {
    fn start(&self) -> bool {
        let mut st = self.state.lock().expect("mdns");
        st.starts += 1;
        !st.refuse_start
    }

    fn names(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("mdns")
            .clocks
            .keys()
            .cloned()
            .collect()
    }

    fn changed(&self) -> u64 {
        self.state.lock().expect("mdns").changed
    }

    fn resolve(&self, name: &str) -> Option<ClockTarget> {
        let mut st = self.state.lock().expect("mdns");
        st.resolved.push(name.to_owned());
        st.clocks.get(name).cloned()
    }

    fn stop(&self) {
        self.state.lock().expect("mdns").stops += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_rfc_1321_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            ),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn digest_response_matches_the_rfc_2617_example() {
        let response = digest_response(
            "Mufasa",
            "testrealm@host.com",
            "Circle Of Life",
            "GET",
            "/dir/index.html",
            "dcd98b7102dd2f0e8b11d0f600bfb0c093",
            "00000001",
            "0a4f113b",
        );
        assert_eq!(response, "6629fae49393a05397450978507c4ef1");
    }

    #[test]
    fn digest_fields_parse_quoted_and_bare_values() {
        let f = digest_fields("Digest username=\"admin\", nc=00000001, uri=\"/a?b=c,d\"");
        assert_eq!(f["username"], "admin");
        assert_eq!(f["nc"], "00000001");
        assert_eq!(f["uri"], "/a?b=c,d");
    }
}
