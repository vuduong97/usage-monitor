//! What the clock's screen looks like, drawn by the clock's own code (`preview.py`).
//!
//! `clawd_render` (a C++ program from the firmware's drawing sources) takes the usage payload
//! last sent and a style (the classic screen, or a style pack) and writes the 240x240 frame the
//! clock would show, or (`--clip`) one loop as it animates. This module runs it and wraps the
//! pixels in a BMP, or the loop in an animated PNG, for the settings page. A renderer is only
//! trusted when built from the bundled firmware's commit (else the page draws an approximation).
//!
//! A public build has no renderer; it uses the feed release's, unpacked into
//! `~/.config/claude-usage-monitor/renderer/<sha16>/` with a `.files.json` marker and re-verified
//! file by file on every load: the one decrypted program the app keeps on disk. Unzip defends
//! against path traversal. No renderer binary, zip, pak or APNG is ever committed: tests use a
//! fake renderer script and compare hashes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use clawd_contracts::UsagePayload;
use thiserror::Error;

/// `preview.W` / `preview.H`.
pub const W: usize = 240;
pub const H: usize = 240;
/// `preview.FRAME_BYTES`.
pub const FRAME_BYTES: usize = W * H * 3;
/// `preview.RUN_TIMEOUT`.
pub const RUN_TIMEOUT: Duration = Duration::from_secs(15);
/// `preview.CACHE_MAX`.
pub const CACHE_MAX: usize = 32;
/// `preview.CLIP_MAGIC`.
pub const CLIP_MAGIC: &[u8; 4] = b"CLP1";
/// `preview.RENDER_API`: what this app expects of `clawd_render`; a newer one is not run.
pub const RENDER_API: u32 = 1;
/// `preview.CLASSIC`: the style key of the classic screen (no pack).
pub const CLASSIC: &str = "classic";
/// `preview.PAK_MAX`.
pub const PAK_MAX: usize = 512_000;

/// A `ValueError`/`OSError` of the Python module.
#[derive(Debug, Error)]
pub enum PreviewError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// `preview.preview_dir()`: env `CLAWD_PREVIEW_DIR`, else the app's resources dir (private
/// builds only).
pub fn preview_dir() -> PathBuf {
    todo!("phase 09")
}

/// `preview.Renderer`: the `clawd_render` program with its pak and style packs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renderer {
    pub exe: PathBuf,
    pub pak: PathBuf,
    pub commit: String,
    /// Pack id -> `.pak` next to the renderer.
    pub styles: std::collections::HashMap<String, PathBuf>,
}

impl Renderer {
    /// `Renderer.draw(style, payload)`: RGB bytes ([`FRAME_BYTES`]) of the screen in `style`
    /// (`None` payload or one the clock would reject: its waiting screen; exit code 2 retries
    /// without payload). No console window on Windows; timeout [`RUN_TIMEOUT`].
    pub async fn draw(
        &self,
        style: &str,
        payload: Option<&UsagePayload>,
    ) -> Result<Vec<u8>, PreviewError> {
        todo!("phase 09")
    }

    /// `Renderer.clip(style, payload)`: one loop of the screen as it animates (see [`parse_clip`]).
    pub async fn clip(
        &self,
        style: &str,
        payload: Option<&UsagePayload>,
    ) -> Result<Vec<Frame>, PreviewError> {
        todo!("phase 09")
    }
}

/// `preview.Frame`: one step of a clip: `rgb` (`w*h*3` bytes) painted at `x, y`, shown `delay_ms`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub delay_ms: u32,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub rgb: Vec<u8>,
}

/// `preview.parse_clip(data)`: the `CLP1` stream from `clawd_render --clip`.
pub fn parse_clip(data: &[u8]) -> Result<Vec<Frame>, PreviewError> {
    todo!("phase 09")
}

/// `preview.apng(frames)`: an animated PNG (own chunk writer, deflate via `miniz_oxide`).
pub fn apng(frames: &[Frame]) -> Vec<u8> {
    todo!("phase 09")
}

