//! Install the bundled firmware on the clock, then (optionally) the settings that were waiting
//! for it, e.g. a style pack the old image cannot draw (`ota.py`).
//!
//! Order matters: a pack is sent only after the new image is up and says it draws that style
//! API (an old image has no `/api/style` at all). Hold the engine's pushes with
//! `clawd_store::pause::{hold, release}` **for the whole operation, including on error or panic**
//! (a `Drop` guard), renewing the lease on every `wait_back` tick (`ota.py:28-37, 84-100`).
//! This module does **not** set `ota.busy`: the settings job does (phase 13), `settings_api.py:
//! 612-644`.
//!
//! Do not "improve" the sequence: a different order can brick a clock.
//!
//! # Time model
//!
//! Every wait is tokio time: [`OtaOptions::settle`], the 5 s gap between pack retries and the 3 s
//! poll of [`install_packs`] are `tokio::time::sleep`, and the monotonic clock handed to
//! `ClockApi::wait_back` and used for the `CRC_WAIT` deadline is [`OtaOptions::clock`] (default
//! `MonotonicClock`, which is also built on tokio's `Instant`). Tests therefore call
//! `tokio::time::pause()` and the whole sequence runs in virtual time. A hand-driven `FakeClock`
//! does not follow tokio time: never combine it with paused tokio time in one wait.

use std::future::Future;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use clawd_clock::ClockApi;
use clawd_contracts::{AppPaths, Clock, ClockError, MonotonicClock};
use serde_json::Value;

use crate::firmware::{self, Bundle};
use crate::packs::{pack_state, PACK_KINDS};

/// `ota.SETTLE`: let a push the engine already started finish before the upload.
pub const SETTLE: Duration = Duration::from_secs(6);
/// `ota.LEASE` (seconds): push pause, renewed while we wait.
pub const LEASE: f64 = 60.0;
/// `ota.UPLOAD_LEASE` (seconds): longer than `clawd_clock::api::UPLOAD_TIMEOUT`.
pub const UPLOAD_LEASE: f64 = 200.0;
/// `ota.CRC_WAIT`: a clock checksums packs it did not receive itself in the background.
pub const CRC_WAIT: Duration = Duration::from_secs(150);
/// `ota.PACK_TRIES`.
pub const PACK_TRIES: u32 = 2;

/// Pause between two `/api/heap` polls while the clock checksums a pack (`sleep(3.0)`).
const CRC_POLL: Duration = Duration::from_secs(3);
/// Pause before the second try of a pack upload that failed (`sleep(5.0)`).
const PACK_RETRY_GAP: Duration = Duration::from_secs(5);

/// `progress(step)`: `check`, `pause`, `upload`, `reboot`, `done`, or a pack kind.
pub type Progress = Arc<dyn Fn(&str) + Send + Sync>;
/// `hold(seconds, reason)`: take or renew the push pause.
pub type HoldFn = Arc<dyn Fn(f64, &str) -> io::Result<()> + Send + Sync>;
/// `release()`: end the push pause.
pub type ReleaseFn = Arc<dyn Fn() -> io::Result<()> + Send + Sync>;

/// The `progress` / `hold` / `release` / `settle` / `clock` keyword arguments of the Python
/// functions. See the module docs for the time model.
///
/// `#[non_exhaustive]`: build it with [`OtaOptions::new`] and assign the fields you override.
#[derive(Clone)]
#[non_exhaustive]
pub struct OtaOptions {
    /// Where `pause.json` lives (`clawd_store::pause`).
    pub paths: AppPaths,
    pub progress: Option<Progress>,
    /// Override [`SETTLE`] (tests).
    pub settle: Option<Duration>,
    /// Monotonic clock for `wait_back` and the CRC deadline; `None` = `MonotonicClock`.
    pub clock: Option<Arc<dyn Clock>>,
    /// Override the pause lease writer (`hold=`); `None` = `clawd_store::pause::hold(paths, ..)`.
    pub hold: Option<HoldFn>,
    /// Override the pause lease remover (`release=`); `None` = `clawd_store::pause::release`.
    pub release: Option<ReleaseFn>,
}

