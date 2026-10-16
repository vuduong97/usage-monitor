//! Phase-02 test support shared by every crate.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use clawd_contracts::{AppPaths, Clock, OsStoreNames};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---- golden fixtures ---------------------------------------------------------------

/// `<repo>/fixtures`, found from this crate's manifest dir so it works from any crate.
pub fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
}

/// `fixtures/<area>/<name>.json` (areas: store events agents config usage clock fw
/// appupdate settings).
pub fn fixture_path(area: &str, name: &str) -> PathBuf {
    fixtures_root().join(area).join(format!("{name}.json"))
}

/// What `python-0.1.x:tools/golden/_harness.py` writes: the exporter's `input` and the real Python's `output`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fixture<I, O> {
    pub input: I,
    pub output: O,
}

/// Load a golden fixture. Panics with the path on a missing or malformed file (test support).
pub fn load_fixture<I: DeserializeOwned, O: DeserializeOwned>(
    area: &str,
    name: &str,
) -> Fixture<I, O> {
    let path = fixture_path(area, name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("bad fixture {}: {e}", path.display()))
}

/// The fixture as untyped JSON (`{"input": ..., "output": ...}`).
pub fn load_fixture_value(area: &str, name: &str) -> Value {
    load_fixture::<Value, Value>(area, name).into_value()
}

impl Fixture<Value, Value> {
    fn into_value(self) -> Value {
        serde_json::json!({"input": self.input, "output": self.output})
    }
}

// ---- temp config dir ---------------------------------------------------------------

static UNIQUE: AtomicU64 = AtomicU64::new(0);

/// A throwaway home + config dir. Nothing under it is shared with the real machine: tests build
/// every path from [`Self::paths`] and every OS-store name from [`Self::os_names`].
pub struct TempConfig {
    dir: tempfile::TempDir,
    paths: AppPaths,
    names: OsStoreNames,
}

impl TempConfig {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::new(dir.path().join("home"));
        let n = UNIQUE.fetch_add(1, Ordering::Relaxed);
        let names = OsStoreNames::for_test(&format!("{}-{n}", std::process::id()));
        Self { dir, paths, names }
    }

    /// The injected [`AppPaths`] (`<temp>/home/.config/claude-usage-monitor/...`).
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// Unique, never-production OS-store names for this instance.
    pub fn os_names(&self) -> &OsStoreNames {
        &self.names
    }

    pub fn home(&self) -> &Path {
        self.paths.home()
    }

    /// The temp root (parent of `home`), for tests that need scratch space of their own.
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Write `contents` to `rel` under the home dir (parents created); returns the full path.
    pub fn write_home(&self, rel: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.home().join(rel);
        std::fs::create_dir_all(path.parent().expect("has parent")).expect("mkdir");
        std::fs::write(&path, contents).expect("write");
        path
    }

    /// Write `contents` to `name` inside the config dir; returns the full path.
    pub fn write_config(&self, name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.paths.config_dir().join(name);
        std::fs::create_dir_all(self.paths.config_dir()).expect("mkdir");
        std::fs::write(&path, contents).expect("write");
        path
    }
}

impl Default for TempConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ---- fake clock --------------------------------------------------------------------

/// A [`Clock`] driven by hand. Clones share the same time.
#[derive(Debug, Clone, Default)]
pub struct FakeClock {
    now: Arc<Mutex<f64>>,
}

impl FakeClock {
    pub fn new(start: f64) -> Self {
        Self {
            now: Arc::new(Mutex::new(start)),
        }
    }

    pub fn set(&self, t: f64) {
        *self.now.lock().expect("clock lock") = t;
    }

    pub fn advance(&self, secs: f64) {
        *self.now.lock().expect("clock lock") += secs;
    }

    /// An `Arc<dyn Clock>` sharing this clock's time.
    pub fn arc(&self) -> Arc<dyn Clock> {
        Arc::new(self.clone())
    }
}

impl Clock for FakeClock {
    fn now(&self) -> f64 {
        *self.now.lock().expect("clock lock")
    }
}

// ---- fake HTTP server --------------------------------------------------------------

/// One request the fake server received (header names lower-cased).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    pub method: String,
    /// Request target as sent: path plus `?query`.
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RecordedRequest {
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    pub fn query(&self) -> &str {
        self.target.split_once('?').map_or("", |(_, q)| q)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn body_json(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }
}

/// What a route answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Wait this long before answering (client timeout tests: slow ESP8266, slowloris).
    pub delay: Duration,
    /// Close the connection without any answer (connection reset / no answer tests).
    pub drop_connection: bool,
    /// Send the body with `Transfer-Encoding: chunked` (two chunks) instead of `Content-Length`.
    pub chunked: bool,
}

