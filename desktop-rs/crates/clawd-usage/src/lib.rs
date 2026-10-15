//! `clawd-usage`: poll Claude/Codex usage and push it to the clocks (phase 07 implements it).
//!
//! STUB (phase 02): real signatures and docs, `todo!()` bodies. Python origin: `usage.py`
//! (from `poll_api` on) and `codex_usage.py` (see `desktop-rs/PORT-MAP.md`). The "read the
//! sources" half of `usage.py` (config file, tokens, targets, CLIProxy settings) is
//! `clawd-config`.
//!
//! **BLE is dropped (owner decision Q3):** `BleakError`, `Session`, `discover_target`,
//! `retrieve_connected_macos`, `connect_and_run`, `ble_loop`, `unpair_macos`, `_blueutil`,
//! `load_cached_address`, `main`, `Publisher.attach_ble/detach_ble` and their constants
//! (`DEVICE_NAME`, `*_UUID`, `CONNECT_TIMEOUT`, `SAVED_ADDR_FILE`, `BLUEUTIL_TIMEOUT`) have no
//! Rust counterpart. `usage.start_events` is wiring, done by the app engine (phase 12).
//!
//! Every poll/publish loop must survive a failing cycle (log, go on): the app also wraps each
//! task in `catch_unwind` (red team #12).
#![allow(
    unused_variables,
    dead_code,
    clippy::new_without_default,
    clippy::too_many_arguments,
    clippy::type_complexity
)] // stub crate: the owning phase removes this when it implements the bodies

pub mod cliproxy;
pub mod codex;
pub mod oauth;
pub mod payload;
pub mod publisher;
pub mod selector;
pub mod sinks;

use std::sync::Arc;

use clawd_config::CommandRunner;
use clawd_contracts::AppPaths;

pub use cliproxy::CliProxySource;
pub use oauth::{poll_api, poll_token, poll_usage_endpoint, usage_from_oauth_usage, PollError};
pub use payload::{
    add_chime_field, add_clock_fields, encode_payload, take_attention_flag, take_clears, to_ascii,
    Attention,
};
pub use publisher::{PayloadHook, Publisher};
pub use selector::{
    poll_active, poll_active_payload, poll_claude, PlanKey, PlanSelector, PollState,
};
pub use sinks::{HttpSinks, ResultHook};

/// What the poller needs from its surroundings: where the config lives, and how to run
/// `security`/`defaults`. Replaces the module globals (`CONFIG_FILE`, ...) of `usage.py`.
#[derive(Clone)]
pub struct UsageEnv {
    pub paths: AppPaths,
    pub runner: Arc<dyn CommandRunner>,
}
