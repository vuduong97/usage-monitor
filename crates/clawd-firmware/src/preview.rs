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
//! fake renderer program and compare hashes.
//!
//! Blocking calls here (`load_*`, `unpack_renderer`, [`PakCache`], file reads) go through
//! `spawn_blocking` in async callers. [`Renderer::draw`] / [`Renderer::clip`] are async (the
//! child runs on tokio's process driver, killed when the timeout drops the future).

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use base64::Engine as _;
use clawd_contracts::{Env, UsagePayload};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use crate::crc32::crc32;
use crate::firmware::{load_styles, sha256_hex};
use crate::pyval;

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

/// `preview.UNPACK_MAX`: bytes, all files of a renderer zip together.
const UNPACK_MAX: u64 = 64 * 1024 * 1024;
/// `preview.UNPACK_FILES`.
const UNPACK_FILES: usize = 256;
/// `preview.MARK`: `{"zip": sha256, "files": {name: sha256}}`, written last.
const MARK: &str = ".files.json";
const PAK_HEADER: usize = 12;
const PAK_DESC: usize = 72;

/// A `ValueError`/`OSError` of the Python module.
#[derive(Debug, Error)]
pub enum PreviewError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn invalid(msg: impl Into<String>) -> PreviewError {
    PreviewError::Invalid(msg.into())
}

/// `preview.preview_dir()`: `env` var `CLAWD_PREVIEW_DIR`, else `renderer/` in the app's resources
/// dir (private builds only).
pub fn preview_dir(env: &Env) -> PathBuf {
    preview_dir_in(env, &crate::resources_dir())
}

/// [`preview_dir`] with the resources dir given (the app passes Tauri's `resource_dir()`).
pub fn preview_dir_in(env: &Env, resource_dir: &Path) -> PathBuf {
    match env.get("CLAWD_PREVIEW_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => resource_dir.join("renderer"),
    }
}

/// `preview.Renderer`: the `clawd_render` program with its pak and style packs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renderer {
    pub exe: PathBuf,
    pub pak: PathBuf,
    pub commit: String,
    /// Pack id -> `.pak` next to the renderer.
    pub styles: HashMap<String, PathBuf>,
}

struct RunOutput {
    code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn stderr_tail(stderr: &[u8]) -> String {
    String::from_utf8_lossy(&stderr[stderr.len().saturating_sub(200)..]).into_owned()
}

fn payload_stdin(payload: Option<&UsagePayload>) -> Option<(Vec<u8>, &UsagePayload)> {
    let p = payload.filter(|p| !p.0.is_empty())?;
    Some((
        pyval::json_dumps(&Value::Object(p.0.clone()), false).into_bytes(),
        p,
    ))
}

impl Renderer {
    /// `Renderer._args(style)`.
    fn args(&self, style: &str, clip: bool) -> Result<Vec<String>, PreviewError> {
        let mut args = vec!["--pak".to_owned(), self.pak.to_string_lossy().into_owned()];
        if style != CLASSIC {
            let path = self.styles.get(style).ok_or_else(|| {
                invalid(format!(
                    "no style pack {} next to the renderer",
                    pyval::str_repr(style)
                ))
            })?;
            args.push("--style".to_owned());
            args.push(path.to_string_lossy().into_owned());
        }
        if clip {
            args.push("--clip".to_owned());
        }
        Ok(args)
    }

