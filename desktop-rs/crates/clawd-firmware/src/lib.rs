//! `clawd-firmware`: the clock firmware the app carries or fetches, and how it gets onto the
//! clock (phase 09 implements it).
//!
//! STUB (phase 02): real signatures and docs, `todo!()` bodies. Python origin: `firmware.py`,
//! `feed.py`, `ota.py`, `preview.py` (modules keep their Python names; see
//! `desktop-rs/PORT-MAP.md`). Depends on `clawd-config` (feed URL/token/channel come from env ->
//! config file -> build-time `option_env!`) and `clawd-clock` (`ClockApi`). Fixtures live under
//! `fixtures/fw/` because `tools/check_public.py` forbids a directory named `firmware`, and no
//! pack/zip/APNG/art may be committed (hashes only).
#![allow(
    unused_variables,
    dead_code,
    clippy::new_without_default,
    clippy::too_many_arguments,
    clippy::type_complexity
)] // stub crate: the owning phase removes this when it implements the bodies

pub mod feed;
pub mod firmware;
pub mod ota;
pub mod preview;
