//! Shared builders of the shell tests (no window, no real crate behind them).
#![allow(dead_code)]

use clawd_app::engine::Status;
use clawd_contracts::{ClockSource, ClockTarget, UsagePayload};
use serde_json::{json, Value};

pub const NOW: f64 = 1_800_000_000.0;

/// `A = Clock(host="10.0.0.7", name="clawd")`.
pub fn clock_a() -> ClockTarget {
    let mut c = ClockTarget::new("10.0.0.7", ClockSource::Saved);
    c.name = "clawd".to_owned();
    c
}

pub fn payload(v: Value) -> UsagePayload {
    serde_json::from_value(v).expect("payload")
}

/// The `snap()` helper of `test_tray.py`: a known clock is not assumed; `token` is `true`.
pub fn snap() -> Status {
    Status {
        token: Some(true),
        ..Status::default()
    }
}

/// `test_popover.snap()`: connected, 10 % / 41 %.
pub fn popover_snap() -> Status {
    Status {
        clock: Some(clock_a()),
        last_ok: Some(NOW - 5.0),
        token: Some(true),
        payload: Some(payload(
            json!({"s": 10, "sr": 105, "w": 41, "wr": 6965, "acct": "pro", "ok": true}),
        )),
        payload_at: Some(NOW),
        ..Status::default()
    }
}