    async fn run(
        &self,
        args: &[String],
        stdin: &[u8],
        timeout: Duration,
    ) -> Result<RunOutput, PreviewError> {
        let mut cmd = tokio::process::Command::new(&self.exe);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd.spawn()?;
        if let Some(mut si) = child.stdin.take() {
            // a payload is under 200 bytes: it fits the pipe, no reader thread needed. A program
            // that exits without reading it (broken pipe) is judged by its exit code instead.
            let _ = si.write_all(stdin).await;
        }
        let out = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| {
                PreviewError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("clawd_render did not finish in {} s", timeout.as_secs_f32()),
                ))
            })??;
        Ok(RunOutput {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }

    /// `Renderer.draw(style, payload)`: RGB bytes ([`FRAME_BYTES`]) of the screen in `style`
    /// (`None` payload or one the clock would reject: its waiting screen; exit code 2 retries
    /// without payload). No console window on Windows; timeout [`RUN_TIMEOUT`].
    pub async fn draw(
        &self,
        style: &str,
        payload: Option<&UsagePayload>,
    ) -> Result<Vec<u8>, PreviewError> {
        self.draw_timeout(style, payload, RUN_TIMEOUT).await
    }

    /// [`Renderer::draw`] with its own timeout (tests).
    pub async fn draw_timeout(
        &self,
        style: &str,
        payload: Option<&UsagePayload>,
        timeout: Duration,
    ) -> Result<Vec<u8>, PreviewError> {
        let args = self.args(style, false)?;
        let mut payload = payload_stdin(payload);
        loop {
            let stdin = payload.as_ref().map_or(&[][..], |(b, _)| b.as_slice());
            let r = self.run(&args, stdin, timeout).await?;
            if r.code == 2 && payload.is_some() {
                // the clock ignores such a payload: it would stay on its last screen
                payload = None;
                continue;
            }
            if r.code != 0 || r.stdout.len() != FRAME_BYTES {
                return Err(invalid(format!(
                    "clawd_render failed ({}): {}",
                    r.code,
                    stderr_tail(&r.stderr)
                )));
            }
            return Ok(r.stdout);
        }
    }

    /// `Renderer.clip(style, payload)`: one loop of the screen as it animates (see [`parse_clip`]).
    pub async fn clip(
        &self,
        style: &str,
        payload: Option<&UsagePayload>,
    ) -> Result<Vec<Frame>, PreviewError> {
        self.clip_timeout(style, payload, RUN_TIMEOUT).await
    }

    /// [`Renderer::clip`] with its own timeout (tests).
    pub async fn clip_timeout(
        &self,
        style: &str,
        payload: Option<&UsagePayload>,
        timeout: Duration,
    ) -> Result<Vec<Frame>, PreviewError> {
        let args = self.args(style, true)?;
        let mut payload = payload_stdin(payload);
        loop {
            let stdin = payload.as_ref().map_or(&[][..], |(b, _)| b.as_slice());
            let r = self.run(&args, stdin, timeout).await?;
            if r.code == 2 && payload.is_some() {
                payload = None;
                continue;
            }
            if r.code != 0 {
                return Err(invalid(format!(
                    "clawd_render --clip failed ({}): {}",
                    r.code,
                    stderr_tail(&r.stderr)
                )));
            }
            return parse_clip(&r.stdout);
        }
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

impl Frame {
    fn is_whole_screen(&self) -> bool {
        (self.x, self.y, self.w, self.h) == (0, 0, W as u32, H as u32)
    }
}

fn le16(data: &[u8], at: usize) -> u32 {
    u32::from(u16::from_le_bytes([data[at], data[at + 1]]))
}

/// `preview.parse_clip(data)`: the `CLP1` stream from `clawd_render --clip`: `CLP1`, u16 count,
/// then per frame u16 delay x y w h and the RGB rectangle (little-endian). Frame 0 must be the
/// whole screen.
pub fn parse_clip(data: &[u8]) -> Result<Vec<Frame>, PreviewError> {
    if data.get(..4) != Some(CLIP_MAGIC.as_slice()) || data.len() < 6 {
        return Err(invalid("not a clawd_render clip"));
    }
    let count = le16(data, 4);
    let (mut off, mut frames) = (6usize, Vec::new());
    for _ in 0..count {
        if off + 10 > data.len() {
            return Err(invalid("clip cut short"));
        }
        let (delay, x, y, w, h) = (
            le16(data, off),
            le16(data, off + 2),
            le16(data, off + 4),
            le16(data, off + 6),
            le16(data, off + 8),
        );
        off += 10;
        let n = (w * h * 3) as usize;
        if w == 0 || h == 0 || x + w > W as u32 || y + h > H as u32 || off + n > data.len() {
            return Err(invalid("bad clip frame"));
        }
        frames.push(Frame {
            delay_ms: delay,
            x,
            y,
            w,
            h,
            rgb: data[off..off + n].to_vec(),
        });
        off += n;
    }
    if off != data.len() || frames.first().is_none_or(|f| !f.is_whole_screen()) {
        return Err(invalid("bad clip"));
    }
    Ok(frames)
}

fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    let mut crc_input = kind.to_vec();
    crc_input.extend_from_slice(body);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    out
}

fn png_data(f: &Frame) -> Vec<u8> {
    let stride = f.w as usize * 3;
    let mut raw = Vec::with_capacity((stride + 1) * f.h as usize);
    for y in 0..f.h as usize {
        raw.push(0);
        raw.extend_from_slice(&f.rgb[y * stride..(y + 1) * stride]);
    }
    miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9)
}

/// `preview.apng(frames)`: an animated PNG that loops `frames` forever: frame 0 whole, each later
/// one a rectangle over the one before (dispose none, blend source). A viewer without APNG shows
/// frame 0. Own chunk writer, deflate via `miniz_oxide` (the compressed bytes differ from
/// Python's zlib; the decompressed chunks are identical, which is what the golden compares).
/// `Invalid` when frame 0 is not the whole screen.
pub fn try_apng(frames: &[Frame]) -> Result<Vec<u8>, PreviewError> {
    if frames.first().is_none_or(|f| !f.is_whole_screen()) {
        return Err(invalid("frame 0 must be the whole screen"));
    }
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(W as u32).to_be_bytes());
    ihdr.extend_from_slice(&(H as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut actl = Vec::new();
    actl.extend_from_slice(&(frames.len() as u32).to_be_bytes());
    actl.extend_from_slice(&0u32.to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    out.extend(chunk(b"IHDR", &ihdr));
    out.extend(chunk(b"acTL", &actl));
    let mut seq = 0u32;
    for (i, f) in frames.iter().enumerate() {
        let mut fctl = Vec::new();
        for n in [seq, f.w, f.h, f.x, f.y] {
            fctl.extend_from_slice(&n.to_be_bytes());
        }
        fctl.extend_from_slice(&(f.delay_ms.clamp(1, 65535) as u16).to_be_bytes());
        fctl.extend_from_slice(&1000u16.to_be_bytes());
        fctl.extend_from_slice(&[0, 0]);
        out.extend(chunk(b"fcTL", &fctl));
        seq += 1;
        let data = png_data(f);
        if i == 0 {
            out.extend(chunk(b"IDAT", &data));
        } else {
            let mut body = seq.to_be_bytes().to_vec();
            body.extend(data);
            out.extend(chunk(b"fdAT", &body));
            seq += 1;
        }
    }
    out.extend(chunk(b"IEND", b""));
    Ok(out)
}

/// `preview.apng(frames)` for callers that cannot fail: empty bytes when [`try_apng`] refuses
/// (no frames, or frame 0 is not the whole screen). Prefer [`try_apng`] (Contract CR: the stub
/// signature has no error).
pub fn apng(frames: &[Frame]) -> Vec<u8> {
    try_apng(frames).unwrap_or_default()
}

/// `preview.apng_data_url(frames)`: `data:image/png;base64,...`, or `Invalid` like [`try_apng`].
pub fn try_apng_data_url(frames: &[Frame]) -> Result<String, PreviewError> {
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(try_apng(frames)?)
    ))
}

/// `preview.apng_data_url(frames)`: empty string when [`try_apng`] refuses.
pub fn apng_data_url(frames: &[Frame]) -> String {
    try_apng_data_url(frames).unwrap_or_default()
}

/// `preview.load_renderer(folder, firmware_commit)`: the built renderer, or `None` when it is
/// missing, damaged, speaks another `renderApi`, or was built from another firmware commit than
/// `firmware_commit` (`""` = no bundled firmware to compare). `folder` defaults to
/// [`preview_dir`].
pub fn load_renderer(env: &Env, folder: Option<&Path>, firmware_commit: &str) -> Option<Renderer> {
    let folder = folder.map_or_else(|| preview_dir(env), Path::to_path_buf);
    load_renderer_from(&folder, firmware_commit)
}