impl OtaOptions {
    /// Defaults: no progress callback, [`SETTLE`], `MonotonicClock`, the real `pause.json` under
    /// `paths`.
    pub fn new(paths: AppPaths) -> Self {
        Self {
            paths,
            progress: None,
            settle: None,
            clock: None,
            hold: None,
            release: None,
        }
    }

    fn say(&self, step: &str) {
        if let Some(p) = &self.progress {
            p(step);
        }
    }

    fn hold_lease(&self, seconds: f64, reason: &str) -> io::Result<()> {
        match &self.hold {
            Some(h) => h(seconds, reason),
            None => clawd_store::pause::hold(&self.paths, seconds, reason),
        }
    }

    fn clock(&self) -> Arc<dyn Clock> {
        self.clock
            .clone()
            .unwrap_or_else(|| Arc::new(MonotonicClock::new()))
    }

    /// A guard that releases the pause when dropped: on success, on `Err`, on a panic and when
    /// the future is dropped.
    fn guard(&self) -> PauseGuard {
        PauseGuard {
            opts: self.clone_for_release(),
        }
    }

    fn clone_for_release(&self) -> ReleaseTarget {
        ReleaseTarget {
            paths: self.paths.clone(),
            release: self.release.clone(),
        }
    }
}

struct ReleaseTarget {
    paths: AppPaths,
    release: Option<ReleaseFn>,
}

/// Releases the push pause on drop (`finally: release()`).
struct PauseGuard {
    opts: ReleaseTarget,
}

impl Drop for PauseGuard {
    fn drop(&mut self) {
        let res = match &self.opts.release {
            Some(r) => r(),
            None => clawd_store::pause::release(&self.opts.paths),
        };
        if let Err(e) = res {
            tracing::warn!("ota: could not release the push pause: {e}");
        }
    }
}

/// What OTA needs from the clock: the subset of [`ClockApi`] it calls. [`ClockApi`] implements
/// it; tests drive the sequence with a scripted fake.
#[allow(async_fn_in_trait)]
pub trait ClockOps {
    async fn heap(&self) -> Result<Value, ClockError>;
    async fn upload_firmware(&self, image: &[u8]) -> Result<(), ClockError>;
    async fn upload_pack(&self, kind: &str, data: &[u8]) -> Result<Value, ClockError>;
    async fn save(&self, changes: &Value) -> Result<Value, ClockError>;
    async fn wait_back(
        &self,
        booted_before: f64,
        clock: &dyn Clock,
        tick: &(dyn Fn() + Send + Sync),
    ) -> Result<Value, ClockError>;
}

impl ClockOps for ClockApi {
    fn heap(&self) -> impl Future<Output = Result<Value, ClockError>> {
        ClockApi::heap(self)
    }
    fn upload_firmware(&self, image: &[u8]) -> impl Future<Output = Result<(), ClockError>> {
        ClockApi::upload_firmware(self, image)
    }
    fn upload_pack(
        &self,
        kind: &str,
        data: &[u8],
    ) -> impl Future<Output = Result<Value, ClockError>> {
        ClockApi::upload_pack(self, kind, data)
    }
    fn save(&self, changes: &Value) -> impl Future<Output = Result<Value, ClockError>> {
        ClockApi::save(self, changes)
    }
    fn wait_back(
        &self,
        booted_before: f64,
        clock: &dyn Clock,
        tick: &(dyn Fn() + Send + Sync),
    ) -> impl Future<Output = Result<Value, ClockError>> {
        ClockApi::wait_back(self, booted_before, None, clock, Some(tick))
    }
}

fn other(msg: impl Into<String>) -> ClockError {
    ClockError::Other(msg.into())
}

fn pause_error(e: io::Error) -> ClockError {
    other(format!("cannot hold the push pause: {e}"))
}