impl FakeResponse {
    fn new(status: u16, headers: Vec<(String, String)>, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
            delay: Duration::ZERO,
            drop_connection: false,
            chunked: false,
        }
    }

    /// Answer after `delay`.
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Accept the request, then close the connection with no answer.
    pub fn dropped() -> Self {
        let mut r = Self::new(0, Vec::new(), Vec::new());
        r.drop_connection = true;
        r
    }

    /// Use chunked transfer encoding for the body.
    pub fn chunked(mut self) -> Self {
        self.chunked = true;
        self
    }

    pub fn status(status: u16) -> Self {
        Self::new(status, Vec::new(), Vec::new())
    }

    pub fn text(status: u16, body: &str) -> Self {
        Self::new(
            status,
            vec![("Content-Type".into(), "text/plain".into())],
            body.as_bytes().to_vec(),
        )
    }

    pub fn json(status: u16, body: &Value) -> Self {
        Self::new(
            status,
            vec![("Content-Type".into(), "application/json".into())],
            serde_json::to_vec(body).expect("json"),
        )
    }

    pub fn bytes(status: u16, body: Vec<u8>) -> Self {
        Self::new(status, Vec::new(), body)
    }

    /// Add a response header (`Location` for a redirect, `WWW-Authenticate` for a challenge).
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

/// Decides the answer for one request (receives it after it is recorded).
pub type Handler = Arc<dyn Fn(&RecordedRequest) -> FakeResponse + Send + Sync>;

struct Route {
    method: Option<String>,
    path: String,
    handler: Handler,
}

struct ServerState {
    routes: Mutex<Vec<Route>>,
    requests: Mutex<Vec<RecordedRequest>>,
    stop: AtomicBool,
}

/// A tiny HTTP/1.1 server on `127.0.0.1:<free port>` for tests: routes map (method, path) to
/// a response or handler, every request is recorded. One connection per request
/// (`Connection: close`); bodies need a `Content-Length` (no chunked uploads). The first matching
/// route wins; no match answers 404. Stops when dropped.
pub struct FakeHttpServer {
    addr: SocketAddr,
    state: Arc<ServerState>,
    thread: Option<JoinHandle<()>>,
}

impl FakeHttpServer {
    pub fn start() -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let addr = listener.local_addr()?;
        let state = Arc::new(ServerState {
            routes: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
        });
        let shared = Arc::clone(&state);
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if shared.stop.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(stream) = stream {
                    let shared = Arc::clone(&shared);
                    std::thread::spawn(move || {
                        let _ = serve(stream, &shared);
                    });
                }
            }
        });
        Ok(Self {
            addr,
            state,
            thread: Some(thread),
        })
    }

    /// `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Answer `method path` (path without the query) with a fixed response.
    pub fn route(&self, method: &str, path: &str, response: FakeResponse) -> &Self {
        self.route_fn(method, path, Arc::new(move |_| response.clone()))
    }

    /// Answer `method path` by calling `handler` (stateful fakes: digest challenge, redirects).
    /// An empty `method` matches any method.
    pub fn route_fn(&self, method: &str, path: &str, handler: Handler) -> &Self {
        self.state.routes.lock().expect("routes").push(Route {
            method: (!method.is_empty()).then(|| method.to_ascii_uppercase()),
            path: path.to_owned(),
            handler,
        });
        self
    }

    /// Everything received so far, oldest first.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.state.requests.lock().expect("requests").clone()
    }

    /// The recorded requests for one path.
    pub fn requests_to(&self, path: &str) -> Vec<RecordedRequest> {
        self.requests()
            .into_iter()
            .filter(|r| r.path() == path)
            .collect()
    }
}

