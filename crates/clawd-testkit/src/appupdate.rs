//! Fakes for `clawd-appupdate` tests (phase 10 owns this file): fake GitHub API with redirects,
//! fake `CommandRunner`, fake detached launcher.
//!
//! RULE (PORT-MAP.md): these implement traits of `clawd-appupdate`, so they may only be used
//! from `crates/clawd-appupdate/tests/*.rs`, never from `#[cfg(test)]` modules inside the crate.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_appupdate::{Body, Fut, LaunchSpec, Launcher, Reply, Transport};
use clawd_config::{CommandOutput, CommandRunner};
use clawd_contracts::UpdateError;

/// One scripted answer of [`FakeGitHub`] (`httpx.Response` in the Python tests).
#[derive(Debug, Clone)]
pub struct FakeReply {
    pub status: u16,
    pub location: Option<String>,
    pub body: Vec<u8>,
}

impl FakeReply {
    /// `200` with `body`.
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            location: None,
            body: body.into(),
        }
    }

    /// Any status, empty body.
    pub fn status(status: u16) -> Self {
        Self {
            status,
            location: None,
            body: Vec::new(),
        }
    }

    /// `302` to `location`.
    pub fn redirect(location: &str) -> Self {
        Self {
            status: 302,
            location: Some(location.to_owned()),
            body: Vec::new(),
        }
    }

    /// `200` with a JSON body.
    pub fn json(value: &serde_json::Value) -> Self {
        Self::ok(serde_json::to_vec(value).expect("json"))
    }
}

/// Decides the answer for a URL not in the route table; `None` = 404.
pub type RouteFn = Arc<dyn Fn(&str) -> Option<FakeReply> + Send + Sync>;

/// `api.github.com` + the asset CDN in memory: a [`Transport`] answering from a route table
/// keyed by the full URL (like `httpx.MockTransport`). Nothing leaves the process.
pub struct FakeGitHub {
    routes: Mutex<HashMap<String, FakeReply>>,
    fallback: Mutex<Option<RouteFn>>,
    calls: Mutex<Vec<String>>,
    chunk: Mutex<usize>,
    unreachable: Mutex<Option<String>>,
}

impl FakeGitHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            routes: Mutex::new(HashMap::new()),
            fallback: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
            chunk: Mutex::new(64 * 1024),
            unreachable: Mutex::new(None),
        })
    }

    /// Answer `url` (exactly) with `reply`.
    pub fn route(&self, url: &str, reply: FakeReply) -> &Self {
        self.routes
            .lock()
            .expect("routes")
            .insert(url.to_owned(), reply);
        self
    }

    /// Answer every URL without a route by calling `handler`.
    pub fn fallback(&self, handler: RouteFn) -> &Self {
        *self.fallback.lock().expect("fallback") = Some(handler);
        self
    }

    /// Serve bodies in pieces of `bytes` (progress and size-limit tests).
    pub fn chunk_size(&self, bytes: usize) -> &Self {
        *self.chunk.lock().expect("chunk") = bytes.max(1);
        self
    }

    /// Every request fails with `message` (`ConnectError: ...`), like an offline machine.
    pub fn go_offline(&self, message: &str) -> &Self {
        *self.unreachable.lock().expect("unreachable") = Some(message.to_owned());
        self
    }

    /// Every URL asked so far, oldest first.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("calls").clone()
    }

    /// Forget the recorded calls.
    pub fn clear_calls(&self) {
        self.calls.lock().expect("calls").clear();
    }
}

struct FakeBody {
    rest: Vec<u8>,
    chunk: usize,
}

impl Body for FakeBody {
    fn chunk(&mut self) -> Fut<'_, Result<Option<Vec<u8>>, UpdateError>> {
        Box::pin(async move {
            if self.rest.is_empty() {
                return Ok(None);
            }
            let n = self.chunk.min(self.rest.len());
            let piece: Vec<u8> = self.rest.drain(..n).collect();
            Ok(Some(piece))
        })
    }
}

impl Transport for FakeGitHub {
    fn get(&self, url: &str, _timeout: Duration) -> Fut<'_, Result<Reply, UpdateError>> {
        let url = url.to_owned();
        Box::pin(async move {
            self.calls.lock().expect("calls").push(url.clone());
            if let Some(message) = self.unreachable.lock().expect("unreachable").clone() {
                return Err(UpdateError(format!("ConnectError: {message}")));
            }
            let routed = self.routes.lock().expect("routes").get(&url).cloned();
            let fallback = self.fallback.lock().expect("fallback").clone();
            let reply = routed
                .or_else(|| fallback.and_then(|f| f(&url)))
                .unwrap_or_else(|| FakeReply::status(404));
            let chunk = *self.chunk.lock().expect("chunk");
            Ok(Reply {
                status: reply.status,
                location: reply.location,
                body: Box::new(FakeBody {
                    rest: reply.body,
                    chunk,
                }),
            })
        })
    }
}

/// Decides what a fake program prints: `(program, args) -> output`.
pub type RunFn = Box<dyn Fn(&str, &[&str]) -> io::Result<CommandOutput> + Send + Sync>;

/// A [`CommandRunner`] that never starts a process: it records every call and answers from a
/// closure (`codesign`, `ditto` on every OS).
pub struct FakeRunner {
    handler: RunFn,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

impl FakeRunner {
    pub fn new(
        handler: impl Fn(&str, &[&str]) -> io::Result<CommandOutput> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            handler: Box::new(handler),
            calls: Mutex::new(Vec::new()),
        })
    }

    /// Every program asked to run, with its arguments, oldest first.
    pub fn calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls.lock().expect("calls").clone()
    }
}

/// An empty successful output.
pub fn output_ok() -> CommandOutput {
    CommandOutput {
        status: 0,
        stdout: String::new(),
        stderr: String::new(),
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str], _timeout: Duration) -> io::Result<CommandOutput> {
        self.calls.lock().expect("calls").push((
            program.to_owned(),
            args.iter().map(|a| (*a).to_owned()).collect(),
        ));
        (self.handler)(program, args)
    }
}

/// A [`Launcher`] that starts nothing: it records what would have been started.
#[derive(Default)]
pub struct FakeLauncher {
    started: Mutex<Vec<LaunchSpec>>,
    fail: Mutex<Option<io::ErrorKind>>,
}

impl FakeLauncher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Every program that would have been started, oldest first.
    pub fn started(&self) -> Vec<LaunchSpec> {
        self.started.lock().expect("started").clone()
    }

    /// The next starts fail with this error kind (`PermissionDenied`...).
    pub fn fail_with(&self, kind: io::ErrorKind) {
        *self.fail.lock().expect("fail") = Some(kind);
    }
}

impl Launcher for FakeLauncher {
    fn spawn_detached(&self, spec: &LaunchSpec) -> io::Result<()> {
        if let Some(kind) = *self.fail.lock().expect("fail") {
            return Err(io::Error::new(kind, "fake launcher refused"));
        }
        self.started.lock().expect("started").push(spec.clone());
        Ok(())
    }
}