fn load_renderer_from(folder: &Path, firmware_commit: &str) -> Option<Renderer> {
    let text = std::fs::read_to_string(folder.join("preview.json")).ok()?;
    let m: Value = serde_json::from_str(&text).ok()?;
    let api = match m.get("renderApi") {
        Some(v) => pyval::int_of(v)?,
        None => 1,
    };
    if api != i64::from(RENDER_API) {
        return None;
    }
    let style_api = match m.get("styleApi") {
        Some(v) => u32::try_from(pyval::int_of(v)?).ok()?,
        None => 0,
    };
    let packs = load_styles(&folder.join("styles"), style_api);
    let r = Renderer {
        exe: folder.join(pyval::str_of(m.get("exe")?)),
        pak: folder.join(pyval::str_of(m.get("pak")?)),
        commit: m.get("commit").map(pyval::str_of).unwrap_or_default(),
        styles: packs.into_iter().map(|p| (p.id, p.path)).collect(),
    };
    if !r.exe.is_file() || !r.pak.is_file() {
        return None;
    }
    if !firmware_commit.is_empty()
        && !r.commit.is_empty()
        && !same_commit(&r.commit, firmware_commit)
    {
        return None;
    }
    Some(r)
}

/// `preview.same_commit(a, b)`: two git hashes of one commit, either abbreviated.
pub fn same_commit(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let n = a.chars().count().min(b.chars().count());
    a == b || (n >= 7 && a.chars().take(n).eq(b.chars().take(n)))
}

// ---- the renderer of a feed release, unpacked --------------------------------------------

/// `preview.RENDERER_FILE.fullmatch`: only what `build_preview.py` puts in the folder.
fn renderer_file(name: &str) -> bool {
    if matches!(
        name,
        "preview.json" | "clawd_render" | "clawd_render.exe" | "clawd.pak" | "styles/index.json"
    ) {
        return true;
    }
    name.strip_prefix("styles/")
        .and_then(|f| f.strip_suffix(".pak"))
        .is_some_and(crate::firmware::is_valid_style_id)
}

/// A manifest `sha256` is 64 lowercase hex; refuse anything that could be a path.
fn sha_prefix(sha256: &str) -> Result<&str, PreviewError> {
    if sha256.len() >= 16 && sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(&sha256[..16])
    } else {
        Err(invalid("renderer zip: bad sha256"))
    }
}

