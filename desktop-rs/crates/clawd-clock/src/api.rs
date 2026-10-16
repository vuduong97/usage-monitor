//! The clock's settings API (`clockapi.py`): read and save settings (`/api/config`; a POST
//! carries only what changed), scan WiFi, reboot, factory reset, backup/restore, push a firmware
//! image (`/update`), install or clear the usage screen's style pack (`/api/style`).
//!
//! The web password is HTTP **Digest** auth (`diqwest`): one auth session lives as long as the
//! client, so after the first challenge every request (the 0.5 MB upload too) is signed up
//! front. Requests are serialised per client (the ESP8266 serves one client at a time).
//! Failure mapping: 401 -> `ClockError::AuthNeeded`; >= 400 -> `ClockError::Http` (body cut to
//! 200 chars); connect/timeout -> retries then `ClockError::NoAnswer`; a malformed host ->
//! `ClockError::Other`, never a panic.
//!
//! Uploads are multipart bodies built in memory (not a streamed `reqwest::multipart::Form`) so
//! that `diqwest` can clone and re-sign the request. Like Python, an upload first sends a small
//! `GET /api/config` to collect the digest challenge, so the big body is sent only once.
//!
//! No proxy is used (`no_proxy`): the clock is on the LAN, and a system proxy would hide it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_contracts::{Clock, ClockError};
use diqwest::{DigestAuthSession, WithDigestAuth};
use reqwest::{Method, Url};
use serde_json::{Map, Value};

use crate::pyval;

/// `clockapi.TIMEOUT`: the clock answers slowly for a few minutes after a boot.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// `clockapi.UPLOAD_TIMEOUT`.
pub const UPLOAD_TIMEOUT: Duration = Duration::from_secs(180);
/// `clockapi.RETRIES`: extra tries for a GET that timed out or could not connect.
pub const RETRIES: u32 = 2;
/// `clockapi.RETRY_GAP`.
pub const RETRY_GAP: Duration = Duration::from_millis(1500);
/// `clockapi.OTA_MAX`: the clawd -> clawd OTA budget.
pub const OTA_MAX: usize = 524_288;
/// `clockapi.STYLE_MAX`: the largest style pack the firmware takes.
pub const STYLE_MAX: usize = 32_768;
/// `clockapi.PACK_MAX`: `/desk.pak` and `/clawd.pak`: what the filesystem can hold.
pub const PACK_MAX: usize = 3_000_000;
/// `clockapi.PACK_TIMEOUT`: about 2 MB over the clock's WiFi.
pub const PACK_TIMEOUT: Duration = Duration::from_secs(600);
/// The gap between two polls of [`ClockApi::wait_back`] (`self._sleep(2.0)`).
pub const POLL_GAP: Duration = Duration::from_secs(2);

/// `clockapi.PACK_PATHS`.
const PACK_PATHS: [(&str, &str); 2] = [("clawdpak", "/api/clawdpak"), ("deskpak", "/api/deskpak")];
const SCAN_TIMEOUT: Duration = Duration::from_secs(30);
const FILE_TIMEOUT: Duration = Duration::from_secs(60);
const STYLE_TIMEOUT: Duration = Duration::from_secs(60);

/// What Python's `ClockApi(base, auth, transport, sleep)` lets a test inject.
#[derive(Clone)]
pub struct ClockApiOptions {
    /// `sleep(RETRY_GAP)` between the tries of a GET (default [`RETRY_GAP`]).
    pub retry_gap: Duration,
    /// The sleep between two polls of `wait_back` (default [`POLL_GAP`]).
    pub poll_gap: Duration,
    /// Replaces the timeout of every request ([`TIMEOUT`], [`UPLOAD_TIMEOUT`], ... by default).
    pub timeout: Option<Duration>,
    /// The HTTP client (`transport`): redirects must stay off for Python parity. `None` builds
    /// one without proxy support.
    pub client: Option<reqwest::Client>,
}

impl Default for ClockApiOptions {
    fn default() -> Self {
        Self {
            retry_gap: RETRY_GAP,
            poll_gap: POLL_GAP,
            timeout: None,
            client: None,
        }
    }
}

/// `clockapi.ClockApi`. Responses stay untyped JSON (`serde_json::Value`): the firmware owns
/// their schema. Share it behind an `Arc`; every method takes `&self`.
pub struct ClockApi {
    base: String,
    http: reqwest::Client,
    auth: Mutex<Option<Arc<DigestAuthSession>>>,
    /// `ClockApi._lock`: one request at a time.
    lock: tokio::sync::Mutex<()>,
    options: ClockApiOptions,
}

