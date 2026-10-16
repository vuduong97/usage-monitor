//! `clawd-contracts`: the types and constants every crate of the port agrees on.
//!
//! FROZEN (plan.md "Frozen contracts"): changes are *additive* and go through a Contract CR
//! in the phase report; the integrator batches them. Nothing here does I/O except
//! [`AppPaths::system`] reading two environment variables.
//!
//! Python source of truth (branch `python-0.1.x`): `python-0.1.x:clawd_desktop/{paths,pause,events,usage,feed,appupdate,discovery,
//! clockapi,autostart,creds}.py` (see `plans/261015-0726-rust-tauri-port/PORT-MAP.md`).

mod app_paths;
mod appupdate;
mod clock;
mod consts;
mod env;
mod errors;
mod feed;
mod os_store;
mod pet;
mod platform;
mod prefs;
mod runtime;
mod usage;
mod view;

pub use app_paths::AppPaths;
pub use appupdate::AppRelease;
pub use clock::{ClockSource, ClockTarget};
pub use consts::*;
pub use env::Env;
pub use errors::{
    AgentError, AuthNeeded, ClockError, FeedError, TokenExpired, UpdateError, UsageNotApplicable,
};
pub use feed::{FeedFile, FeedManifest};
pub use os_store::OsStoreNames;
pub use pet::PetConfig;
pub use platform::Platform;
pub use prefs::Prefs;
pub use runtime::{new_stop, BoxFuture, Clock, MonotonicClock, StopRx, SystemClock, TargetsFn};
pub use usage::UsagePayload;
pub use view::{DesiredState, View, WireAnim, WireState};
