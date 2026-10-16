//! `clawd-clock`: talk to the Clawd desk clock and find it on the LAN (phase 08 implements it).
//!
//! Python origin: `clockapi.py`,
//! `discovery.py`, `bootwatch.py` (see `desktop-rs/PORT-MAP.md`). Errors are
//! [`clawd_contracts::ClockError`] / [`clawd_contracts::AuthNeeded`]; the clock's web password
//! comes from `clawd_store::creds`. Only mDNS multicast (UDP 5353) and plain HTTP to the clock
//! touch the network; there is no port scan.

pub mod api;
pub mod bootwatch;
pub mod clock;
pub mod diff;
pub mod finder;
pub mod mdns;
mod pyval;
pub mod store;

pub use api::{ClockApi, ClockApiOptions};
pub use bootwatch::{BootWatch, OnRestart};
pub use clock::{manual_clock, valid_host};
pub use diff::config_diff;
pub use finder::{ClockFinder, FinderSnapshot};
pub use mdns::{MdnsBrowser, MdnsSource};
pub use store::{stored_clock, ClockStore};
