//! The firmware feed: clock firmware and packs newer than what the app ships (`feed.py`).
//!
//! A release is published to a private bucket behind a small Worker. The app asks it with its
//! app token: `GET /v1/latest?channel=<channel>` (manifest v1: seq, notes, one row per file
//! `{name, kind, size, enc, sha256, crc}`, plus `renderers[]`), `POST /v1/grant {"seq": N}` (the
//! AES-256-GCM release key and short-lived signed URLs). Every file is
//! `nonce(12) | ciphertext | tag(16)` with AAD `clawd-fw/<seq>/<name>`; it is decrypted in
//! memory and checked against the manifest's size, SHA-256 and CRC. The encrypted files are
//! cached per release in `feed-cache/`.
//!
//! Security (red team #15, frozen): the feed URL must be `https://` (or loopback `http://` for
//! tests); the client **never follows redirects**; the token goes nowhere else.
//! URL/token/channel: env `CLAWD_FEED_URL` / `CLAWD_FEED_TOKEN` / `CLAWD_FEED_CHANNEL` -> config
//! `feed_url` / `feed_token` / `feed_channel` -> build time
//! (`clawd_contracts::BUILD_FEED_URL` / `BUILD_FEED_TOKEN`); without URL and token there is no
//! feed ([`from_settings`] is `None`).

use std::collections::HashMap;
use std::ops::Deref;
use std::path::PathBuf;
use std::time::Duration;

use clawd_contracts::{AppPaths, FeedError};
use serde_json::{Map, Value};

use crate::firmware::{Bundle, StylePack};

pub use clawd_contracts::{FeedFile, FeedManifest as Manifest};

/// `feed.TIMEOUT`.
pub const TIMEOUT: Duration = Duration::from_secs(30);
/// `feed.DOWNLOAD_TIMEOUT`.
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
/// `feed.CHECK_EVERY` (seconds): the tray's background check.
pub const CHECK_EVERY: f64 = 6.0 * 3600.0;
/// `feed.KINDS`: file kinds a manifest may carry.
pub const KINDS: [&str; 5] = ["fw", "clawdpak", "deskpak", "styles", "style"];
/// `feed.PACK_KINDS`.
pub const PACK_KINDS: [&str; 2] = ["clawdpak", "deskpak"];
/// `feed.STYLE_KINDS`.
pub const STYLE_KINDS: [&str; 2] = ["styles", "style"];
/// `feed.MAX_FILE`.
pub const MAX_FILE: u64 = 4 * 1024 * 1024;
/// `feed.RENDERER_MAX`.
pub const RENDERER_MAX: u64 = 32 * 1024 * 1024;
/// `feed.RENDERER_PLATFORMS`.
pub const RENDERER_PLATFORMS: [&str; 3] = ["win-x64", "mac", "linux-x64"];

/// `feed.renderer_platform()`: this computer's key in `renderers`: `win-x64` (also Windows on
/// Arm), `mac` (one universal file), `linux-x64`.
pub fn renderer_platform() -> &'static str {
    todo!("phase 09")
}

/// `feed.parse_manifest(d)`: the manifest, every field checked (`v == 1`, names, kinds, sizes,
/// `enc == size + 28`, hashes, exactly one `fw`, `renderers` validated apart; a bad renderer row
/// drops the renderers, not the release). `Err` carries the Python `ValueError` text.
pub fn parse_manifest(d: &Value) -> Result<Manifest, FeedError> {
    todo!("phase 09")
}

/// `feed.decrypt(key, seq, name, blob)`: AES-256-GCM (`aes-gcm` 0.11); `blob` is
/// `nonce(12) | ciphertext | tag(16)`, AAD `clawd-fw/<seq>/<name>`. `FeedError` when too short
/// or it does not decrypt.
pub fn decrypt(key: &[u8], seq: u64, name: &str, blob: &[u8]) -> Result<Vec<u8>, FeedError> {
    todo!("phase 09")
}

/// `feed.Release`: a feed release in memory; the rest of the app treats it like the bundled
/// firmware (derefs to [`Bundle`]) plus the packs. A bundled firmware is wrapped with
/// [`Release::from_bundle`] (no manifest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub bundle: Bundle,
    pub manifest: Option<Manifest>,
    /// File name -> plaintext (only what was fetched).
    pub blobs: HashMap<String, Vec<u8>>,
}

impl Deref for Release {
    type Target = Bundle;
    fn deref(&self) -> &Bundle {
        &self.bundle
    }
}

impl Release {
    /// A bundled firmware as a manifest-less release.
    pub fn from_bundle(bundle: Bundle) -> Self {
        Self {
            bundle,
            manifest: None,
            blobs: HashMap::new(),
        }
    }

    /// `Release.notes` (empty without manifest).
    pub fn notes(&self) -> &str {
        todo!("phase 09")
    }

    /// `Release.date`.
    pub fn date(&self) -> &str {
        todo!("phase 09")
    }

