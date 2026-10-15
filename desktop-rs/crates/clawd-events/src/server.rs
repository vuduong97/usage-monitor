//! `events.EventServer` (`events.py:1037-1124`): HTTP/1.1 on `127.0.0.1` for the hooks, the pi
//! extension and the `clawd` CLI. Hyper 1 directly (no axum); loopback only.
//!
//! Routes (frozen): `GET /state` -> `{desired, sessions, turns, away, focus_left}`;
//! `POST /poke`; `POST /hook/<agent>?e=<event>`. A request with an `Origin` header is 403 and a
//! body over [`clawd_contracts::MAX_BODY`] is 413 (also when chunked). Additions approved for
//! the port: `Host` must be `127.0.0.1:<port>`/`localhost:<port>` (DNS rebinding), header-read
//! timeout and a connection cap.

use std::io;
use std::sync::Arc;

use clawd_contracts::Clock;
use serde_json::Value;

use crate::brain::SharedBrain;

/// `agents.normalize(agent_id, payload, hint)` as the server sees it: tracker events for one
/// hook payload, `None` for an agent id nobody knows (the server answers 404). Defined here so
/// this crate does not depend on `clawd-agents`; the app passes `clawd_agents::normalize`
/// wrapped in an `Arc`, tests pass a fake.
pub type NormalizeFn = Arc<dyn Fn(&str, &Value, &str) -> Option<Vec<Value>> + Send + Sync>;

/// `events.EventServer`.
pub struct EventServer {
    _private: (),
}

impl EventServer {
    /// `EventServer(brain, ...)` bound to `127.0.0.1:EVENT_PORT` once started.
    pub fn new(brain: SharedBrain, normalize: NormalizeFn) -> Self {
        todo!("phase 04")
    }

    /// Use another port (`read_event_port`'s answer; `0` = any free port, for tests).
    pub fn with_port(self, port: u16) -> Self {
        todo!("phase 04")
    }

    /// Use another clock (tests).
    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Self {
        todo!("phase 04")
    }

    /// `EventServer.start()`: bind and serve in the background; after it returns [`Self::port`]
    /// is the real port. `Err` when the port is taken (the app then disables agent events).
    pub async fn start(&mut self) -> io::Result<()> {
        todo!("phase 04")
    }

    /// The port in use (after [`Self::start`] it is the bound one, even for port 0).
    pub fn port(&self) -> u16 {
        todo!("phase 04")
    }

    /// `EventServer.close()`: stop accepting and drop the connections.
    pub async fn close(&mut self) {
        todo!("phase 04")
    }
}