/// `ota.install(api, bundle, ...)`: push the bundle; returns the clock's `/api/heap` after it
/// came back. `ClockError` ("no update to install (<state>)", "the clock came back on its old
/// firmware") otherwise.
pub async fn install<A: ClockOps + ?Sized>(
    api: &A,
    bundle: &Bundle,
    opts: &OtaOptions,
) -> Result<Value, ClockError> {
    let image = bundle.read().map_err(other)?;
    opts.say("check");
    let heap = api.heap().await?;
    let new = install_image(api, bundle, image, &heap, opts).await?;
    opts.say("done");
    Ok(new)
}

/// The body of `ota.install` once the image bytes and the clock's heap are in hand.
async fn install_image<A: ClockOps + ?Sized>(
    api: &A,
    bundle: &Bundle,
    image: Vec<u8>,
    heap: &Value,
    opts: &OtaOptions,
) -> Result<Value, ClockError> {
    let state = firmware::update_state(heap, Some(bundle));
    if state != "available" {
        return Err(other(format!("no update to install ({state})")));
    }
    opts.hold_lease(LEASE, "firmware update")
        .map_err(pause_error)?;
    let clock = opts.clock();
    let new = {
        let _pause = opts.guard();
        opts.say("pause");
        tokio::time::sleep(opts.settle.unwrap_or(SETTLE)).await;
        opts.hold_lease(UPLOAD_LEASE, "firmware update")
            .map_err(pause_error)?; // the upload blocks: cover all of it
        opts.say("upload");
        let started = clock.now();
        api.upload_firmware(&image).await?;
        opts.say("reboot");
        let renew = || {
            if let Err(e) = opts.hold_lease(LEASE, "firmware update") {
                tracing::warn!("ota: could not renew the push pause: {e}");
            }
        };
        api.wait_back(started, clock.as_ref(), &renew).await?
    };
    if firmware::clock_seq_signed(&new) < i128::from(bundle.seq) {
        return Err(other("the clock came back on its old firmware"));
    }
    Ok(new)
}

/// `ota._wait_crcs(api, heap, bundle, ...)`: a heap in which no pack of the bundle is still
/// being checksummed (or the last one after [`CRC_WAIT`]: those packs are then sent anyway).
async fn wait_crcs<A: ClockOps + ?Sized>(
    api: &A,
    mut heap: Value,
    bundle: &Bundle,
    opts: &OtaOptions,
) -> Value {
    let clock = opts.clock();
    let end = clock.now() + CRC_WAIT.as_secs_f64();
    let waiting = |heap: &Value| {
        PACK_KINDS.iter().any(|k| {
            bundle
                .pack(k)
                .is_some_and(|p| pack_state(heap, k, &p.crc) == "wait")
        })
    };
    while waiting(&heap) && clock.now() < end {
        tokio::time::sleep(CRC_POLL).await;
        if let Ok(h) = api.heap().await {
            heap = h;
        }
    }
    heap
}

/// `ota.install_packs(api, bundle, heap, ...)`: send the bundle's `clawd.pak`, then `desk.pak`,
/// where the clock's differs (CRC in `/api/heap`), after waiting out any checksum in progress.
/// One retry each. Returns the clock's heap afterwards.
pub async fn install_packs<A: ClockOps + ?Sized>(
    api: &A,
    bundle: &Bundle,
    heap: Value,
    opts: &OtaOptions,
) -> Result<Value, ClockError> {
    let heap = wait_crcs(api, heap, bundle, opts).await;
    let mut todo: Vec<(&str, Vec<u8>, &str)> = Vec::new();
    for kind in PACK_KINDS {
        let Some(p) = bundle.pack(kind) else {
            continue;
        };
        if pack_state(&heap, kind, &p.crc) != "ok" {
            todo.push((kind, p.read().map_err(other)?, &p.crc));
        }
    }
    if todo.is_empty() {
        return Ok(heap);
    }
    {
        let _pause = opts.guard();
        for (kind, data, crc) in &todo {
            opts.say(kind);
            for attempt in 0..PACK_TRIES {
                let last = attempt + 1 == PACK_TRIES;
                opts.hold_lease(UPLOAD_LEASE * 3.0, "pack update")
                    .map_err(pause_error)?;
                let r = match api.upload_pack(kind, data).await {
                    Ok(r) => r,
                    Err(e) if last => return Err(e),
                    Err(_) => {
                        tokio::time::sleep(PACK_RETRY_GAP).await;
                        continue;
                    }
                };
                match r.get("crc").and_then(Value::as_str) {
                    Some(got) if !got.is_empty() && got != *crc => {
                        if last {
                            return Err(other(format!(
                                "{kind} arrived damaged (CRC {got}, want {crc})"
                            )));
                        }
                    }
                    _ => break,
                }
            }
        }
    }
    Ok(api.heap().await.unwrap_or(heap))
}