/// The request body.
enum Payload<'a> {
    Empty,
    Json(&'a Value),
    /// A multipart form with one file part.
    File {
        field: &'a str,
        filename: &'a str,
        data: &'a [u8],
    },
}

/// One request of the client (`ClockApi._send` and its keyword arguments).
struct Call<'a> {
    method: Method,
    path: &'a str,
    query: Option<(&'a str, &'a str)>,
    retries: u32,
    timeout: Duration,
    payload: Payload<'a>,
}

impl<'a> Call<'a> {
    fn new(method: Method, path: &'a str) -> Self {
        Self {
            method,
            path,
            query: None,
            retries: 0,
            timeout: TIMEOUT,
            payload: Payload::Empty,
        }
    }

    fn get(path: &'a str) -> Self {
        Self::new(Method::GET, path)
    }

    fn retries(mut self, n: u32) -> Self {
        self.retries = n;
        self
    }

    fn timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    fn json(mut self, body: &'a Value) -> Self {
        self.payload = Payload::Json(body);
        self
    }

    fn file(mut self, field: &'a str, filename: &'a str, data: &'a [u8]) -> Self {
        self.payload = Payload::File {
            field,
            filename,
            data,
        };
        self
    }
}

/// A finished HTTP exchange with status < 400.
struct Reply {
    content_type: String,
    body: Vec<u8>,
}

impl Reply {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// How one attempt failed.
enum Fail {
    /// Connect error or timeout: worth another try (the `httpx.ConnectError`/`TimeoutException`
    /// branch). The text is the error kind for `no answer from ...`.
    Retry(String),
    Hard(ClockError),
}

fn error_chain(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        text.push_str(": ");
        text.push_str(&s.to_string());
        source = s.source();
    }
    text
}

fn classify(e: &reqwest::Error) -> Fail {
    if e.is_timeout() {
        let kind = if e.is_connect() {
            "ConnectTimeout"
        } else {
            "ReadTimeout"
        };
        Fail::Retry(kind.into())
    } else if e.is_connect() {
        Fail::Retry("ConnectError".into())
    } else {
        let kind = if e.is_body() || e.is_decode() {
            "ReadError"
        } else {
            "RequestError"
        };
        Fail::Hard(ClockError::Other(format!("{kind}: {}", error_chain(e))))
    }
}

fn classify_digest(e: diqwest::error::Error) -> Fail {
    match e {
        diqwest::error::Error::Reqwest(e) => classify(&e),
        other => Fail::Hard(ClockError::Other(format!(
            "DigestError: {}",
            error_chain(&other)
        ))),
    }
}