/// `preview.apng_data_url(frames)`: `data:image/png;base64,...`.
pub fn apng_data_url(frames: &[Frame]) -> String {
    todo!("phase 09")
}

/// `preview.load_renderer(folder, firmware_commit)`: the built renderer, or `None` when it is
/// missing, damaged, speaks another `renderApi`, or was built from another firmware commit than
/// `firmware_commit` (`""` = no bundled firmware to compare).
pub fn load_renderer(folder: Option<&Path>, firmware_commit: &str) -> Option<Renderer> {
    todo!("phase 09")
}

/// `preview.same_commit(a, b)`: two git hashes of one commit, either abbreviated.
pub fn same_commit(a: &str, b: &str) -> bool {
    todo!("phase 09")
}

/// `preview.unpack_renderer(data, root, sha256)`: unpack a renderer zip (already checked against
/// the manifest's `sha256`) into `root/<sha256[:16]>`, replacing what is there and dropping older
/// folders; returns the folder. `Invalid` when the zip is not a renderer (only
/// `preview.json`, `clawd_render[.exe]`, `clawd.pak`, `styles/...` are accepted; at most 256
/// files, 64 MiB), `Io` when it cannot be written.
pub fn unpack_renderer(data: &[u8], root: &Path, sha256: &str) -> Result<PathBuf, PreviewError> {
    todo!("phase 09")
}

/// `preview.load_unpacked(root, sha256, firmware_commit)`: the renderer `unpack_renderer` left
/// for this zip, every file re-checked against the marker (damaged or changed: `None`, so it is
/// unpacked again).
pub fn load_unpacked(root: &Path, sha256: &str, firmware_commit: &str) -> Option<Renderer> {
    todo!("phase 09")
}

/// `preview.bmp_data_url(rgb)`: a 24-bit BMP of a [`W`]x[`H`] RGB frame as a `data:` URL.
pub fn bmp_data_url(rgb: &[u8]) -> Result<String, PreviewError> {
    todo!("phase 09")
}

/// `preview.live_payload(payload, now)`: the stored payload with its wall-clock `t` moved to the
/// start of `now`'s minute (the title clock ticks on); `None` for no payload.
pub fn live_payload(payload: Option<&UsagePayload>, now: f64) -> Option<UsagePayload> {
    todo!("phase 09")
}

/// `preview.pak_count(data)`: how many animations a `clawd.pak` holds (`CLWD` magic, version 1,
/// descriptors checked); `Invalid` when it is not one.
pub fn pak_count(data: &[u8]) -> Result<u32, PreviewError> {
    todo!("phase 09")
}

/// `preview.PakCache(path)`: the last `clawd.pak` read from a clock, on disk (`count` = what
/// `/api/heap` said it held).
pub struct PakCache {
    _private: (),
}

impl PakCache {
    pub fn new(path: PathBuf) -> Self {
        todo!("phase 09")
    }

    /// `PakCache.load()`: `(pak, count)` when present and valid.
    pub fn load(&self) -> Option<(Vec<u8>, u32)> {
        todo!("phase 09")
    }

    /// `PakCache.save(data, count)`: atomic.
    pub fn save(&self, data: &[u8], count: u32) -> std::io::Result<()> {
        todo!("phase 09")
    }
}

/// `preview.FrameCache(size)`: the last few frames, so reopening a tab or polling does not run
/// the renderer again (LRU).
pub struct FrameCache {
    _private: (),
}

impl FrameCache {
    /// `FrameCache(size)`; default [`CACHE_MAX`].
    pub fn new(size: usize) -> Self {
        todo!("phase 09")
    }

    pub fn get(&mut self, key: &str) -> Option<String> {
        todo!("phase 09")
    }

    pub fn put(&mut self, key: &str, value: String) {
        todo!("phase 09")
    }
}

/// `preview.frame_key(style, payload, minute, kind)`: `"<kind>|<style>|<minute>|<payload JSON,
/// sorted keys>"`; a frame is good for the minute (the title clock moves once a minute).
pub fn frame_key(style: &str, payload: Option<&UsagePayload>, minute: u64, kind: &str) -> String {
    todo!("phase 09")
}