/// `changes` is "truthy" in Python: a non-empty object (or any other non-empty value).
fn has_changes(changes: &Value) -> bool {
    crate::pyval::truthy(changes)
}

fn style_api_missing(heap: &Value, need_style_api: u32) -> bool {
    need_style_api != 0 && firmware::clock_style_api(heap) != need_style_api
}

/// Everything the bundle brings that this clock lacks: the firmware (when newer), then the
/// animation packs that differ, then POST `changes`. `ClockError` when the clock afterwards does
/// not draw style packs of API `need_style_api` (0: no need). Returns the clock's heap.
pub async fn install_bundle<A: ClockOps + ?Sized>(
    api: &A,
    bundle: &Bundle,
    changes: &Value,
    need_style_api: u32,
    opts: &OtaOptions,
) -> Result<Value, ClockError> {
    opts.say("check");
    let mut heap = api.heap().await?;
    if firmware::update_state(&heap, Some(bundle)) == "available" {
        let image = bundle.read().map_err(other)?;
        heap = install_image(api, bundle, image, &heap, opts).await?;
    }
    heap = install_packs(api, bundle, heap, opts).await?;
    if style_api_missing(&heap, need_style_api) {
        return Err(other("the new firmware does not draw these style packs"));
    }
    if has_changes(changes) {
        api.save(changes).await?;
    }
    opts.say("done");
    Ok(heap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc32::crc32_hex;
    use serde_json::json;
    use std::sync::Mutex;

    const IMAGE_LEN: usize = 1000;

    fn heap(seq: u64, extra: Value) -> Value {
        let mut h = json!({"variant": "esp8266-clawd", "fwSeq": seq, "layouts": 1, "styleApi": 1,
                           "pak": 17, "desk": 31, "uptime": 100});
        for (k, v) in extra.as_object().unwrap() {
            h[k] = v.clone();
        }
        h
    }

    const CLAWD: &[u8] = b"clawd-pack-bytes";
    const DESK: &[u8] = b"desk-pack-bytes-longer";

    /// The clock's side of an install: `/update`, then a reboot into `after`; packs.
    struct FakeClock {
        state: Mutex<Inner>,
    }
    struct Inner {
        h: Value,
        after: Value,
        log: Vec<String>,
        fail_uploads: u32,
        bad_crc: u32,
        polls: u32,
        panic_on_upload: bool,
    }

    impl FakeClock {
        fn new(before: Value, after: Value) -> Self {
            Self {
                state: Mutex::new(Inner {
                    h: before,
                    after,
                    log: Vec::new(),
                    fail_uploads: 0,
                    bad_crc: 0,
                    polls: 0,
                    panic_on_upload: false,
                }),
            }
        }
        fn log(&self) -> Vec<String> {
            self.state.lock().unwrap().log.clone()
        }
        fn with(self, f: impl FnOnce(&mut Inner)) -> Self {
            f(&mut self.state.lock().unwrap());
            self
        }
    }

    impl ClockOps for FakeClock {
        async fn heap(&self) -> Result<Value, ClockError> {
            let mut s = self.state.lock().unwrap();
            s.polls += 1;
            if s.polls == 3 {
                // the background checksum finished meanwhile
                let after = s.after.clone();
                for (k, v) in after.as_object().unwrap() {
                    if k.ends_with("Crc") {
                        s.h[k] = v.clone();
                    }
                }
            }
            Ok(s.h.clone())
        }
        async fn upload_firmware(&self, image: &[u8]) -> Result<(), ClockError> {
            let mut s = self.state.lock().unwrap();
            assert!(!s.panic_on_upload, "boom");
            s.log.push(format!("fw {}", image.len()));
            Ok(())
        }
        async fn upload_pack(&self, kind: &str, data: &[u8]) -> Result<Value, ClockError> {
            let mut s = self.state.lock().unwrap();
            if s.fail_uploads > 0 {
                s.fail_uploads -= 1;
                return Err(ClockError::Other("no answer".into()));
            }
            s.log.push(format!("{kind} {}", data.len()));
            let mut got = crc32_hex(data);
            if s.bad_crc > 0 {
                s.bad_crc -= 1;
                got = "00000000".into();
            }
            let key = if kind == "clawdpak" {
                "pakCrc"
            } else {
                "deskCrc"
            };
            s.h[key] = json!(got);
            Ok(json!({"ok": true, "anims": 17, "crc": got}))
        }
        async fn save(&self, changes: &Value) -> Result<Value, ClockError> {
            self.state
                .lock()
                .unwrap()
                .log
                .push(format!("save {changes}"));
            Ok(json!({"ok": true}))
        }
        async fn wait_back(
            &self,
            _started: f64,
            _clock: &dyn Clock,
            tick: &(dyn Fn() + Send + Sync),
        ) -> Result<Value, ClockError> {
            tick();
            let mut s = self.state.lock().unwrap();
            let mut h = s.after.clone();
            h["pakCrc"] = json!("");
            h["deskCrc"] = json!(""); // just booted: CRCs not known yet
            s.h = h.clone();
            Ok(h)
        }
    }

    #[derive(Default, Clone)]
    struct Recorder {
        holds: Arc<Mutex<Vec<(f64, String)>>>,
        releases: Arc<Mutex<u32>>,
        steps: Arc<Mutex<Vec<String>>>,
    }

    impl Recorder {
        fn opts(&self) -> OtaOptions {
            let mut o = OtaOptions::new(AppPaths::new("/nonexistent-home"));
            let (holds, releases, steps) = (
                self.holds.clone(),
                self.releases.clone(),
                self.steps.clone(),
            );
            o.hold = Some(Arc::new(move |s, r| {
                holds.lock().unwrap().push((s, r.to_owned()));
                Ok(())
            }));
            o.release = Some(Arc::new(move || {
                *releases.lock().unwrap() += 1;
                Ok(())
            }));
            o.progress = Some(Arc::new(move |s| steps.lock().unwrap().push(s.to_owned())));
            o
        }
        fn releases(&self) -> u32 {
            *self.releases.lock().unwrap()
        }
        fn steps(&self) -> Vec<String> {
            self.steps.lock().unwrap().clone()
        }
    }

    /// A bundle in `dir`: the image, and with `packs` both animation packs.
    fn bundle_in(dir: &std::path::Path, seq: u64, packs: bool) -> Bundle {
        std::fs::create_dir_all(dir).unwrap();
        let image = [&[0xe9u8][..], &[0u8; IMAGE_LEN - 1]].concat();
        std::fs::write(dir.join("fw.bin"), &image).unwrap();
        let mut files = Vec::new();
        if packs {
            for (kind, name, data) in [
                ("clawdpak", "clawd.pak", CLAWD),
                ("deskpak", "desk.pak", DESK),
            ] {
                std::fs::write(dir.join(name), data).unwrap();
                files.push(crate::firmware::PackFile {
                    kind: kind.into(),
                    path: dir.join(name),
                    size: data.len(),
                    crc: crc32_hex(data),
                });
            }
        }
        Bundle {
            path: dir.join("fw.bin"),
            size: image.len(),
            sha256: crate::firmware::sha256_hex(&image),
            variant: "esp8266-clawd".into(),
            seq,
            layouts: 1,
            commit: "abc1234".into(),
            style_api: 0,
            styles: Vec::new(),
            style_rev: 0,
            packs: files,
        }
    }

    fn bundle(dir: &std::path::Path, seq: u64) -> Bundle {
        bundle_in(dir, seq, false)
    }

    fn old() -> Value {
        json!({"version": "2.16.0", "pak": 17, "desk": 31, "uptime": 900})
    }
    fn new1() -> Value {
        json!({"variant": "esp8266-clawd", "fwSeq": 1, "layouts": 1, "pak": 17, "desk": 31, "uptime": 4})
    }
    fn styled() -> Value {
        json!({"variant": "esp8266-clawd", "fwSeq": 2, "layouts": 1, "styleApi": 1, "style": "synth",
               "pak": 17, "desk": 31, "uptime": 4})
    }

    #[tokio::test(start_paused = true)]
    async fn install_pauses_uploads_waits_then_saves() {
        let t = tempfile::tempdir().unwrap();
        let rec = Recorder::default();
        let api = FakeClock::new(old(), new1());
        install_bundle(
            &api,
            &bundle(t.path(), 1),
            &json!({"brightness": 40}),
            0,
            &rec.opts(),
        )
        .await
        .unwrap();
        assert_eq!(
            api.log(),
            [
                format!("fw {IMAGE_LEN}"),
                r#"save {"brightness":40}"#.to_owned()
            ]
        );
        let holds = rec.holds.lock().unwrap().clone();
        assert_eq!(
            holds
                .iter()
                .map(|(s, r)| (*s, r.as_str()))
                .collect::<Vec<_>>(),
            [
                (60.0, "firmware update"),
                (200.0, "firmware update"),
                (60.0, "firmware update")
            ],
            "taken, widened for the upload, renewed on the wait_back tick"
        );
        assert_eq!(rec.releases(), 1);
        assert_eq!(rec.steps(), ["check", "pause", "upload", "reboot", "done"]);
    }

    #[tokio::test(start_paused = true)]
    async fn install_waits_the_settle_time_in_virtual_time() {
        let t = tempfile::tempdir().unwrap();
        let rec = Recorder::default();
        let api = FakeClock::new(old(), new1());
        let start = tokio::time::Instant::now();
        install(&api, &bundle(t.path(), 1), &rec.opts())
            .await
            .unwrap();
        assert_eq!(start.elapsed(), SETTLE);
        let mut o = rec.opts();
        o.settle = Some(Duration::from_secs(1));
        let api = FakeClock::new(old(), new1());
        let start = tokio::time::Instant::now();
        install(&api, &bundle(t.path(), 1), &o).await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_secs(1));
    }

    #[tokio::test(start_paused = true)]
    async fn install_checks_the_new_image_draws_styles() {
        let t = tempfile::tempdir().unwrap();
        let b = bundle(t.path(), 2);
        let rec = Recorder::default();
        let no_styles = json!({"variant": "esp8266-clawd", "fwSeq": 2});
        let e = install_bundle(
            &FakeClock::new(new1(), no_styles),
            &b,
            &json!({}),
            1,
            &rec.opts(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            e.to_string(),
            "the new firmware does not draw these style packs"
        );
        let h = install_bundle(
            &FakeClock::new(new1(), styled()),
            &b,
            &json!({}),
            1,
            &rec.opts(),
        )
        .await
        .unwrap();
        assert_eq!(h["styleApi"], 1);
    }

    #[tokio::test(start_paused = true)]
    async fn install_refuses_when_current_and_releases_on_failure() {
        let t = tempfile::tempdir().unwrap();
        let b = bundle(t.path(), 1);
        let rec = Recorder::default();
        let e = install(&FakeClock::new(new1(), new1()), &b, &rec.opts())
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "no update to install (current)");
        assert!(
            rec.holds.lock().unwrap().is_empty() && rec.releases() == 0,
            "nothing held yet"
        );
        // came back on the old image: held, then released, then the error
        let e = install(&FakeClock::new(old(), old()), &b, &rec.opts())
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "the clock came back on its old firmware");
        assert_eq!(rec.releases(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn the_pause_is_released_when_the_upload_panics() {
        let t = tempfile::tempdir().unwrap();
        let b = bundle(t.path(), 1);
        let rec = Recorder::default();
        let opts = rec.opts();
        let task = tokio::spawn(async move {
            let api = FakeClock::new(old(), new1()).with(|s| s.panic_on_upload = true);
            install(&api, &b, &opts).await
        });
        assert!(task.await.unwrap_err().is_panic());
        assert_eq!(
            rec.releases(),
            1,
            "Drop guard released the lease during unwinding"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_failing_hold_is_an_error_and_a_failing_release_is_not() {
        let t = tempfile::tempdir().unwrap();
        let b = bundle(t.path(), 1);
        let mut o = Recorder::default().opts();
        o.hold = Some(Arc::new(|_, _| Err(io::Error::other("read-only"))));
        let e = install(&FakeClock::new(old(), new1()), &b, &o)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("read-only"), "{e}");
        let mut o = Recorder::default().opts();
        o.release = Some(Arc::new(|| Err(io::Error::other("locked"))));
        install(&FakeClock::new(old(), new1()), &b, &o)
            .await
            .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn install_bundle_firmware_then_only_the_packs_that_differ() {
        let rec = Recorder::default();
        let t = tempfile::tempdir().unwrap();
        let rel = bundle_in(t.path(), 6, true);
        let after = heap(
            6,
            json!({"pakCrc": crc32_hex(CLAWD), "deskCrc": "0badc0de"}),
        );
        let api = FakeClock::new(heap(5, json!({})), after);
        let h = install_bundle(&api, &rel, &json!({"brightness": 30}), 0, &rec.opts())
            .await
            .unwrap();
        assert_eq!(
            api.log(),
            [
                format!("fw {IMAGE_LEN}"),
                format!("deskpak {}", DESK.len()),
                r#"save {"brightness":30}"#.to_owned()
            ]
        );
        assert_eq!(h["deskCrc"], crc32_hex(DESK));
        assert_eq!(h["fwSeq"], 6);
        let steps = rec.steps();
        assert_eq!(steps.first().map(String::as_str), Some("check"));
        assert!(steps.contains(&"upload".to_owned()) && steps.contains(&"deskpak".to_owned()));
        assert_eq!(steps.last().map(String::as_str), Some("done"));
        assert_eq!(
            rec.releases(),
            2,
            "one release per held phase (firmware, packs)"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn install_bundle_packs_only_and_nothing_when_current() {
        let rec = Recorder::default();
        let t = tempfile::tempdir().unwrap();
        let rel = bundle_in(t.path(), 6, true);
        let api = FakeClock::new(
            heap(
                6,
                json!({"desk": 0, "pakCrc": crc32_hex(CLAWD), "deskCrc": ""}),
            ),
            json!({}),
        );
        install_bundle(&api, &rel, &json!({}), 0, &rec.opts())
            .await
            .unwrap();
        assert_eq!(api.log(), [format!("deskpak {}", DESK.len())]);
        let api = FakeClock::new(
            heap(
                6,
                json!({"pakCrc": crc32_hex(CLAWD), "deskCrc": crc32_hex(DESK)}),
            ),
            json!({}),
        );
        install_bundle(&api, &rel, &json!({}), 0, &rec.opts())
            .await
            .unwrap();
        assert!(api.log().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn pack_upload_retries_once_then_fails() {
        let t = tempfile::tempdir().unwrap();
        let rel = bundle_in(t.path(), 6, true);
        let both = |extra: Value| {
            heap(6, json!({"pakCrc": "1", "deskCrc": crc32_hex(DESK)}))
                .as_object()
                .cloned()
                .map(|mut o| {
                    o.extend(extra.as_object().unwrap().clone());
                    Value::Object(o)
                })
                .unwrap()
        };
        let rec = Recorder::default();
        let api = FakeClock::new(both(json!({})), json!({})).with(|s| s.fail_uploads = 1);
        let start = tokio::time::Instant::now();
        install_bundle(&api, &rel, &json!({}), 0, &rec.opts())
            .await
            .unwrap();
        assert_eq!(api.log(), [format!("clawdpak {}", CLAWD.len())]);
        assert_eq!(start.elapsed(), PACK_RETRY_GAP);
        let api = FakeClock::new(both(json!({})), json!({})).with(|s| s.bad_crc = 2);
        let e = install_bundle(&api, &rel, &json!({}), 0, &rec.opts())
            .await
            .unwrap_err();
        assert!(e.to_string().contains("damaged"), "{e}");
        let rec = Recorder::default();
        let api = FakeClock::new(both(json!({})), json!({})).with(|s| s.fail_uploads = 2);
        assert!(install_bundle(&api, &rel, &json!({}), 0, &rec.opts())
            .await
            .is_err());
        assert_eq!(rec.releases(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_checksum_in_progress_is_waited_out_but_only_for_crc_wait() {
        let t = tempfile::tempdir().unwrap();
        let rel = bundle_in(t.path(), 6, true);
        let rec = Recorder::default();
        // polls: install_bundle's first heap = 1, then wait_crcs polls; the 3rd poll reveals CRCs
        let api = FakeClock::new(
            heap(6, json!({"pakCrc": "", "deskCrc": crc32_hex(DESK)})),
            heap(
                6,
                json!({"pakCrc": crc32_hex(CLAWD), "deskCrc": crc32_hex(DESK)}),
            ),
        );
        let start = tokio::time::Instant::now();
        install_bundle(&api, &rel, &json!({}), 0, &rec.opts())
            .await
            .unwrap();
        assert!(
            api.log().is_empty(),
            "the checksum finished: nothing to send"
        );
        assert_eq!(
            start.elapsed(),
            CRC_POLL * 2,
            "the third /api/heap poll shows the CRC"
        );
        // never finishes: after CRC_WAIT the pack is sent anyway
        let api = FakeClock::new(
            heap(6, json!({"pakCrc": "", "deskCrc": crc32_hex(DESK)})),
            json!({}),
        );
        let start = tokio::time::Instant::now();
        let h = heap(6, json!({"pakCrc": "", "deskCrc": crc32_hex(DESK)}));
        let steady = FakeSteady(api);
        install_packs(&steady, &rel, h, &rec.opts()).await.unwrap();
        assert!(start.elapsed() >= CRC_WAIT);
        assert_eq!(steady.0.log(), [format!("clawdpak {}", CLAWD.len())]);
    }

    /// A clock whose checksum never finishes (polls never reveal the CRC).
    struct FakeSteady(FakeClock);
    impl ClockOps for FakeSteady {
        async fn heap(&self) -> Result<Value, ClockError> {
            Ok(heap(6, json!({"pakCrc": "", "deskCrc": crc32_hex(DESK)})))
        }
        async fn upload_firmware(&self, i: &[u8]) -> Result<(), ClockError> {
            self.0.upload_firmware(i).await
        }
        async fn upload_pack(&self, k: &str, d: &[u8]) -> Result<Value, ClockError> {
            self.0.upload_pack(k, d).await
        }
        async fn save(&self, c: &Value) -> Result<Value, ClockError> {
            self.0.save(c).await
        }
        async fn wait_back(
            &self,
            s: f64,
            c: &dyn Clock,
            t: &(dyn Fn() + Send + Sync),
        ) -> Result<Value, ClockError> {
            self.0.wait_back(s, c, t).await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_damaged_bundled_pack_is_an_error_not_sent() {
        let t = tempfile::tempdir().unwrap();
        let rel = bundle_in(t.path(), 6, true);
        std::fs::write(t.path().join("desk.pak"), b"damaged").unwrap();
        let api = FakeClock::new(
            heap(6, json!({"pakCrc": crc32_hex(CLAWD), "deskCrc": "1"})),
            json!({}),
        );
        let e = install_bundle(&api, &rel, &json!({}), 0, &Recorder::default().opts())
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "bundled deskpak does not match its manifest");
        assert!(api.log().is_empty());
    }
}