/// `preview.unpack_renderer(data, root, sha256)`: unpack a renderer zip (already checked against
/// the manifest's `sha256`) into `root/<sha256[:16]>`, replacing what is there and dropping older
/// folders; returns the folder. `Invalid` when the zip is not a renderer (only
/// `preview.json`, `clawd_render[.exe]`, `clawd.pak`, `styles/...` are accepted; at most 256
/// files, 64 MiB), `Io` when it cannot be written.
///
/// Names are taken as stored: a backslash or `..` never matches, so nothing can land outside the
/// folder (Python's `zipfile` turns `\` into `/` on Windows only; here a backslash is always
/// refused).
pub fn unpack_renderer(data: &[u8], root: &Path, sha256: &str) -> Result<PathBuf, PreviewError> {
    let prefix = sha_prefix(sha256)?;
    let files = read_renderer_zip(data)?;
    let (exe, _pak) = renderer_names(&files)?;
    #[cfg(not(unix))]
    let _ = &exe; // only the POSIX build marks it executable
    std::fs::create_dir_all(root)?;
    let folder = root.join(prefix);
    let tmp = root.join(format!(".{prefix}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("styles"))?;
    let written = (|| -> std::io::Result<()> {
        for (name, body) in &files {
            std::fs::write(tmp.join(name), body)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(tmp.join(&exe), std::fs::Permissions::from_mode(0o755))?;
        }
        let hashes: serde_json::Map<String, Value> = files
            .iter()
            .map(|(n, b)| (n.clone(), Value::from(sha256_hex(b))))
            .collect();
        let mark = pyval::json_dumps(&json!({"zip": sha256, "files": hashes}), false);
        std::fs::write(tmp.join(MARK), mark)?;
        if folder.exists() {
            // a damaged copy (load_unpacked refused it); Windows keeps a running program's
            // folder: Io, the next window retries
            std::fs::remove_dir_all(&folder)?;
        }
        std::fs::rename(&tmp, &folder)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e.into());
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for old in entries.flatten().map(|e| e.path()) {
            // one release's renderer is enough (a running one may stay)
            if old.is_dir() && old.file_name() != folder.file_name() {
                let _ = std::fs::remove_dir_all(old);
            }
        }
    }
    Ok(folder)
}

/// The files of a renderer zip, each read whole and CRC-checked.
fn read_renderer_zip(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, PreviewError> {
    let zip_err = |e: &dyn std::fmt::Display| invalid(format!("renderer zip: {e}"));
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| zip_err(&e))?;
    let mut infos: Vec<(usize, String, u64)> = Vec::new();
    for i in 0..z.len() {
        let f = z.by_index_raw(i).map_err(|e| zip_err(&e))?;
        if !f.is_dir() {
            infos.push((
                i,
                String::from_utf8_lossy(f.name_raw()).into_owned(),
                f.size(),
            ));
        }
    }
    let mut names: Vec<&str> = infos.iter().map(|(_, n, _)| n.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    let total: u64 = infos.iter().map(|(_, _, s)| *s).sum();
    if infos.is_empty()
        || infos.len() > UNPACK_FILES
        || names.len() != infos.len()
        || !infos.iter().all(|(_, n, _)| renderer_file(n))
        || total > UNPACK_MAX
    {
        return Err(invalid("not a renderer zip"));
    }
    let mut files = Vec::with_capacity(infos.len());
    for (i, name, size) in infos {
        let mut f = z.by_index(i).map_err(|e| zip_err(&e))?;
        let mut body = Vec::with_capacity(size as usize);
        // a lying header cannot make us read more than it declared; the CRC is checked at EOF
        (&mut f)
            .take(size + 1)
            .read_to_end(&mut body)
            .map_err(|e| zip_err(&e))?;
        if body.len() as u64 != size {
            return Err(zip_err(&"file size does not match its header"));
        }
        files.push((name, body));
    }
    Ok(files)
}

/// The `exe` and `pak` names `preview.json` of an unpacked zip announces, checked.
fn renderer_names(files: &[(String, Vec<u8>)]) -> Result<(String, String), PreviewError> {
    let get = |n: &str| files.iter().find(|(name, _)| name == n).map(|(_, b)| b);
    let unusable = || invalid("renderer zip without a usable preview.json");
    let m: Value = serde_json::from_slice(get("preview.json").ok_or_else(unusable)?)
        .map_err(|_| unusable())?;
    let (Some(exe), Some(pak)) = (m.get("exe"), m.get("pak")) else {
        return Err(unusable());
    };
    let (exe, pak) = (pyval::str_of(exe), pyval::str_of(pak));
    if get(&exe).is_none()
        || !exe.starts_with("clawd_render")
        || pak != "clawd.pak"
        || get(&pak).is_none()
    {
        return Err(invalid(
            "renderer zip: preview.json names files it does not hold",
        ));
    }
    Ok((exe, pak))
}

/// `preview.load_unpacked(root, sha256, firmware_commit)`: the renderer `unpack_renderer` left
/// for this zip, every file re-checked against the marker (damaged or changed: `None`, so it is
/// unpacked again).
pub fn load_unpacked(root: &Path, sha256: &str, firmware_commit: &str) -> Option<Renderer> {
    let folder = root.join(sha_prefix(sha256).ok()?);
    let mark: Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join(MARK)).ok()?).ok()?;
    let files = mark.get("files")?.as_object()?;
    if mark.get("zip")?.as_str()? != sha256 || !files.contains_key("preview.json") {
        return None;
    }
    for (name, digest) in files {
        if !renderer_file(name) {
            return None;
        }
        let body = std::fs::read(folder.join(name)).ok()?;
        if digest.as_str()? != sha256_hex(&body) {
            return None;
        }
    }
    load_renderer_from(&folder, firmware_commit)
}

// ---- BMP, live payload ----------------------------------------------------------------------

/// `preview.bmp_data_url(rgb)`: a 24-bit BMP of a [`W`]x[`H`] RGB frame as a `data:` URL (both
/// webviews show BMP).
pub fn bmp_data_url(rgb: &[u8]) -> Result<String, PreviewError> {
    if rgb.len() != FRAME_BYTES {
        return Err(invalid("not a 240x240 RGB frame"));
    }
    let stride = W * 3; // 720: already a multiple of 4
    let mut pixels = Vec::with_capacity(FRAME_BYTES);
    for y in (0..H).rev() {
        // bottom-up, RGB -> BGR
        for &[r, g, b] in rgb[y * stride..(y + 1) * stride].as_chunks::<3>().0 {
            pixels.extend_from_slice(&[b, g, r]);
        }
    }
    let mut bmp = b"BM".to_vec();
    bmp.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
    bmp.extend_from_slice(&[0, 0, 0, 0]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(W as i32).to_le_bytes());
    bmp.extend_from_slice(&(H as i32).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&(pixels.len() as u32).to_le_bytes());
    bmp.extend_from_slice(&2835i32.to_le_bytes());
    bmp.extend_from_slice(&2835i32.to_le_bytes());
    bmp.extend_from_slice(&[0; 8]);
    bmp.extend(pixels);
    Ok(format!(
        "data:image/bmp;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bmp)
    ))
}

/// Seconds east of UTC at `now` (`time.localtime(now).tm_gmtoff`).
#[cfg(unix)]
pub fn local_utc_offset_secs(now: f64) -> i64 {
    // SAFETY: `tm` is plain data, zeroed then filled by localtime_r, which only reads `t`.
    unsafe {
        let t = now as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            0
        } else {
            // `tm_gmtoff` is `c_long`: already i64 on 64-bit macOS and Linux (clippy flags the
            // conversion there), i32 on 32-bit targets, so the cast is the portable spelling.
            #[allow(clippy::unnecessary_cast)]
            {
                tm.tm_gmtoff as i64
            }
        }
    }
}

/// Seconds east of UTC (the time zone in force now; Python asks for `now`'s, which differs only
/// across a DST change).
#[cfg(windows)]
pub fn local_utc_offset_secs(_now: f64) -> i64 {
    #[repr(C)]
    struct SystemTime([u16; 8]);
    #[repr(C)]
    struct TimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SystemTime,
        daylight_bias: i32,
    }
    extern "system" {
        fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
    }
    // SAFETY: the struct has the Win32 TIME_ZONE_INFORMATION layout and is only written to.
    unsafe {
        let mut tz: TimeZoneInformation = std::mem::zeroed();
        let id = GetTimeZoneInformation(&mut tz);
        let extra = match id {
            1 => tz.standard_bias,
            2 => tz.daylight_bias,
            _ => 0,
        };
        -i64::from(tz.bias + extra) * 60
    }
}

/// `preview.live_payload(payload, now)`: the stored payload with its wall-clock `t` moved to the
/// start of `now`'s minute (the title clock ticks on, and a clip never shows the minute change);
/// `t` is local wall-clock seconds, as `usage.add_clock_fields` makes it. `None` for no (or an
/// empty) payload.
pub fn live_payload(payload: Option<&UsagePayload>, now: f64) -> Option<UsagePayload> {
    live_payload_at(payload, now, local_utc_offset_secs(now))
}

/// [`live_payload`] with the UTC offset (seconds) given: tests and the golden.
pub fn live_payload_at(
    payload: Option<&UsagePayload>,
    now: f64,
    gmtoff: i64,
) -> Option<UsagePayload> {
    let p = payload.filter(|p| !p.0.is_empty())?;
    let mut out = p.clone();
    if out.0.contains_key("t") {
        let t = now as i64 + gmtoff;
        out.0
            .insert("t".to_owned(), Value::from(t - t.rem_euclid(60)));
    }
    Some(out)
}

