//! Settings commands: firmware and packs (`settings_api.py`): the facts, the remote style packs
//! check and the background install (the page polls `job` meanwhile). The firmware itself only
//! ever comes with the app.
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clawd_contracts::ClockError;
use clawd_firmware::firmware::{self, Bundle};
use clawd_firmware::ota::{self, OtaOptions};
use clawd_firmware::preview::CLASSIC;
use serde_json::{json, Value};

use super::session::{arg, bad_value, diff, err, fail, Bridge, StyleError};
use super::CommandCtx;
use crate::state::lock;

/// The tray must not restart the app during an install: `ota.busy` is touched at most this
/// often while the install reports progress.
const BUSY_BEAT: Duration = Duration::from_secs(5);

/// `SettingsApi.fw`: the firmware/packs facts alone: the page asks again while `stylesBusy` is
/// true, to pick up remote style packs when the background check finishes.
pub async fn fw(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    fw_on(&Bridge::of(ctx), &args).await
}

/// `fw` on a bridge (the command without the Tauri context).
pub async fn fw_on(b: &Bridge, _args: &[Value]) -> Result<Value, String> {
    Ok(json!({"ok": true, "fw": b.clone().fw_info()}))
}

/// `SettingsApi.check_update`: ask for remote style packs now (the Update tab's button); the
/// clock's heap is read again too.
pub async fn check_update(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    check_update_on(&Bridge::of(ctx), &args).await
}

/// `check_update` on a bridge (the command without the Tauri context).
pub async fn check_update_on(b: &Bridge, _args: &[Value]) -> Result<Value, String> {
    if b.job_running() {
        return Ok(fail("busy", "Đang cập nhật rồi."));
    }
    if let Ok(heap) = b.api().heap().await {
        b.data().heap = heap;
    }
    b.check_styles(true).await;
    let heap = b.data().heap.clone();
    Ok(json!({"ok": true, "fw": b.fw_info(), "heap": heap, "packs": b.services.packs.is_some()}))
}

/// `SettingsApi.update(edited, style)`: install the bundled firmware (when newer) and the
/// animation packs that differ in the background, then save `edited` and show `style` (`null`:
/// leave both).
pub async fn update(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    update_on(&Bridge::of(ctx), &args).await
}

/// `update` on a bridge (the command without the Tauri context).
pub async fn update_on(b: &Bridge, args_in: &[Value]) -> Result<Value, String> {
    let args = args_in.to_vec();
    if b.src().is_none() {
        return Ok(fail("bad", "Bản app này không kèm firmware cho đồng hồ."));
    }
    let edited = arg(&args, 0);
    let prepared = (if edited.is_null() {
        Ok(json!({}))
    } else {
        diff(b, edited)
    })
    .and_then(|changes| Ok((changes, b.style_arg(arg(&args, 1))?)));
    let (changes, style) = match prepared {
        Ok(ok) => ok,
        Err(e) => return Ok(bad_value(&e)),
    };
    {
        let mut job = lock(&b.jobs.job);
        if job.get("running").is_some_and(super::session::truthy) {
            return Ok(fail("busy", "Đang cập nhật rồi."));
        }
        *job = json!({
            "running": true, "step": "check", "error": "", "done": false, "started": b.now(),
        });
    }
    b.jobs.busy.store(true, Ordering::SeqCst);
    clawd_store::paths::set_ota_busy(b.paths(), true);
    tokio::spawn(run_update(b.clone(), changes, style));
    Ok(json!({"ok": true}))
}

/// Ends the install job however `run_update` ends: the job stops running, the window may close
/// again and `ota.busy` goes away (Python's `finally`).
struct JobEnd {
    bridge: Bridge,
}

impl Drop for JobEnd {
    fn drop(&mut self) {
        if let Some(job) = lock(&self.bridge.jobs.job).as_object_mut() {
            job.insert("running".into(), json!(false));
        }
        self.bridge.jobs.busy.store(false, Ordering::SeqCst);
        clawd_store::paths::set_ota_busy(self.bridge.paths(), false);
    }
}

fn job_set(b: &Bridge, key: &str, value: Value) {
    if let Some(job) = lock(&b.jobs.job).as_object_mut() {
        job.insert(key.into(), value);
    }
}

/// What ended an install: a clock error keeps the clock's message, the rest is `Lỗi: ...`.
enum UpdateFailure {
    Clock(ClockError),
    Other(String),
}

impl UpdateFailure {
    fn message(&self) -> String {
        match self {
            UpdateFailure::Clock(e) => err(e)["error"].as_str().unwrap_or_default().to_owned(),
            UpdateFailure::Other(e) => format!("Lỗi: {e}"),
        }
    }
}

async fn run_update(b: Bridge, changes: Value, style: Option<String>) {
    let _end = JobEnd { bridge: b.clone() };
    match install(&b, &changes, style.as_deref()).await {
        Ok(()) => {
            job_set(&b, "done", json!(true));
            job_set(&b, "step", json!("done"));
        }
        Err(e) => job_set(&b, "error", json!(e.message())),
    }
}

/// `_run_update`'s body.
async fn install(b: &Bridge, changes: &Value, style: Option<&str>) -> Result<(), UpdateFailure> {
    let beat: Arc<Mutex<Option<Instant>>> = Arc::default();
    let step: Arc<dyn Fn(&str) + Send + Sync> = {
        let (b, beat) = (b.clone(), Arc::clone(&beat));
        Arc::new(move |s: &str| {
            job_set(&b, "step", json!(s));
            let mut last = lock(&beat);
            if last.is_none_or(|at| at.elapsed() > BUSY_BEAT) {
                *last = Some(Instant::now());
                clawd_store::paths::set_ota_busy(b.paths(), true); // the tray must not restart the app now
            }
        })
    };
    let mut opts = OtaOptions::new(b.paths().clone());
    opts.progress = Some(Arc::clone(&step));
    opts.settle = b.data().ota_settle;

    let src = b
        .src()
        .ok_or_else(|| UpdateFailure::Other("no firmware to install".into()))?;
    let need = if style.is_some() { src.style_api } else { 0 };
    let api = b.api();
    let heap = ota::install_bundle(&*api, &src, changes, need, &opts)
        .await
        .map_err(UpdateFailure::Clock)?;
    b.data().heap = heap.clone();
    let shown = firmware::clock_style(&heap);
    match style {
        Some(s)
            if s != if shown.is_empty() {
                CLASSIC
            } else {
                shown.as_str()
            } =>
        {
            b.apply_style(s).await.map_err(|e| match e {
                StyleError::Clock(e) => UpdateFailure::Clock(e),
                StyleError::Pack(e) => UpdateFailure::Other(e),
            })?;
        }
        _ => refresh_style(b, &src).await,
    }
    if let Ok(cfg) = api.config().await {
        b.data().cfg = cfg;
    }
    let cfg = b.data().cfg.clone();
    b.mirror_screen(&cfg);
    Ok(())
}

/// `_refresh_style(src)`: the clock's style pack, re-sent when the packs shown carry a different
/// build of it. Failures are not the install's.
async fn refresh_style(b: &Bridge, src: &Bundle) {
    let id = firmware::clock_style(&b.data().heap);
    if id.is_empty() {
        return;
    }
    let Some(pack) = src.styles.iter().find(|p| p.id == id) else {
        return;
    };
    let Ok(on_clock) = b.api().style().await else {
        return;
    };
    let crc = on_clock
        .get("crc")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if crc != format!("{:08x}", pack.crc) {
        job_set(b, "step", json!("style"));
        let _ = b.apply_style(&id).await;
    }
}