    /// `Release.has(kind)`: the release has files of that kind and all are in memory.
    pub fn has(&self, kind: &str) -> bool {
        todo!("phase 09")
    }

    /// `Release.missing(kinds)`: of `kinds`, the ones the release has but not in memory yet.
    pub fn missing(&self, kinds: &[&str]) -> Vec<String> {
        todo!("phase 09")
    }

    /// `Release.read()`: the firmware image (once fetched), checked against the manifest.
    pub fn read(&self) -> Result<Vec<u8>, String> {
        todo!("phase 09")
    }

    /// `Release.pack(kind)`: `(pack, its CRC-32 hex)` for `clawdpak` / `deskpak`, `None` when the
    /// release has none (or it is not fetched).
    pub fn pack(&self, kind: &str) -> Option<(Vec<u8>, String)> {
        todo!("phase 09")
    }
}

/// `feed.make_release(m, blobs)`: build a [`Release`] from a manifest and the fetched files
/// (style packs from the `styles` index, each checked).
pub fn make_release(m: &Manifest, blobs: HashMap<String, Vec<u8>>) -> Release {
    todo!("phase 09")
}

/// `progress(kind, done_bytes, total_bytes)` during [`Feed::fetch`].
pub type FetchProgress<'a> = &'a (dyn Fn(&str, u64, u64) + Send + Sync);

/// `feed.Feed`: the client. One per use; cheap to build.
pub struct Feed {
    _private: (),
}

impl Feed {
    /// `Feed(url, token, channel, cache_dir)`: `channel` other than `stable`/`beta` becomes
    /// `stable`; `cache_dir` `None` = no cache. No redirects are followed.
    pub fn new(url: &str, token: &str, channel: &str, cache_dir: Option<PathBuf>) -> Self {
        todo!("phase 09")
    }

    /// `Feed.latest()`: the channel's newest release manifest; `Ok(None)` on 404.
    pub async fn latest(&self) -> Result<Option<Manifest>, FeedError> {
        todo!("phase 09")
    }

    /// `Feed.fetch(rel, kinds, progress)`: `rel` with the files of `kinds` downloaded (8 at a
    /// time), decrypted and verified; the first failure ends the fetch.
    pub async fn fetch(
        &self,
        rel: &Release,
        kinds: &[&str],
        progress: Option<FetchProgress<'_>>,
    ) -> Result<Release, FeedError> {
        todo!("phase 09")
    }

    /// `Feed.fetch_renderer(rel, platform)`: the release's renderer zip for `platform`,
    /// decrypted and checked; `Ok(None)` when it has none.
    pub async fn fetch_renderer(
        &self,
        rel: &Release,
        platform: &str,
    ) -> Result<Option<(FeedFile, Vec<u8>)>, FeedError> {
        todo!("phase 09")
    }
}

/// `feed.from_settings()`: the feed this install uses; `None` when it has no URL or token, or the
/// URL is neither `https://` nor loopback `http://`.
pub fn from_settings(paths: &AppPaths) -> Option<Feed> {
    todo!("phase 09")
}

/// `feed.pack_state(heap, kind, crc)`: `"ok"` (the clock has this pack) | `"need"` | `"wait"`
/// (it is still checksumming it) | `"unknown"` (its firmware does not say: decided after the
/// firmware update).
pub fn pack_state(heap: &Value, kind: &str, crc: &str) -> &'static str {
    todo!("phase 09")
}

/// `feed.plan(heap, rel)`: what installing `rel` means for this clock: `{"fw": "need"}` when its
/// image is newer, and per pack of a feed release its [`pack_state`]; `{}` when nothing applies.
/// Insertion order is `fw`, `clawdpak`, `deskpak`.
pub fn plan(heap: &Value, rel: Option<&Release>) -> Map<String, Value> {
    todo!("phase 09")
}

/// `feed.needed(heap, rel)`: the kinds to download for an update: the firmware if newer, and
/// every pack not known to match (`need`, `wait`, `unknown`).
pub fn needed(heap: &Value, rel: Option<&Release>) -> Vec<String> {
    todo!("phase 09")
}

/// `feed.worth_it(heap, rel)`: something to install now: a newer image, or a pack known to differ.
pub fn worth_it(heap: &Value, rel: Option<&Release>) -> bool {
    todo!("phase 09")
}

/// `feed.offer(heap, m, bundle)`: the release number the tray should offer for this clock, `0` for
/// none. A bundled image at least as new as the feed's wins (a private build).
pub fn offer(heap: &Value, m: Option<&Manifest>, bundle: Option<&Bundle>) -> u64 {
    todo!("phase 09")
}

/// Style packs of a release from its `styles` index, each verified; empty when absent or not for
/// the manifest's style API (`feed._styles`).
pub fn styles_of(m: &Manifest, blobs: &HashMap<String, Vec<u8>>) -> Vec<StylePack> {
    todo!("phase 09")
}
