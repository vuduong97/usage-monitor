//! The clock firmware that ships inside the app, and when to install it (`firmware.py`).
//!
//! A public build carries none (private, git-ignored in Python): [`load_bundle`] is `None` and
//! the feed ([`crate::feed`]) is the source. `fwSeq`, `layouts` and `styleApi` are what an image
//! reports in `/api/heap` once it runs; the app pushes an image only to a clawd clock whose
//! `fwSeq` is lower (missing = 0), so it never downgrades one. `heap` arguments are the clock's
//! `/api/heap` JSON.

use std::path::{Path, PathBuf};

use clawd_contracts::Env;
use serde_json::Value;

pub use clawd_contracts::CLAWD_VARIANT;

/// `firmware.StylePack`: one style pack from a bundle's `styles/index.json` (or a feed release,
/// with `data` in memory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StylePack {
    /// `[a-z0-9_-]{1,15}`.
    pub id: String,
    /// At most 23 chars.
    pub name: String,
    pub path: PathBuf,
    pub size: usize,
    /// CRC-32 of bytes `[16..)`, from the pack header.
    pub crc: u32,
    pub light: bool,
    /// In memory (feed releases); `None` = read `path`.
    pub data: Option<Vec<u8>>,
}

impl StylePack {
    /// `StylePack.read()`: the pack, checked against the index and its own header (`CSTY`
    /// magic, size, CRC); a damaged one is never sent (`Err`).
    pub fn read(&self) -> Result<Vec<u8>, String> {
        todo!("phase 09")
    }
}

/// `firmware.STYLE_MAGIC`.
pub const STYLE_MAGIC: &[u8; 4] = b"CSTY";
/// `firmware.STYLE_MAX`: the pack compiler's cap.
pub const STYLE_MAX: usize = 20_480;

/// `firmware.STYLE_ID.fullmatch(id)`: `[a-z0-9_-]{1,15}`.
pub fn is_valid_style_id(id: &str) -> bool {
    todo!("phase 09")
}

/// `firmware.bundle_dir()`: `env` var `CLAWD_FIRMWARE_DIR`, else the app's resources dir.
pub fn bundle_dir(env: &Env) -> PathBuf {
    todo!("phase 09")
}

/// `firmware.load_styles(folder, api)`: the packs in `folder/index.json` built for style API
/// `api`; empty when none or unreadable.
pub fn load_styles(folder: &Path, api: u32) -> Vec<StylePack> {
    todo!("phase 09")
}

/// `firmware.Bundle`: firmware image + manifest (`firmware.json`: `file size sha256 variant fwSeq
/// layouts styleApi commit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub path: PathBuf,
    pub size: usize,
    pub sha256: String,
    pub variant: String,
    /// `fwSeq`.
    pub seq: u64,
    pub layouts: u32,
    pub commit: String,
    /// Style packs the image draws (0: none).
    pub style_api: u32,
    pub styles: Vec<StylePack>,
}

impl Bundle {
    /// `Bundle.read()`: the image, checked against the manifest (size and SHA-256).
    pub fn read(&self) -> Result<Vec<u8>, String> {
        todo!("phase 09")
    }
}

/// `firmware.load_bundle(folder)`: `None` when missing, damaged, or the image is not a file.
pub fn load_bundle(env: &Env, folder: Option<&Path>) -> Option<Bundle> {
    todo!("phase 09")
}

/// `firmware.is_clawd(heap)`: the clawd firmware: says so (`variant`), or predates that field
/// but has the desk player (`desk` and `pak` keys).
pub fn is_clawd(heap: &Value) -> bool {
    todo!("phase 09")
}

/// `firmware.clock_seq(heap)`: `fwSeq` (an integer, not a bool), else 0.
pub fn clock_seq(heap: &Value) -> u64 {
    todo!("phase 09")
}

/// `firmware.update_state(heap, bundle)`: `"available"` | `"current"` | `"none"` (no bundle) |
/// `"incompatible"` (not a clawd clock).
pub fn update_state(heap: &Value, bundle: Option<&Bundle>) -> &'static str {
    todo!("phase 09")
}

/// `firmware.clock_style_api(heap)`: the style pack API the clock draws (0: an image from before
/// style packs).
pub fn clock_style_api(heap: &Value) -> u32 {
    todo!("phase 09")
}

/// `firmware.clock_style(heap)`: the id of the pack the clock shows, `""` for the classic screen.
pub fn clock_style(heap: &Value) -> String {
    todo!("phase 09")
}

/// `firmware.style_state(heap, bundle)`: for the bundled packs `"ready"` (the clock draws them)
/// | `"update"` (after installing the bundled image) | `"unavailable"`.
pub fn style_state(heap: &Value, bundle: Option<&Bundle>) -> &'static str {
    todo!("phase 09")
}
