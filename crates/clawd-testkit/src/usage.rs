//! Fakes for `clawd-usage` tests (phase 07 owns this file): a fake Anthropic usage server, a
//! fake CLIProxyAPI management API and a fake clock that records what the publisher pushes.
//! Use them from `tests/*.rs` integration tests only (PORT-MAP rule).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::core::{FakeHttpServer, FakeResponse};

/// `FakeHttpServer::start` with a few retries: binding a loopback port fails now and then on a
/// busy Windows host (seen once in a full-suite run).
fn start_server(what: &str) -> FakeHttpServer {
    let mut last = None;
    for _ in 0..5 {
        match FakeHttpServer::start() {
            Ok(server) => return server,
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    panic!("{what}: {:?}", last)
}

/// `tests/fixtures/oauth_usage_max.json` of the Python repo: a Max account's
/// `/api/oauth/usage` answer (7 % / 8 %).
pub fn oauth_usage_max() -> Value {
    serde_json::from_str(include_str!("../../../fixtures/usage/oauth_usage_max.json"))
        .expect("fixtures/usage/oauth_usage_max.json is valid JSON")
}

/// Fake `api.anthropic.com`: `GET /api/oauth/usage` and `POST /v1/messages`.
pub struct FakeAnthropic {
    pub server: FakeHttpServer,
}

impl FakeAnthropic {
    pub fn start() -> Self {
        Self {
            server: start_server("fake anthropic server"),
        }
    }

    /// `UsageEnv.usage_url`.
    pub fn usage_url(&self) -> String {
        format!("{}/api/oauth/usage", self.server.url())
    }

    /// `UsageEnv.api_url`.
    pub fn api_url(&self) -> String {
        format!("{}/v1/messages", self.server.url())
    }

    /// The usage endpoint answers 200 with `body`.
    pub fn usage_ok(&self, body: &Value) -> &Self {
        self.server
            .route("GET", "/api/oauth/usage", FakeResponse::json(200, body));
        self
    }

    /// The usage endpoint answers `status` with a text body.
    pub fn usage_status(&self, status: u16, text: &str) -> &Self {
        self.server
            .route("GET", "/api/oauth/usage", FakeResponse::text(status, text));
        self
    }

    /// The Haiku probe answers 200 with these `anthropic-ratelimit-unified-*` headers.
    pub fn probe_headers(&self, headers: &[(&str, &str)]) -> &Self {
        let mut response = FakeResponse::json(200, &json!({"id": "msg_test"}));
        for (name, value) in headers {
            response = response.with_header(name, value);
        }
        self.server.route("POST", "/v1/messages", response);
        self
    }

    /// The Haiku probe answers `status`.
    pub fn probe_status(&self, status: u16) -> &Self {
        self.server
            .route("POST", "/v1/messages", FakeResponse::status(status));
        self
    }
}

/// Fake clock `POST /api/usage`: records every payload, answers 200 (or `status`).
pub struct FakeClockSink {
    pub server: FakeHttpServer,
}

impl FakeClockSink {
    pub fn start() -> Self {
        Self::start_with_status(200)
    }

    pub fn start_with_status(status: u16) -> Self {
        let server = start_server("fake clock server");
        server.route(
            "POST",
            "/api/usage",
            FakeResponse::json(status, &json!({"ok": status == 200})),
        );
        Self { server }
    }

    /// The push URL (`http://127.0.0.1:<port>/api/usage`).
    pub fn url(&self) -> String {
        format!("{}/api/usage", self.server.url())
    }

    /// Every pushed payload, oldest first.
    pub fn payloads(&self) -> Vec<Value> {
        self.server
            .requests_to("/api/usage")
            .iter()
            .filter_map(|r| r.body_json())
            .collect()
    }

    /// Every pushed payload as the raw JSON text.
    pub fn raw_bodies(&self) -> Vec<String> {
        self.server
            .requests_to("/api/usage")
            .iter()
            .map(|r| String::from_utf8_lossy(&r.body).into_owned())
            .collect()
    }
}

/// Fake CLIProxyAPI management API (`auth-files` list + `api-call`), key-checked.
pub struct FakeCliProxy {
    pub server: FakeHttpServer,
    list_calls: Arc<Mutex<u32>>,
    api_calls: Arc<Mutex<Vec<Value>>>,
}

impl FakeCliProxy {
    /// `files` = the `auth-files` answer; `upstream(auth_index)` = `(status, body)` of the
    /// usage call the proxy makes for that account.
    pub fn start(
        key: &str,
        files: Value,
        upstream: impl Fn(&str) -> (u16, Value) + Send + Sync + 'static,
    ) -> Self {
        let server = start_server("fake cliproxy server");
        let list_calls = Arc::new(Mutex::new(0u32));
        let api_calls = Arc::new(Mutex::new(Vec::new()));
        let want = format!("Bearer {key}");
        {
            let (want, list_calls) = (want.clone(), Arc::clone(&list_calls));
            server.route_fn(
                "GET",
                "/v0/management/auth-files",
                Arc::new(move |req| {
                    if req.header("authorization") != Some(want.as_str()) {
                        return FakeResponse::json(401, &json!({"error": "bad key"}));
                    }
                    *list_calls.lock().expect("calls") += 1;
                    FakeResponse::json(200, &files)
                }),
            );
        }
        {
            let api_calls = Arc::clone(&api_calls);
            server.route_fn(
                "POST",
                "/v8/management/requests/api-call",
                Arc::new(move |req| {
                    if req.header("authorization") != Some(want.as_str()) {
                        return FakeResponse::json(401, &json!({"error": "bad key"}));
                    }
                    let call = req.body_json().unwrap_or(Value::Null);
                    let idx = call["authIndex"].as_str().unwrap_or("").to_owned();
                    api_calls.lock().expect("calls").push(call);
                    let (status, body) = upstream(&idx);
                    FakeResponse::json(
                        200,
                        &json!({"status_code": status, "header": {}, "body": body.to_string()}),
                    )
                }),
            );
        }
        Self {
            server,
            list_calls,
            api_calls,
        }
    }

    /// The proxy's base URL (`cliproxy_url`).
    pub fn url(&self) -> String {
        self.server.url()
    }

    /// How many times the account list was fetched.
    pub fn list_calls(&self) -> u32 {
        *self.list_calls.lock().expect("calls")
    }

    /// Every `api-call` request body the proxy received.
    pub fn api_calls(&self) -> Vec<Value> {
        self.api_calls.lock().expect("calls").clone()
    }
}
