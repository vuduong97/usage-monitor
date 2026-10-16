//! `events.EventServer` (`events.py:1037-1124`): HTTP/1.1 on `127.0.0.1` for the hooks, the pi
//! extension and the `clawd` CLI. Hyper 1 directly (no axum); loopback only.
//!
//! Routes (frozen): `GET /state` -> `{desired, sessions, turns, away, focus_left}`;
//! `POST /poke`; `POST /hook/<agent>?e=<event>`. A request with an `Origin` header is 403 and a
//! body over [`clawd_contracts::MAX_BODY`] is 413 (also when chunked). Additions approved for
//! the port: `Host` must be `127.0.0.1:<port>`/`localhost:<port>` (DNS rebinding), header-read
//! timeout and a connection cap.

use std::convert::Infallible;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use clawd_contracts::{Clock, SystemClock, EVENT_HOST, EVENT_PORT, MAX_BODY};
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE, HOST, ORIGIN};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{watch, Semaphore};
use tokio::task::{JoinHandle, JoinSet};

use crate::brain::SharedBrain;
use crate::pyjson::to_py_json;
use crate::source_events::char_prefix;

/// `agents.normalize(agent_id, payload, hint)` as the server sees it: tracker events for one
/// hook payload, `None` for an agent id nobody knows (the server answers 404). Defined here so
/// this crate does not depend on `clawd-agents`; the app passes `clawd_agents::normalize`
/// wrapped in an `Arc`, tests pass a fake.
pub type NormalizeFn = Arc<dyn Fn(&str, &Value, &str) -> Option<Vec<Value>> + Send + Sync>;

/// Time and connection limits (red team #13). The defaults are the approved ones; tests shorten
/// them so a slow client is cut in a second, not ten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// A client must send its request head within this long.
    pub header_read_timeout: Duration,
    /// A whole connection (head, body, answer) lives at most this long.
    pub connection_timeout: Duration,
    /// Connections served at once; the next one waits in the OS backlog until a slot frees.
    pub max_connections: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            header_read_timeout: Duration::from_secs(10),
            connection_timeout: Duration::from_secs(10),
            max_connections: 64,
        }
    }
}

/// What a request handler needs, shared by every connection.
struct Context {
    brain: SharedBrain,
    normalize: NormalizeFn,
    clock: Arc<dyn Clock>,
    port: u16,
}

struct Running {
    shutdown: watch::Sender<bool>,
    accept_task: JoinHandle<()>,
}

/// `events.EventServer`.
pub struct EventServer {
    brain: SharedBrain,
    normalize: NormalizeFn,
    clock: Arc<dyn Clock>,
    port: u16,
    limits: Limits,
    running: Option<Running>,
}

impl EventServer {
    /// `EventServer(brain, ...)` bound to `127.0.0.1:EVENT_PORT` once started.
    pub fn new(brain: SharedBrain, normalize: NormalizeFn) -> Self {
        Self {
            brain,
            normalize,
            clock: Arc::new(SystemClock),
            port: EVENT_PORT,
            limits: Limits::default(),
            running: None,
        }
    }

    /// Use another port (`read_event_port`'s answer; `0` = any free port, for tests).
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Use another clock (tests).
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Use other limits (tests).
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// `EventServer.start()`: bind and serve in the background; after it returns [`Self::port`]
    /// is the real port. `Err` when the port is taken (the app then disables agent events).
    pub async fn start(&mut self) -> io::Result<()> {
        if self.running.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "event server already started",
            ));
        }
        debug_assert_eq!(EVENT_HOST, "127.0.0.1");
        let listener =
            TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))).await?;
        self.port = listener.local_addr()?.port();
        let context = Arc::new(Context {
            brain: Arc::clone(&self.brain),
            normalize: Arc::clone(&self.normalize),
            clock: Arc::clone(&self.clock),
            port: self.port,
        });
        let (shutdown, stop) = watch::channel(false);
        let accept_task = tokio::spawn(accept_loop(listener, context, self.limits, stop));
        self.running = Some(Running {
            shutdown,
            accept_task,
        });
        tracing::info!("Event server on http://{EVENT_HOST}:{}", self.port);
        Ok(())
    }

    /// The port in use (after [`Self::start`] it is the bound one, even for port 0).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// `EventServer.close()`: stop accepting and drop the connections.
    pub async fn close(&mut self) {
        if let Some(running) = self.running.take() {
            let _ = running.shutdown.send(true);
            let _ = running.accept_task.await;
        }
    }
}

impl Drop for EventServer {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            let _ = running.shutdown.send(true);
            running.accept_task.abort();
        }
    }
}

