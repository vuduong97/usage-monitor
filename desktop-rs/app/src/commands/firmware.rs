//! Settings commands: firmware and packs (`settings_api.py`): the facts, the feed check and the
//! background install (the page polls `job` meanwhile).
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clawd_contracts::ClockError;
use clawd_firmware::feed;
use clawd_firmware::firmware;
use clawd_firmware::ota::{self, OtaOptions};
use clawd_firmware::preview::CLASSIC;
use serde_json::{json, Value};

use super::session::{arg, bad_value, diff, err, fail, Bridge, StyleError};
use super::CommandCtx;
use crate::state::lock;

/// The tray must not restart the app during an install: `ota.busy` is touched at most this
/// often while the install reports progress.
const BUSY_BEAT: Duration = Duration::from_secs(5);

/// `SettingsApi.fw`: the firmware/packs facts alone: the page asks again while `feedBusy` is
/// true, to pick up the feed's release when the background check finishes.
pub async fn fw(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    Ok(json!({"ok": true, "fw": Bridge::of(ctx).fw_info()}))
}

/// `SettingsApi.check_update`: ask the feed now (the Update tab's button); the clock's heap is
/// read again too.
pub async fn check_update(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    if b.job_running() {
        return Ok(fail("busy", "Đang cập nhật rồi."));
    }
    if let Ok(heap) = b.api().heap().await {
        b.data().heap = heap;
    }
    b.check_feed(true).await;
    let heap = b.data().heap.clone();
    Ok(json!({"ok": true, "fw": b.fw_info(), "heap": heap, "feed": b.services.feed.is_some()}))
}

/// `SettingsApi.update(edited, style)`: install the newest firmware and packs (the feed's
/// release, else the bundled firmware) in the background, then save `edited` and show `style`
/// (`null`: leave both).
pub async fn update(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    if b.src().is_none() {
        return Ok(fail(
            "bad",
            "Chưa có bản firmware nào để cài (app không kèm firmware, và chưa đọc được nguồn cập nhật).",
        ));
    }
    let edited = arg(&args, 0);
    let prepared = (if edited.is_null() {
        Ok(json!({}))
    } else {
        diff(&b, edited)
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
    tokio::spawn(run_update(b, changes, style));
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

    let src = b
        .src()
        .ok_or_else(|| UpdateFailure::Other("no firmware to install".into()))?;
    let need = if style.is_some() { src.style_api } else { 0 };
    let api = b.api();
    let from_feed = b.data().release.is_some();
    let heap = match b.services.feed.clone().filter(|_| from_feed) {
        Some(feedsrc) => {
            step("download");
            let now = api.heap().await.map_err(UpdateFailure::Clock)?;
            let mut kinds: Vec<String> = feed::needed(&now, Some(&src));
            kinds.extend(feed::STYLE_KINDS.iter().map(|k| (*k).to_owned()));
            let mut seen = std::collections::HashSet::new();
            kinds.retain(|k| seen.insert(k.clone()));
            let kinds: Vec<&str> = kinds.iter().map(String::as_str).collect();
            let progress = {
                let step = Arc::clone(&step);
                move |_: &str, _: u64, _: u64| step("download")
            };
            let fetched = feedsrc
                .fetch(&src, &kinds, Some(&progress))
                .await
                .map_err(|e| UpdateFailure::Other(e.to_string()))?;
            let fetched = Arc::new(fetched);
            b.data().release = Some(Arc::clone(&fetched));
            ota::install_release(&api, &fetched, changes, need, &opts).await
        }
        None => ota::install_then_save(&api, &src.bundle, changes, need, &opts).await,
    }
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

/// `_refresh_style(src)`: the clock's style pack, re-sent when the release carries a different
/// build of it. Failures are not the install's.
async fn refresh_style(b: &Bridge, src: &feed::Release) {
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