// ---- the clock's own animation pack, for the page's approximate preview -----------------------

/// `preview.pak_count(data)`: how many animations a `clawd.pak` holds (`CLWD` magic, version 1,
/// descriptors checked); `Invalid` when it is not one.
pub fn pak_count(data: &[u8]) -> Result<u32, PreviewError> {
    if data.len() < PAK_HEADER || data.len() > PAK_MAX || &data[..4] != b"CLWD" {
        return Err(invalid("not a clawd.pak"));
    }
    let (version, count) = (le16(data, 4), le16(data, 6) as usize);
    if version != 1 || !(0 < count && count < 64) || PAK_HEADER + count * PAK_DESC > data.len() {
        return Err(invalid("unknown clawd.pak version or count"));
    }
    let le32 = |at: usize| {
        u64::from(u32::from_le_bytes([
            data[at],
            data[at + 1],
            data[at + 2],
            data[at + 3],
        ]))
    };
    for i in 0..count {
        let d = PAK_HEADER + i * PAK_DESC;
        let (w, h) = (u64::from(data[d + 16]), u64::from(data[d + 17]));
        let (frames, fbytes) = (u64::from(le16(data, d + 20)), u64::from(le16(data, d + 26)));
        let (frames_off, holds_off) = (le32(d + 60), le32(d + 64));
        let len = data.len() as u64;
        if w == 0
            || h == 0
            || frames == 0
            || fbytes < (w * h).div_ceil(2)
            || frames_off + frames * fbytes > len
            || holds_off + 2 * frames > len
        {
            return Err(invalid(format!("clawd.pak animation {i} is damaged")));
        }
    }
    Ok(count as u32)
}

/// `preview.PakCache(path)`: the last `clawd.pak` read from a clock, on disk (`count` = what
/// `/api/heap` said it held).
pub struct PakCache {
    path: PathBuf,
    meta: PathBuf,
}

impl PakCache {
    pub fn new(path: PathBuf) -> Self {
        let mut meta = path.clone().into_os_string();
        meta.push(".json");
        Self {
            path,
            meta: PathBuf::from(meta),
        }
    }

    /// `PakCache.load()`: `(pak, count)` when present and valid.
    pub fn load(&self) -> Option<(Vec<u8>, u32)> {
        let data = std::fs::read(&self.path).ok()?;
        let meta: Value = serde_json::from_str(&std::fs::read_to_string(&self.meta).ok()?).ok()?;
        let count = u32::try_from(pyval::int_of(meta.get("count")?)?).ok()?;
        pak_count(&data).ok()?;
        Some((data, count))
    }

    /// `PakCache.save(data, count)`: atomic.
    pub fn save(&self, data: &[u8], count: u32) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, &self.path)?;
        std::fs::write(&self.meta, format!("{{\"count\": {count}}}"))
    }
}

/// `preview.FrameCache(size)`: the last few frames, so reopening a tab or polling does not run
/// the renderer again (LRU).
pub struct FrameCache {
    entries: VecDeque<(String, String)>,
    size: usize,
}

impl Default for FrameCache {
    fn default() -> Self {
        Self::new(CACHE_MAX)
    }
}

impl FrameCache {
    /// `FrameCache(size)`; default [`CACHE_MAX`].
    pub fn new(size: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            size,
        }
    }

    pub fn get(&mut self, key: &str) -> Option<String> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(at)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }

    pub fn put(&mut self, key: &str, value: String) {
        self.entries.retain(|(k, _)| k != key);
        self.entries.push_back((key.to_owned(), value));
        while self.entries.len() > self.size {
            self.entries.pop_front();
        }
    }
}

