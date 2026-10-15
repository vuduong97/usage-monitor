//! `clawd-testkit`: support code for the tests of every crate (dev-dependency only).
//!
//! * [`core`] (phase 02, frozen API): fixture loader, [`TempConfig`], [`FakeHttpServer`],
//!   [`FakeClock`].
//! * One module per area, empty at phase 02; each is owned by the phase named in its doc and
//!   holds that area's fakes (plan.md file-ownership rule 2).

pub mod core;

pub mod agents;
pub mod appupdate;
pub mod clock;
pub mod config;
pub mod events;
pub mod fw;
pub mod store;
pub mod usage;

pub use crate::core::*;
