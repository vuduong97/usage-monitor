//! The mutable half of a settings window's session (`SettingsSession.data`). STUB owned by
//! phase 13, which adds whatever `SettingsApi.__init__` kept per window
//! (`settings_api.py:133-176`): the `ClockApi` + keychain login for `SettingsSession.base`/`key`
//! (replaced by `_follow_store`), the last `/api/heap` and `/api/config`, the feed release and
//! its check state (`_release`, `_feed_at`, `_feed_error`, `_feed_pending`), the preview
//! `FrameCache`, `_pak_failed`, the mDNS browser of the setup wizard. Fields are added freely:
//! nothing outside phase 13 builds this struct (it is only created with `Default`).
#![allow(unused_variables)]

use std::sync::Arc;

use clawd_clock::ClockApi;
use serde_json::Value;

/// Per-window state; `Default` is an empty session.
#[derive(Default)]
pub struct SessionData {
    /// The clock client for this window (`SettingsApi._api`).
    pub api: Option<Arc<ClockApi>>,
    /// Last `/api/heap` (`_heap`).
    pub heap: Value,
    /// Last `/api/config` (`_cfg`).
    pub cfg: Value,
}
