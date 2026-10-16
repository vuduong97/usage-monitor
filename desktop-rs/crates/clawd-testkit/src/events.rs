//! Fakes for `clawd-events` tests (phase 04 owns this file): a fake normalize fn for the event
//! server and a fake clock (device) for the `StatePusher`.
//!
//! Per the testkit rule, tests of `clawd-events` itself use these from `tests/*.rs` only.

use std::io;
use std::sync::{Arc, Mutex};

use clawd_events::{claude_event, pi_event, NormalizeFn};
use serde_json::{json, Value};

use crate::core::{FakeHttpServer, FakeResponse, RecordedRequest};

/// What the fake normalizer saw for each `/hook/<agent>?e=<hint>` request: `(agent, hint)`.
pub type SeenHooks = Arc<Mutex<Vec<(String, String)>>>;

/// A stand-in for `clawd_agents::normalize`: `claude` payloads go through
/// [`clawd_events::claude_event`], `pi` payloads through [`clawd_events::pi_event`] (one event or
/// none, both are `Some(vec)`), every other agent id is unknown (`None`, the server answers 404).
/// The second value records each call.
pub fn fake_normalize() -> (NormalizeFn, SeenHooks) {
    let seen: SeenHooks = Arc::default();
    let record = Arc::clone(&seen);
    let normalize: NormalizeFn = Arc::new(move |agent: &str, payload: &Value, hint: &str| {
        record
            .lock()
            .expect("seen hooks")
            .push((agent.to_owned(), hint.to_owned()));
        match agent {
            "claude" => Some(claude_event(payload).into_iter().collect()),
            "pi" => Some(pi_event(payload).into_iter().collect()),
            _ => None,
        }
    });
    (normalize, seen)
}

/// How the fake clock answers.
#[derive(Debug, Clone)]
struct ClockBehaviour {
    state_status: u16,
    /// `desk` in `/api/heap`; `None` answers 503.
    desk: Option<i64>,
}

/// A fake Clawd clock: accepts `POST /api/state` and answers `GET /api/heap`, recording what it
/// got. [`FakeClockServer::push_url`] is what `TargetsFn` returns for it.
pub struct FakeClockServer {
    http: FakeHttpServer,
    behaviour: Arc<Mutex<ClockBehaviour>>,
}

impl FakeClockServer {
    pub fn start() -> io::Result<Self> {
        let http = FakeHttpServer::start()?;
        let behaviour = Arc::new(Mutex::new(ClockBehaviour {
            state_status: 200,
            desk: Some(31),
        }));
        let state = Arc::clone(&behaviour);
        http.route_fn(
            "POST",
            "/api/state",
            Arc::new(move |_req: &RecordedRequest| {
                FakeResponse::status(state.lock().expect("clock behaviour").state_status)
            }),
        );
        let heap = Arc::clone(&behaviour);
        http.route_fn(
            "GET",
            "/api/heap",
            Arc::new(move |_req: &RecordedRequest| {
                match heap.lock().expect("clock behaviour").desk {
                    Some(desk) => FakeResponse::json(200, &json!({"desk": desk, "heap": 12345})),
                    None => FakeResponse::status(503),
                }
            }),
        );
        Ok(Self { http, behaviour })
    }

    /// The clock's `/api/usage` URL (a `TargetsFn` entry).
    pub fn push_url(&self) -> String {
        format!("{}/api/usage", self.http.url())
    }

    /// Status `POST /api/state` answers (200 by default).
    pub fn set_state_status(&self, status: u16) {
        self.behaviour.lock().expect("clock behaviour").state_status = status;
    }

    /// The `desk` count `/api/heap` reports; `None` makes it fail with 503.
    pub fn set_desk(&self, desk: Option<i64>) {
        self.behaviour.lock().expect("clock behaviour").desk = desk;
    }

    /// Every `POST /api/state` body received so far, parsed.
    pub fn state_bodies(&self) -> Vec<Value> {
        self.http
            .requests_to("/api/state")
            .iter()
            .filter_map(RecordedRequest::body_json)
            .collect()
    }

    /// Every request received so far.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.http.requests()
    }
}