/// Accept connections until `stop`; the connection tasks live in a `JoinSet`, so leaving the
/// loop drops (aborts) every open one.
async fn accept_loop(
    listener: TcpListener,
    context: Arc<Context>,
    limits: Limits,
    mut stop: watch::Receiver<bool>,
) {
    let slots = Arc::new(Semaphore::new(limits.max_connections.max(1)));
    let mut connections = JoinSet::new();
    loop {
        // The slot is taken before accepting: past the cap, new clients wait in the OS backlog
        // instead of piling up as tasks and open sockets.
        let permit = tokio::select! {
            _ = crate::stop_requested(&mut stop) => break,
            permit = Arc::clone(&slots).acquire_owned() => match permit {
                Ok(permit) => permit,
                Err(_) => break,
            },
        };
        let stream = tokio::select! {
            _ = crate::stop_requested(&mut stop) => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                Err(e) => {
                    tracing::warn!("event server accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
        };
        let context = Arc::clone(&context);
        connections.spawn(async move {
            serve_connection(stream, context, limits).await;
            drop(permit);
        });
        while connections.try_join_next().is_some() {}
    }
}

async fn serve_connection(stream: tokio::net::TcpStream, context: Arc<Context>, limits: Limits) {
    let service = service_fn(move |request| {
        let context = Arc::clone(&context);
        async move { Ok::<_, Infallible>(handle(&context, request).await) }
    });
    let connection = hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(limits.header_read_timeout)
        .keep_alive(false)
        .serve_connection(TokioIo::new(stream), service);
    // Slow bodies and answers are cut by the same overall limit as slow heads.
    let _ = tokio::time::timeout(limits.connection_timeout, connection).await;
}

/// A JSON answer in Python's `json.dumps` form.
fn reply(status: u16, body: &Value) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(to_py_json(body))));
    *response.status_mut() =
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response.headers_mut().insert(
        CONTENT_TYPE,
        "application/json".parse().expect("static header"),
    );
    response
}

fn host_allowed(request: &Request<Incoming>, port: u16) -> bool {
    match request.headers().get(HOST) {
        // HTTP/1.0 clients may omit Host; browsers (the DNS-rebinding threat) always send it.
        None => true,
        Some(value) => value.to_str().is_ok_and(|host| {
            let host = host.to_ascii_lowercase();
            host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
        }),
    }
}

async fn handle(context: &Context, request: Request<Incoming>) -> Response<Full<Bytes>> {
    if request.headers().contains_key(ORIGIN) {
        return reply(
            403,
            &json!({"ok": false, "error": "browser requests refused"}),
        );
    }
    if !host_allowed(&request, context.port) {
        return reply(403, &json!({"ok": false, "error": "host not allowed"}));
    }
    let declared = request
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    if declared.is_some_and(|n| n > MAX_BODY as u64) {
        return reply(413, &json!({"ok": false}));
    }
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or("").to_owned();
    // Always read the body, chunked or not, under the same cap (a lying or absent length).
    let raw = match Limited::new(request.into_body(), MAX_BODY).collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e)
            if e.downcast_ref::<http_body_util::LengthLimitError>()
                .is_some() =>
        {
            return reply(413, &json!({"ok": false}));
        }
        Err(_) => return reply(400, &json!({"ok": false})),
    };
    let now = context.clock.now();
    if method == Method::GET && path == "/state" {
        let body = context
            .brain
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .state_body(now);
        return reply(200, &body);
    }
    if method != Method::POST {
        return reply(404, &json!({"ok": false}));
    }
    let payload: Value = if raw.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&raw) {
            Ok(value) => value,
            Err(_) => return reply(400, &json!({"ok": false, "error": "bad json"})),
        }
    };
    if path == "/poke" {
        let empty = json!({});
        let cmd = if payload.is_object() {
            &payload
        } else {
            &empty
        };
        let (answer, wake) = {
            let mut brain = context.brain.lock().unwrap_or_else(|e| e.into_inner());
            (brain.pokes.handle(cmd, now), Arc::clone(&brain.wake))
        };
        let ok = answer.get("ok") == Some(&Value::Bool(true));
        if ok {
            wake.notify_one();
        }
        return reply(if ok { 200 } else { 400 }, &answer);
    }
    let Some(agent) = path.strip_prefix("/hook/") else {
        return reply(404, &json!({"ok": false}));
    };
    let hint = char_prefix(&query_param(&query, "e"), 40);
    let Some(events) = (context.normalize)(agent, &payload, &hint) else {
        return reply(404, &json!({"ok": false}));
    };
    let (changed, wake) = {
        let mut brain = context.brain.lock().unwrap_or_else(|e| e.into_inner());
        let mut changed = false;
        for event in &events {
            changed = brain.ingest(event, now) || changed;
        }
        (changed, Arc::clone(&brain.wake))
    };
    if changed {
        wake.notify_one();
    }
    reply(200, &json!({"ok": true}))
}

/// `urllib.parse.parse_qs(query).get(name, [""])[0]`: first non-blank value, percent-decoded.
fn query_param(query: &str, name: &str) -> String {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if value.is_empty() || percent_decode(key) != name {
            continue;
        }
        return percent_decode(value);
    }
    String::new()
}

/// `urllib.parse.unquote_plus` (bad escapes stay, bad UTF-8 becomes U+FFFD).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len()
                && hex(bytes[i + 1]).is_some()
                && hex(bytes[i + 2]).is_some() =>
            {
                out.push(hex(bytes[i + 1]).unwrap_or(0) * 16 + hex(bytes[i + 2]).unwrap_or(0));
                i += 2;
            }
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|d| d as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_param_matches_parse_qs() {
        assert_eq!(query_param("e=Stop", "e"), "Stop");
        assert_eq!(query_param("x=1&e=a%20b+c", "e"), "a b c");
        assert_eq!(query_param("e=&e=second", "e"), "second");
        assert_eq!(query_param("e", "e"), "");
        assert_eq!(query_param("", "e"), "");
        assert_eq!(query_param("e=%zz%4", "e"), "%zz%4");
        assert_eq!(query_param("e=%41%2", "e"), "A%2");
    }
}
