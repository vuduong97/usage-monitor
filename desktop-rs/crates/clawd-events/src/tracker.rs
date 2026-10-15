//! Per-session agent state resolved into one desk animation (`events.py:232-486`).
//! Crate-internal in practice (only the Brain and the server use it); public for golden tests.

use std::collections::HashMap;

use clawd_contracts::View;
use serde_json::Value;

/// `events.Moment`: a timed animation queued by the pet or a poke.
#[derive(Debug, Clone, PartialEq)]
pub struct Moment {
    pub anim: String,
    pub text: String,
    pub secs: f64,
    /// 0 until first played.
    pub start: f64,
    pub seq: Vec<String>,
    pub urgent: bool,
}

/// `events.Sess`: one agent session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sess {
    pub sid: String,
    pub src: String,
    /// A child agent process (e.g. `pi -p` run by a subagent).
    pub sub: bool,
    /// `idle | thinking | working | attention | waiting | sweeping`.
    pub state: String,
    /// Raw tool name, for `clawd status`.
    pub tool: String,
    /// Its `ACTION_TEXT` key, for the clock.
    pub action: String,
    pub msg: String,
    /// The attention is a question tool (react left/right).
    pub asking: bool,
    pub ask_text: String,
    pub subs: u32,
    pub last: f64,
    pub error_until: f64,
    pub error_text: String,
    pub done_until: f64,
    pub since: f64,
    pub turn_start: f64,
    pub running: u32,
    pub quiet_since: f64,
    pub testing: bool,
    pub debugging: bool,
    pub debug_next: bool,
    pub edits: u32,
}

/// Called with `now` when a turn starts (the Pet counts turns per day).
pub type OnTurn = Box<dyn FnMut(f64) + Send>;

/// `events.Tracker`.
pub struct Tracker {
    pub sessions: HashMap<String, Sess>,
}

impl Tracker {
    /// `Tracker(on_turn)`.
    pub fn new(on_turn: Option<OnTurn>) -> Self {
        todo!("phase 04")
    }

    /// `Tracker.ingest(ev, now)`: apply one tracker event (see [`crate::EVENTS`]); true when it
    /// changed anything (the server then wakes the Brain).
    pub fn ingest(&mut self, ev: &Value, now: f64) -> bool {
        todo!("phase 04")
    }

    /// `Tracker.expire(now)`: stale attention/working sessions go idle, old idle ones are
    /// forgotten.
    pub fn expire(&mut self, now: f64) {
        todo!("phase 04")
    }

    /// `Tracker.view(now)`: all sessions resolved into one agent-layer view, if any.
    pub fn view(&mut self, now: f64) -> Option<View> {
        todo!("phase 04")
    }

    /// `Tracker.snapshot(now)`: the `sessions` array of `GET /state`
    /// (`sid src sub state tool subs running debug age`).
    pub fn snapshot(&self, now: f64) -> Vec<Value> {
        todo!("phase 04")
    }
}