impl Drop for FakeHttpServer {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(500));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn serve(stream: TcpStream, state: &ServerState) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(());
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();
    let mut headers = Vec::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
        }
    }
    let len = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; len];
    reader.read_exact(&mut body)?;
    let request = RecordedRequest {
        method,
        target,
        headers,
        body,
    };
    state
        .requests
        .lock()
        .expect("requests")
        .push(request.clone());

    let handler = state
        .routes
        .lock()
        .expect("routes")
        .iter()
        .find(|r| {
            r.path == request.path() && r.method.as_deref().is_none_or(|m| m == request.method)
        })
        .map(|r| Arc::clone(&r.handler));
    let response = match handler {
        Some(h) => h(&request),
        None => FakeResponse::text(404, "not found"),
    };

    if response.drop_connection {
        let _ = stream.shutdown(Shutdown::Both);
        return Ok(());
    }
    if !response.delay.is_zero() {
        std::thread::sleep(response.delay);
    }
    let mut out = stream;
    write!(
        out,
        "HTTP/1.1 {} {}\r\n",
        response.status,
        reason(response.status)
    )?;
    for (k, v) in &response.headers {
        write!(out, "{k}: {v}\r\n")?;
    }
    if response.chunked {
        write!(
            out,
            "Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
        )?;
        let (a, b) = response.body.split_at(response.body.len() / 2);
        for part in [a, b] {
            if !part.is_empty() {
                write!(out, "{:x}\r\n", part.len())?;
                out.write_all(part)?;
                write!(out, "\r\n")?;
            }
        }
        write!(out, "0\r\n\r\n")?;
    } else {
        write!(
            out,
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            response.body.len()
        )?;
        out.write_all(&response.body)?;
    }
    out.flush()?;
    // Write side only: closing both sides can make Windows answer with an RST that discards
    // the tail of the response the client has not read yet (a flaky truncated body).
    let _ = out.shutdown(Shutdown::Write);
    Ok(())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(server: &FakeHttpServer, raw: &str) -> String {
        let mut s = TcpStream::connect(server.addr()).unwrap();
        s.write_all(raw.as_bytes()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    #[test]
    fn routes_answer_and_requests_are_recorded() {
        let server = FakeHttpServer::start().unwrap();
        server.route(
            "GET",
            "/api/heap",
            FakeResponse::json(200, &json!({"uptime": 5})),
        );
        let body = r#"{"a":1}"#;
        let got = request(&server, "GET /api/heap?x=1 HTTP/1.1\r\nHost: t\r\n\r\n");
        assert!(got.starts_with("HTTP/1.1 200 OK"), "{got}");
        assert!(got.ends_with(r#"{"uptime":5}"#), "{got}");
        let missing = request(
            &server,
            &format!(
                "POST /nope HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        );
        assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
        let seen = server.requests();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].path(), "/api/heap");
        assert_eq!(seen[0].query(), "x=1");
        assert_eq!(seen[1].body_json(), Some(json!({"a": 1})));
    }

    #[test]
    fn delay_chunked_and_dropped_responses() {
        let server = FakeHttpServer::start().unwrap();
        server.route(
            "GET",
            "/slow",
            FakeResponse::text(200, "hello").with_delay(Duration::from_millis(150)),
        );
        server.route("GET", "/chunk", FakeResponse::text(200, "abcdef").chunked());
        server.route("GET", "/drop", FakeResponse::dropped());
        let t = std::time::Instant::now();
        let slow = request(&server, "GET /slow HTTP/1.1\r\nHost: t\r\n\r\n");
        assert!(t.elapsed() >= Duration::from_millis(150));
        assert!(slow.ends_with("hello"), "{slow}");
        let chunk = request(&server, "GET /chunk HTTP/1.1\r\nHost: t\r\n\r\n");
        assert!(chunk.contains("Transfer-Encoding: chunked"), "{chunk}");
        // The decoded body is what matters. Under load something on the loopback path (an AV
        // filter) sometimes re-chunks the stream as one `6` chunk, which is still valid.
        let framed = chunk.split("\r\n\r\n").nth(1).unwrap_or("");
        let mut decoded = String::new();
        let mut rest = framed;
        while let Some((size, tail)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            decoded.push_str(&tail[..n]);
            rest = tail[n..].trim_start_matches("\r\n");
        }
        assert_eq!(decoded, "abcdef", "{chunk}");
        assert_eq!(
            request(&server, "GET /drop HTTP/1.1\r\nHost: t\r\n\r\n"),
            ""
        );
    }

    #[test]
    fn fake_clock_is_shared_between_clones() {
        let c = FakeClock::new(10.0);
        let arc = c.arc();
        c.advance(2.5);
        assert_eq!(arc.now(), 12.5);
    }

    #[test]
    fn temp_config_never_uses_production_names_and_stays_in_its_dir() {
        let t = TempConfig::new();
        assert!(!t.os_names().is_production());
        assert!(t.paths().prefs_file().starts_with(t.root()));
        let p = t.write_config("config", "a = b\n");
        assert!(p.exists());
    }

    #[test]
    fn sample_golden_fixture_loads() {
        let f: Fixture<Value, Value> = load_fixture("store", "prefs_roundtrip");
        assert_eq!(f.output["clock_screen"], 1);
        assert_eq!(f.input["initial"]["usage"]["screen"], 300);
        let v = load_fixture_value("store", "prefs_roundtrip");
        assert_eq!(v["output"]["prefs"]["ratio"], 1.0);
    }
}