/// A multipart/form-data body with one file part (`files={field: (name, data, type)}`).
/// Returns `(content type, body)`.
fn multipart_file(field: &str, filename: &str, data: &[u8]) -> (String, Vec<u8>) {
    let mut n: u64 = 0;
    let boundary = loop {
        let candidate = format!("clawd-clock-{n:016x}");
        if !data
            .windows(candidate.len())
            .any(|w| w == candidate.as_bytes())
        {
            break candidate;
        }
        n += 1;
    };
    let mut body = Vec::with_capacity(data.len() + 256);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; \
             filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn object_or(v: Value, odd: &str) -> Result<Value, ClockError> {
    if v.is_object() {
        Ok(v)
    } else {
        Err(ClockError::Other(odd.into()))
    }
}

/// `d.get("style") or {}`.
fn style_of(d: &Value) -> Value {
    match d.get("style") {
        Some(s) if pyval::truthy(s) => s.clone(),
        _ => Value::Object(Map::new()),
    }
}

fn is_ok(d: &Value) -> bool {
    d.is_object() && d.get("ok").is_some_and(pyval::truthy)
}

impl ClockApi {
    /// `ClockApi(base, auth)`: `base` like `http://10.0.0.5` (a trailing `/` is dropped);
    /// `auth` is `(user, password)` for the Digest login.
    pub fn new(base: &str, auth: Option<(String, String)>) -> Self {
        Self::with_options(base, auth, ClockApiOptions::default())
    }

    /// `ClockApi(base, auth, transport, sleep)`: the same with the retry gap, timeouts, poll gap
    /// and HTTP client injected (tests use short ones and a local server).
    pub fn with_options(
        base: &str,
        auth: Option<(String, String)>,
        options: ClockApiOptions,
    ) -> Self {
        let http = options.client.clone().unwrap_or_else(|| {
            reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_default()
        });
        Self {
            base: base.trim_end_matches('/').to_owned(),
            http,
            auth: Mutex::new(Self::session(auth)),
            lock: tokio::sync::Mutex::new(()),
            options,
        }
    }

    fn session(auth: Option<(String, String)>) -> Option<Arc<DigestAuthSession>> {
        auth.map(|(user, password)| Arc::new(DigestAuthSession::new(user, password)))
    }

    /// The base URL.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// `ClockApi.set_auth(auth)`.
    pub fn set_auth(&self, auth: Option<(String, String)>) {
        *self.auth.lock().unwrap_or_else(|e| e.into_inner()) = Self::session(auth);
    }

    fn has_auth(&self) -> bool {
        self.auth.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    // ---- plumbing --------------------------------------------------------------------

    fn url(&self, call: &Call<'_>) -> Result<Url, ClockError> {
        let mut url = Url::parse(&format!("{}{}", self.base, call.path))
            .map_err(|e| ClockError::Other(format!("InvalidURL: {e}")))?;
        if let Some((key, value)) = call.query {
            url.query_pairs_mut().append_pair(key, value);
        }
        Ok(url)
    }

    /// One try: send, read the whole body, under the request lock.
    async fn attempt(&self, call: &Call<'_>, url: Url) -> Result<(u16, Reply), Fail> {
        let _one_at_a_time = self.lock.lock().await;
        let session = self
            .auth
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let timeout = self.options.timeout.unwrap_or(call.timeout);
        let mut request = self
            .http
            .request(call.method.clone(), url)
            .timeout(timeout);
        request = match &call.payload {
            Payload::Empty => request,
            Payload::Json(value) => request.json(value),
            Payload::File {
                field,
                filename,
                data,
            } => {
                let (content_type, body) = multipart_file(field, filename, data);
                request
                    .header(reqwest::header::CONTENT_TYPE, content_type)
                    .body(body)
            }
        };
        let response = match session {
            Some(session) => request
                .send_digest_auth(&*session)
                .await
                .map_err(classify_digest)?,
            None => request.send().await.map_err(|e| classify(&e))?,
        };
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let body = response.bytes().await.map_err(|e| classify(&e))?.to_vec();
        Ok((
            status,
            Reply {
                content_type,
                body,
            },
        ))
    }

    /// `ClockApi._send`.
    async fn send(&self, call: Call<'_>) -> Result<Reply, ClockError> {
        let url = self.url(&call)?;
        let mut last = String::new();
        for attempt in 0..=call.retries {
            if attempt > 0 {
                tokio::time::sleep(self.options.retry_gap).await;
            }
            match self.attempt(&call, url.clone()).await {
                Ok((401, _)) => return Err(ClockError::AuthNeeded),
                Ok((status, reply)) if status >= 400 => {
                    return Err(ClockError::Http {
                        status,
                        body: reply.text().chars().take(200).collect(),
                    })
                }
                Ok((_, reply)) => return Ok(reply),
                Err(Fail::Retry(kind)) => last = kind,
                Err(Fail::Hard(e)) => return Err(e),
            }
        }
        Err(ClockError::NoAnswer {
            base: self.base.clone(),
            kind: last,
        })
    }

    /// `ClockApi._json`.
    async fn json(&self, call: Call<'_>) -> Result<Value, ClockError> {
        let path = call.path.to_owned();
        let reply = self.send(call).await?;
        serde_json::from_slice(&reply.body)
            .map_err(|_| ClockError::Other(format!("not JSON from {path}")))
    }

    // ---- endpoints -------------------------------------------------------------------

    /// `GET /api/heap`: no login; uptime, version, fwSeq, layouts, packs. Object or
    /// `ClockError::Other("odd /api/heap answer")`.
    pub async fn heap(&self) -> Result<Value, ClockError> {
        let d = self
            .json(Call::get("/api/heap").retries(RETRIES))
            .await?;
        object_or(d, "odd /api/heap answer")
    }

    /// `GET /api/config` (login when set).
    pub async fn config(&self) -> Result<Value, ClockError> {
        let d = self
            .json(Call::get("/api/config").retries(RETRIES))
            .await?;
        object_or(d, "odd /api/config answer")
    }

    /// `POST /api/config` with only `changes`: `{"ok": true, "reboot": <WiFi changed>}`; empty
    /// changes return `{"ok": true, "reboot": false, "unchanged": true}` without a request.
    pub async fn save(&self, changes: &Value) -> Result<Value, ClockError> {
        if !pyval::truthy(changes) {
            return Ok(serde_json::json!({"ok": true, "reboot": false, "unchanged": true}));
        }
        let d = self
            .json(Call::new(Method::POST, "/api/config").json(changes))
            .await?;
        if !is_ok(&d) {
            return Err(ClockError::Other(format!("save refused: {}", pyval::repr(&d))));
        }
        Ok(d)
    }

    /// `GET /api/status` (login): mode, connected, ssid, ip, rssi, uptime...
    pub async fn status(&self) -> Result<Value, ClockError> {
        let d = self.json(Call::get("/api/status")).await?;
        object_or(d, "odd /api/status answer")
    }

    /// `GET /api/scan`: nearby WiFi `[{ssid, rssi, enc}]`; the clock blocks about 3 s scanning.
    pub async fn scan(&self) -> Result<Vec<Value>, ClockError> {
        let d = self
            .json(Call::get("/api/scan").retries(1).timeout(SCAN_TIMEOUT))
            .await?;
        Ok(match d {
            Value::Array(list) => list,
            _ => Vec::new(),
        })
    }

    /// `POST /api/reboot`.
    pub async fn reboot(&self) -> Result<(), ClockError> {
        self.send(Call::new(Method::POST, "/api/reboot")).await?;
        Ok(())
    }

    /// `POST /api/factory`.
    pub async fn factory_reset(&self) -> Result<(), ClockError> {
        self.send(Call::new(Method::POST, "/api/factory")).await?;
        Ok(())
    }

    /// `GET /api/export`: the saved `config.json`, WiFi passwords included (it is a backup).
    pub async fn export(&self) -> Result<Value, ClockError> {
        let d = self
            .json(Call::get("/api/export").retries(RETRIES))
            .await?;
        object_or(d, "odd backup")
    }

    /// `POST /api/import`: apply a backup; the clock reboots afterwards.
    pub async fn restore(&self, data: &Value) -> Result<(), ClockError> {
        self.send(Call::new(Method::POST, "/api/import").json(data))
            .await?;
        Ok(())
    }

    /// `GET /api/fs/get?path=`: one file from the clock's filesystem; `ClockError` when it is
    /// missing or bigger than `max_bytes`.
    pub async fn get_file(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>, ClockError> {
        let mut call = Call::get("/api/fs/get")
            .retries(RETRIES)
            .timeout(FILE_TIMEOUT);
        call.query = Some(("path", path));
        let reply = self.send(call).await?;
        if reply.body.len() > max_bytes {
            return Err(ClockError::Other(format!(
                "{path} is {} B, more than {max_bytes}",
                reply.body.len()
            )));
        }
        Ok(reply.body)
    }

    /// `POST /update` (multipart `firmware`): size must be 1..=[`OTA_MAX`]; collects the digest
    /// challenge with a small request first; the clock reboots about 1 s after the `OK`.
    pub async fn upload_firmware(&self, image: &[u8]) -> Result<(), ClockError> {
        if image.is_empty() || image.len() > OTA_MAX {
            return Err(ClockError::Other(format!(
                "firmware size {} B is outside 1..{OTA_MAX}",
                image.len()
            )));
        }
        if self.has_auth() {
            self.config().await?;
        }
        let reply = self
            .send(
                Call::new(Method::POST, "/update")
                    .timeout(UPLOAD_TIMEOUT)
                    .file("firmware", "firmware.bin", image),
            )
            .await?;
        let text = reply.text();
        if text.trim() != "OK" {
            return Err(ClockError::Other(format!(
                "update refused: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        Ok(())
    }

    /// `POST /api/clawdpak` (`kind = "clawdpak"`) or `/api/deskpak` (`"deskpak"`): replace
    /// `/clawd.pak` / `/desk.pak`; `{ok, anims, crc?}`. Size 1..=[`PACK_MAX`].
    pub async fn upload_pack(&self, kind: &str, data: &[u8]) -> Result<Value, ClockError> {
        let Some((_, path)) = PACK_PATHS.iter().find(|(k, _)| *k == kind) else {
            return Err(ClockError::Other(format!(
                "unknown pack {}",
                pyval::repr_str(kind)
            )));
        };
        if data.is_empty() || data.len() > PACK_MAX {
            return Err(ClockError::Other(format!(
                "pack size {} B is outside 1..{PACK_MAX}",
                data.len()
            )));
        }
        if self.has_auth() {
            self.config().await?;
        }
        let filename = kind.replace("pak", ".pak");
        let reply = self
            .send(
                Call::new(Method::POST, path)
                    .timeout(PACK_TIMEOUT)
                    .file("file", &filename, data),
            )
            .await?;
        let d: Value = serde_json::from_slice(&reply.body)
            .map_err(|_| ClockError::Other(format!("not JSON from {path}")))?;
        if !is_ok(&d) || !d.get("anims").is_some_and(pyval::truthy) {
            return Err(ClockError::Other(format!(
                "the clock refused {kind}: {}",
                pyval::repr(&d)
            )));
        }
        Ok(d)
    }

    /// `GET /api/style` (no login): `{api, active, slot, id?, name?, size?, crc?, error?}`.
    pub async fn style(&self) -> Result<Value, ClockError> {
        let d = self
            .json(Call::get("/api/style").retries(RETRIES))
            .await?;
        object_or(d, "odd /api/style answer")
    }

    /// `POST /api/style` (multipart `file`): send a style pack; the clock shows it at once.
    /// Size 1..=[`STYLE_MAX`]. Returns its `/api/style` view.
    pub async fn install_style(&self, pack: &[u8]) -> Result<Value, ClockError> {
        if pack.is_empty() || pack.len() > STYLE_MAX {
            return Err(ClockError::Other(format!(
                "style pack size {} B is outside 1..{STYLE_MAX}",
                pack.len()
            )));
        }
        if self.has_auth() {
            self.config().await?;
        }
        let refused = |e: ClockError| {
            ClockError::Other(format!("the clock refused the style pack ({e})"))
        };
        let reply = self
            .send(
                Call::new(Method::POST, "/api/style")
                    .timeout(STYLE_TIMEOUT)
                    .file("file", "style.pak", pack),
            )
            .await
            .map_err(refused)?;
        let d = if reply.content_type.starts_with("application/json") {
            serde_json::from_slice(&reply.body)
                .map_err(|_| ClockError::Other("not JSON from /api/style".into()))?
        } else {
            Value::Object(Map::new())
        };
        if !is_ok(&d) {
            return Err(ClockError::Other(format!(
                "the clock refused the style pack: {}",
                pyval::repr(&d)
            )));
        }
        Ok(style_of(&d))
    }

    /// `DELETE /api/style`: back to the classic screen.
    pub async fn clear_style(&self) -> Result<Value, ClockError> {
        let d = self
            .json(Call::new(Method::DELETE, "/api/style"))
            .await?;
        if !is_ok(&d) {
            return Err(ClockError::Other(format!(
                "clear refused: {}",
                pyval::repr(&d)
            )));
        }
        Ok(style_of(&d))
    }

    /// `ClockApi.wait_back(booted_before, deadline, clock, tick)`: poll `/api/heap` until the
    /// clock answers with an uptime that began after `booted_before` (a monotonic time from
    /// `clock`); returns that heap. `tick` runs every poll (the caller renews its push pause
    /// there). `deadline` default 150 s; `ClockError::Other` when the clock does not come back.
    ///
    /// Time: sleeps between polls are tokio sleeps and `clock` should be
    /// `MonotonicClock` (tokio `Instant`), so `tokio::time::pause()` drives the whole loop. A
    /// `FakeClock` is only for loops that do not sleep on tokio time.
    pub async fn wait_back(
        &self,
        booted_before: f64,
        deadline: Option<Duration>,
        clock: &dyn Clock,
        tick: Option<&(dyn Fn() + Send + Sync)>,
    ) -> Result<Value, ClockError> {
        let deadline = deadline.unwrap_or(Duration::from_secs(150));
        let end = clock.now() + deadline.as_secs_f64();
        while clock.now() < end {
            if let Some(tick) = tick {
                tick();
            }
            if let Ok(heap) = self.json(Call::get("/api/heap")).await {
                // a number that is not a bool; `clock() - up > booted_before`
                let up = heap.get("uptime").and_then(Value::as_f64);
                if up.is_some_and(|up| clock.now() - up > booted_before) && heap.is_object() {
                    return Ok(heap);
                }
            }
            tokio::time::sleep(self.options.poll_gap).await;
        }
        Err(ClockError::Other(
            "the clock did not come back after the update".into(),
        ))
    }
}