/// `preview.frame_key(style, payload, minute, kind)`: `"<kind>|<style>|<minute>|<payload JSON,
/// sorted keys>"`; a frame is good for the minute (the title clock moves once a minute). Python's
/// `json.dumps(None)` is `null`; an empty payload dumps as `{}`.
pub fn frame_key(style: &str, payload: Option<&UsagePayload>, minute: u64, kind: &str) -> String {
    let dumped = payload.map_or_else(
        || "null".to_owned(),
        |p| pyval::json_dumps(&Value::Object(p.0.clone()), true),
    );
    format!("{kind}|{style}|{minute}|{dumped}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn payload(v: Value) -> UsagePayload {
        UsagePayload(v.as_object().unwrap().clone())
    }

    fn pl() -> UsagePayload {
        payload(
            json!({"s": 42, "sr": 134, "w": 67, "wr": 3960, "st": "allowed", "t": 1_760_191_500, "tf": 24}),
        )
    }

    type TestFrame = (u16, u16, u16, u16, u16, Vec<u8>);

    fn clip_bytes(frames: &[TestFrame]) -> Vec<u8> {
        let mut out = b"CLP1".to_vec();
        out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
        for (d, x, y, w, h, rgb) in frames {
            for n in [d, x, y, w, h] {
                out.extend_from_slice(&n.to_le_bytes());
            }
            out.extend_from_slice(rgb);
        }
        out
    }

    fn full() -> TestFrame {
        (500, 0, 0, 240, 240, [7u8, 8, 9].repeat(240 * 240))
    }
    fn spot() -> TestFrame {
        (120, 10, 20, 2, 1, vec![1, 2, 3, 4, 5, 6])
    }

    #[test]
    fn parse_clip_reads_every_frame() {
        let frames = parse_clip(&clip_bytes(&[full(), spot()])).unwrap();
        let dims: Vec<_> = frames
            .iter()
            .map(|f| (f.delay_ms, f.x, f.y, f.w, f.h))
            .collect();
        assert_eq!(dims, [(500, 0, 0, 240, 240), (120, 10, 20, 2, 1)]);
        assert_eq!(frames[1].rgb, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn parse_clip_rejects_bad_data() {
        let whole = clip_bytes(&[full()]);
        let mut count_high = b"CLP1\x02\x00".to_vec();
        count_high.extend_from_slice(&whole[6..]);
        let mut trailing = whole.clone();
        trailing.push(b'x');
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("empty", vec![]),
            ("bad-magic", b"NOPE\x01\x00".to_vec()),
            ("frame0-partial", clip_bytes(&[spot()])),
            ("cut", whole[..whole.len() - 1].to_vec()),
            ("trailing", trailing),
            (
                "off-screen",
                clip_bytes(&[full(), (1, 239, 0, 2, 1, vec![0; 6])]),
            ),
            ("count-too-high", count_high),
        ];
        for (name, data) in cases {
            assert!(parse_clip(&data).is_err(), "{name}");
        }
    }

    /// Walk PNG chunks, verifying every CRC.
    fn chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let (mut off, mut out) = (8, Vec::new());
        while off < png.len() {
            let n = u32::from_be_bytes(png[off..off + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = png[off + 4..off + 8].try_into().unwrap();
            let body = png[off + 8..off + 8 + n].to_vec();
            let crc = u32::from_be_bytes(png[off + 8 + n..off + 12 + n].try_into().unwrap());
            let mut input = kind.to_vec();
            input.extend_from_slice(&body);
            assert_eq!(crc, crc32(&input));
            out.push((kind, body));
            off += 12 + n;
        }
        out
    }

    #[test]
    fn apng_loops_the_frames_with_their_delays_and_rectangles() {
        let png = try_apng(&parse_clip(&clip_bytes(&[full(), spot()])).unwrap()).unwrap();
        let cs = chunks(&png);
        let kinds: Vec<&[u8]> = cs.iter().map(|(k, _)| k.as_slice()).collect();
        assert_eq!(
            kinds,
            [
                b"IHDR".as_slice(),
                b"acTL",
                b"fcTL",
                b"IDAT",
                b"fcTL",
                b"fdAT",
                b"IEND"
            ]
        );
        assert_eq!(cs[0].1, [0, 0, 0, 240, 0, 0, 0, 240, 8, 2, 0, 0, 0]);
        assert_eq!(cs[1].1, [0, 0, 0, 2, 0, 0, 0, 0], "two frames, forever");
        let f = |c: &[u8]| -> Vec<u32> {
            (0..5)
                .map(|i| u32::from_be_bytes(c[i * 4..i * 4 + 4].try_into().unwrap()))
                .collect()
        };
        assert_eq!(f(&cs[2].1), [0, 240, 240, 0, 0]);
        assert_eq!(f(&cs[4].1), [1, 2, 1, 10, 20]);
        assert_eq!(&cs[2].1[20..], [0x01, 0xf4, 0x03, 0xe8, 0, 0]); // delay 500 / 1000, dispose 0, blend 0
        assert_eq!(&cs[4].1[20..], [0, 120, 0x03, 0xe8, 0, 0]);
        assert_eq!(
            u32::from_be_bytes(cs[5].1[..4].try_into().unwrap()),
            2,
            "sequence runs on"
        );
        let inflate = |z: &[u8]| miniz_oxide::inflate::decompress_to_vec_zlib(z).unwrap();
        assert_eq!(inflate(&cs[5].1[4..]), [0, 1, 2, 3, 4, 5, 6]);
        let raw = inflate(&cs[3].1);
        assert_eq!((raw.len(), &raw[..4]), (240 * 721, &[0u8, 7, 8, 9][..]));
        assert!(
            try_apng_data_url(&parse_clip(&clip_bytes(&[full()])).unwrap())
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert!(try_apng(&[]).is_err());
        assert!(apng(&[]).is_empty() && apng_data_url(&[]).is_empty());
    }

    #[test]
    fn bmp_is_bottom_up_bgr_and_the_right_size() {
        let mut rgb = vec![0u8; FRAME_BYTES];
        rgb[..3].copy_from_slice(&[10, 20, 30]);
        let last = (H - 1) * W * 3 + (W - 1) * 3;
        rgb[last..last + 3].copy_from_slice(&[1, 2, 3]);
        let url = bmp_data_url(&rgb).unwrap();
        let b64 = url.strip_prefix("data:image/bmp;base64,").unwrap();
        let bmp = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        assert_eq!((&bmp[..2], bmp.len()), (&b"BM"[..], 54 + FRAME_BYTES));
        assert_eq!(
            u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize,
            bmp.len()
        );
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 54);
        assert_eq!(i32::from_le_bytes(bmp[18..22].try_into().unwrap()), 240);
        assert_eq!(&bmp[26..30], [1, 0, 24, 0]);
        assert_eq!(&bmp[54 + 717..54 + 720], [3, 2, 1]);
        assert_eq!(&bmp[bmp.len() - 720..bmp.len() - 717], [30, 20, 10]);
        assert!(bmp_data_url(b"short").is_err());
    }

    #[test]
    fn live_payload_moves_the_wall_clock_to_the_minute() {
        let now = 1_800_000_000.0;
        let live = live_payload_at(Some(&pl()), now, 7 * 3600).unwrap();
        let local = 1_800_000_000 + 7 * 3600;
        assert_eq!(live.0["t"], local - local % 60);
        assert_eq!(live.0["s"], 42);
        assert_eq!(pl().0["t"], 1_760_191_500, "the stored one is not touched");
        assert_eq!(
            live_payload_at(Some(&payload(json!({"s": 1}))), now, 0)
                .unwrap()
                .0["s"],
            1
        );
        assert!(live_payload_at(Some(&payload(json!({"s": 1}))), now, 0)
            .unwrap()
            .0
            .get("t")
            .is_none());
        assert!(live_payload_at(None, now, 0).is_none());
        assert!(live_payload_at(Some(&UsagePayload::default()), now, 0).is_none());
        assert_eq!(
            live_payload(Some(&pl()), now).unwrap().0["t"]
                .as_i64()
                .unwrap()
                % 60,
            0
        );
    }

    #[test]
    fn frame_cache_keeps_the_latest_few() {
        let mut c = FrameCache::new(2);
        c.put("a", "1".into());
        c.put("b", "2".into());
        assert_eq!(c.get("a").as_deref(), Some("1"));
        c.put("c", "3".into());
        assert!(c.get("b").is_none());
        assert_eq!(
            (c.get("a").as_deref(), c.get("c").as_deref()),
            (Some("1"), Some("3"))
        );
    }

    #[test]
    fn frame_key_sorts_keys_like_python() {
        let p = payload(json!({"w": 1, "s": [1, "é"], "a": null}));
        assert_eq!(
            frame_key("synth", Some(&p), 29_903_333, "still"),
            r#"still|synth|29903333|{"a": null, "s": [1, "\u00e9"], "w": 1}"#
        );
        assert_eq!(frame_key("classic", None, 5, "clip"), "clip|classic|5|null");
    }

    #[test]
    fn same_commit_cases() {
        for (a, b, same) in [
            ("abc1234", "abc1234", true),
            ("abc1234", "abc1234def0", true),
            ("ABC1234DEF", "abc1234", true),
            ("abc1234", "abc1235", false),
            ("abc12", "abc1234", false),
            ("abc12", "abc12", true),
        ] {
            assert_eq!(same_commit(a, b), same, "{a} {b}");
        }
    }

    fn make_pak(count: usize) -> Vec<u8> {
        let (w, h, frames) = (4usize, 3usize, 2usize);
        let fbytes = (w * h).div_ceil(2);
        let mut head = b"CLWD".to_vec();
        head.extend_from_slice(&[1, 0, count as u8, 0, 0, 0, 0, 0]);
        let (mut descs, mut body) = (Vec::new(), Vec::new());
        let base = 12 + 72 * count;
        for i in 0..count {
            let frames_off = base + body.len();
            body.extend(vec![0x12u8; fbytes * frames]);
            let holds_off = base + body.len();
            body.extend([100u16, 100].iter().flat_map(|n| n.to_le_bytes()));
            let mut d = format!("anim{i}").into_bytes();
            d.resize(16, 0);
            d.extend_from_slice(&[w as u8, h as u8, 0, 0]);
            for n in [frames as u16, 0, frames as u16 - 1, fbytes as u16] {
                d.extend_from_slice(&n.to_le_bytes());
            }
            d.extend(vec![0u8; 32]);
            d.extend_from_slice(&(frames_off as u32).to_le_bytes());
            d.extend_from_slice(&(holds_off as u32).to_le_bytes());
            d.extend_from_slice(&[0; 4]);
            descs.extend(d);
        }
        [head, descs, body].concat()
    }

    #[test]
    fn pak_count_reads_a_pack_and_refuses_anything_else() {
        assert_eq!(pak_count(&make_pak(3)).unwrap(), 3);
        let p = make_pak(2);
        let mut wrong_version = b"CLWD".to_vec();
        wrong_version.extend_from_slice(&[2, 0, 1, 0]);
        wrong_version.extend(vec![0; 200]);
        let mut zero = b"CLWD".to_vec();
        zero.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
        let mut big = b"CLWD".to_vec();
        big.extend(vec![0; PAK_MAX]);
        for bad in [
            vec![],
            [b"NOPE".to_vec(), vec![0; 20]].concat(),
            p[..p.len() - 1].to_vec(),
            wrong_version,
            zero,
            big,
        ] {
            assert!(pak_count(&bad).is_err());
        }
    }

    #[test]
    fn pak_cache_roundtrip_is_atomic_and_validated() {
        let t = tempfile::tempdir().unwrap();
        let c = PakCache::new(t.path().join("sub").join("clawd.pak"));
        assert!(c.load().is_none());
        let pak = make_pak(2);
        c.save(&pak, 17).unwrap();
        assert_eq!(c.load(), Some((pak.clone(), 17)));
        let mut names: Vec<_> = std::fs::read_dir(t.path().join("sub"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["clawd.pak", "clawd.pak.json"], "no .tmp left");
        assert_eq!(
            std::fs::read_to_string(t.path().join("sub/clawd.pak.json")).unwrap(),
            r#"{"count": 17}"#
        );
        std::fs::write(t.path().join("sub/clawd.pak"), b"CLWD junk").unwrap();
        assert!(c.load().is_none());
    }

    // ---- renderer zip ----------------------------------------------------------------------

    fn make_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (n, d) in files {
            w.start_file(*n, opts).unwrap();
            w.write_all(d).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn good_files(commit: &str) -> Vec<(String, Vec<u8>)> {
        let pack = {
            let mut rest = b"synth".to_vec();
            rest.resize(80, 0);
            let mut p = b"CSTY".to_vec();
            p.extend_from_slice(&[1, 0, 96, 0]);
            p.extend_from_slice(&(rest.len() as u32).to_le_bytes());
            p.extend_from_slice(&crc32(&rest).to_le_bytes());
            p.extend(rest);
            p
        };
        let index = json!({"api": 1, "styles": [{"id": "synth", "name": "Synth", "file": "synth.pak",
            "size": pack.len(), "crc": format!("{:08x}", crc32(&pack[16..])), "light": false}]});
        vec![
            (
                "preview.json".into(),
                json!({"exe": "clawd_render", "pak": "clawd.pak", "commit": commit, "styleApi": 1})
                    .to_string()
                    .into_bytes(),
            ),
            ("clawd_render".into(), b"program".to_vec()),
            ("clawd.pak".into(), b"pak".to_vec()),
            ("styles/index.json".into(), index.to_string().into_bytes()),
            ("styles/synth.pak".into(), pack),
        ]
    }

    fn zip_of(files: &[(String, Vec<u8>)]) -> Vec<u8> {
        make_zip(
            &files
                .iter()
                .map(|(n, d)| (n.as_str(), d.as_slice()))
                .collect::<Vec<_>>(),
        )
    }

    const SHA: &str = "abababababababababababababababababababababababababababababababab";
    const SHA2: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";

    #[test]
    fn unpack_then_load_and_every_file_is_checked() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("r");
        let zip = zip_of(&good_files("c000006"));
        let folder = unpack_renderer(&zip, &root, SHA).unwrap();
        assert_eq!(folder, root.join(&SHA[..16]));
        let r = load_unpacked(&root, SHA, "").unwrap();
        assert_eq!(
            (
                r.styles.keys().cloned().collect::<Vec<_>>(),
                r.commit.as_str()
            ),
            (vec!["synth".to_owned()], "c000006")
        );
        assert!(
            load_unpacked(&root, SHA2, "").is_none(),
            "another zip: not unpacked"
        );
        assert!(
            load_unpacked(&root, SHA, "zzzzzzz").is_none(),
            "built from another firmware"
        );
        std::fs::write(folder.join("styles").join("synth.pak"), b"changed").unwrap();
        assert!(
            load_unpacked(&root, SHA, "").is_none(),
            "a changed file: unpack again"
        );
        unpack_renderer(&zip, &root, SHA).unwrap();
        assert!(load_unpacked(&root, SHA, "").is_some());
    }

    #[test]
    fn unpacking_a_newer_renderer_drops_the_older() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("r");
        unpack_renderer(&zip_of(&good_files("c000006")), &root, SHA).unwrap();
        unpack_renderer(&zip_of(&good_files("c000007")), &root, SHA2).unwrap();
        let names: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [&SHA2[..16]]);
    }

    #[test]
    fn a_zip_that_is_not_a_renderer_is_refused_and_writes_nothing() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("r");
        let with = |extra: &str| {
            let mut f = good_files("c1");
            f.push((extra.to_owned(), b"x".to_vec()));
            zip_of(&f)
        };
        let without = |name: &str| {
            zip_of(
                &good_files("c1")
                    .into_iter()
                    .filter(|(n, _)| n != name)
                    .collect::<Vec<_>>(),
            )
        };
        let preview = |v: Value| {
            let mut f = good_files("c1");
            f[0].1 = v.to_string().into_bytes();
            zip_of(&f)
        };
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("..", with("../evil")),
            ("abs", with("/abs")),
            ("dotdot-in-styles", with("styles/../../x.pak")),
            ("other-dll", with("other.dll")),
            ("backslash", with("styles\\x.pak")),
            ("backslash-dotdot", with("..\\evil")),
            ("windows-drive", with("C:/x.pak")),
            ("no-preview", without("preview.json")),
            ("no-pak", without("clawd.pak")),
            ("not-json", {
                let mut f = good_files("c1");
                f[0].1 = b"not json".to_vec();
                zip_of(&f)
            }),
            (
                "exe-is-pak",
                preview(json!({"exe": "clawd.pak", "pak": "clawd.pak"})),
            ),
            (
                "exe-not-in-zip",
                preview(json!({"exe": "clawd_render.exe", "pak": "clawd.pak"})),
            ),
            ("not-a-zip", b"PK nope".to_vec()),
        ];
        for (name, z) in cases {
            let e = unpack_renderer(&z, &root, SHA);
            assert!(matches!(e, Err(PreviewError::Invalid(_))), "{name}: {e:?}");
            assert!(
                !root.exists() || std::fs::read_dir(&root).unwrap().next().is_none(),
                "{name} wrote something"
            );
        }
        assert!(unpack_renderer(&zip_of(&good_files("c1")), &root, "../x").is_err());
    }

    #[test]
    fn a_marker_naming_a_file_outside_the_folder_is_not_trusted() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("r");
        let folder = unpack_renderer(&zip_of(&good_files("c1")), &root, SHA).unwrap();
        let mut mark: Value =
            serde_json::from_slice(&std::fs::read(folder.join(MARK)).unwrap()).unwrap();
        mark["files"]["../outside"] = json!("00".repeat(32));
        std::fs::write(folder.join(MARK), mark.to_string()).unwrap();
        assert!(load_unpacked(&root, SHA, "").is_none());
    }

    #[test]
    fn a_renderer_for_another_render_api_is_not_run() {
        let t = tempfile::tempdir().unwrap();
        let folder = unpack_renderer(&zip_of(&good_files("c1")), &t.path().join("r"), SHA).unwrap();
        assert!(load_renderer(&Env::empty(), Some(&folder), "").is_some());
        let mut m: Value =
            serde_json::from_slice(&std::fs::read(folder.join("preview.json")).unwrap()).unwrap();
        m["renderApi"] = json!(RENDER_API + 1);
        std::fs::write(folder.join("preview.json"), m.to_string()).unwrap();
        assert!(load_renderer(&Env::empty(), Some(&folder), "").is_none());
    }

    #[test]
    fn load_renderer_needs_every_file_and_trusts_only_its_commit() {
        let t = tempfile::tempdir().unwrap();
        assert!(load_renderer(&Env::empty(), Some(&t.path().join("nothing")), "").is_none());
        let folder =
            unpack_renderer(&zip_of(&good_files("aaaaaaa")), &t.path().join("r"), SHA).unwrap();
        assert!(load_renderer(&Env::empty(), Some(&folder), "bbbbbbb").is_none());
        assert!(load_renderer(&Env::empty(), Some(&folder), "aaaaaaa").is_some());
        assert!(load_renderer(&Env::empty(), Some(&folder), "").is_some());
        std::fs::remove_file(folder.join("styles/index.json")).unwrap();
        assert!(load_renderer(&Env::empty(), Some(&folder), "")
            .unwrap()
            .styles
            .is_empty());
        std::fs::remove_file(folder.join("clawd.pak")).unwrap();
        assert!(load_renderer(&Env::empty(), Some(&folder), "").is_none());
    }

    #[test]
    fn preview_dir_prefers_the_environment() {
        let env = Env::from_fn(|k| (k == "CLAWD_PREVIEW_DIR").then(|| "/x/pv".to_owned()));
        assert_eq!(
            preview_dir_in(&env, Path::new("/res")),
            PathBuf::from("/x/pv")
        );
        assert_eq!(
            preview_dir_in(&Env::empty(), Path::new("/res")),
            Path::new("/res").join("renderer")
        );
    }

    #[test]
    fn a_zip_over_its_limits_is_refused() {
        let mut files = good_files("c1");
        for i in 0..=UNPACK_FILES {
            files.push((format!("styles/s{i}.pak"), vec![1]));
        }
        assert!(unpack_renderer(
            &zip_of(&files),
            &tempfile::tempdir().unwrap().path().join("r"),
            SHA
        )
        .is_err());
    }
}
