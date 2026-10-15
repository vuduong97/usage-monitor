//! `clawd-clock`: talk to the Clawd desk clock and find it on the LAN (phase 08 implements it).
//!
//! STUB (phase 02): real signatures and docs, `todo!()` bodies. Python origin: `clockapi.py`,
//! `discovery.py`, `bootwatch.py` (see `desktop-rs/PORT-MAP.md`). Errors are
//! [`clawd_contracts::ClockError`] / [`clawd_contracts::AuthNeeded`]; the clock's web password
//! comes from `clawd_store::creds`. Only mDNS multicast (UDP 5353) and plain HTTP to the clock
//! touch the network; there is no port scan.
#![allow(
    unused_variables,
    dead_code,
    clippy::new_without_default,
    clippy::too_many_arguments,
    clippy::type_complexity
)] // stub crate: the owning phase removes this when it implements the bodies

pub mod api;
pub mod bootwatch;
pub mod clock;
pub mod diff;
pub mod finder;
pub mod mdns;
pub mod store;

pub use api::ClockApi;
pub use bootwatch::{BootWatch, OnRestart};
pub use clock::{manual_clock, valid_host};
pub use diff::config_diff;
pub use finder::{ClockFinder, FinderSnapshot};
pub use mdns::{MdnsBrowser, MdnsSource};
pub use store::{stored_clock, ClockStore};

/// `discovery.Clock` is [`clawd_contracts::ClockTarget`] (shared with the app and settings).
pub use clawd_contracts::ClockTarget as Clock;
