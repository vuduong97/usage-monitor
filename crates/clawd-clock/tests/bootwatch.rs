//! Port of the `BootWatch` half of `tests/test_bootwatch.py` (the `Publisher.resend` tests belong
//! to the usage phase) against the testkit's fake clock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_clock::{BootWatch, OnRestart};
use clawd_contracts::{new_stop, TargetsFn};
use clawd_testkit::clock::FakeClockDevice;
use clawd_testkit::FakeClock;

struct Rig {
    device: FakeClockDevice,
    now: FakeClock,
    hits: Arc<Mutex<Vec<(String, u64)>>>,
    delivered: Arc<AtomicBool>,
    watch: BootWatch,
    url: String,
}

fn rig(uptime: u64, targets: bool) -> Rig {
    let device = FakeClockDevice::start(None);
    device.set_uptime(Some(uptime));
    let url = device.usage_url();
    let now = FakeClock::new(1000.0);
    let hits: Arc<Mutex<Vec<(String, u64)>>> = Arc::default();
    let delivered = Arc::new(AtomicBool::new(true));
    let on_restart: OnRestart = {
        let hits = Arc::clone(&hits);
        let delivered = Arc::clone(&delivered);
        Arc::new(move |url, up| {
            hits.lock().unwrap().push((url, up));
            let ok = delivered.load(Ordering::SeqCst);
            Box::pin(async move { ok })
        })
    };
    let listed = if targets {
        vec![url.clone()]
    } else {
        Vec::new()
    };
    let targets: TargetsFn = Arc::new(move || listed.clone());
    let watch = BootWatch::new(targets, on_restart, now.arc());
    Rig {
        device,
        now,
        hits,
        delivered,
        watch,
        url,
    }
}

impl Rig {
    fn hits(&self) -> Vec<(String, u64)> {
        self.hits.lock().unwrap().clone()
    }
}

#[tokio::test]
async fn uptime_drop_is_a_restart_reported_once() {
    let mut r = rig(5000, true);
    r.watch.step().await; // first look, booted long ago: not a restart
    r.now.advance(5.0);
    r.device.set_uptime(Some(5005));
    r.watch.step().await; // still running
    assert!(r.hits().is_empty());
    r.now.advance(40.0);
    r.device.set_uptime(Some(12)); // rebooted
    let restarted = r.watch.step().await;
    assert_eq!(restarted, [r.url.clone()]);
    assert_eq!(r.hits(), [(r.url.clone(), 12)]);
    r.now.advance(5.0);
    r.device.set_uptime(Some(17));
    r.watch.step().await; // same boot: no second report
    assert_eq!(r.hits(), [(r.url.clone(), 12)]);
}

#[tokio::test]
async fn slow_answers_are_not_restarts() {
    // Request latency shifts the boot estimate a little; that is not a restart.
    let mut r = rig(5000, true);
    r.watch.step().await;
    r.now.advance(20.0); // a slow reply: 20 s passed, uptime +8
    r.device.set_uptime(Some(5008));
    r.watch.step().await;
    assert!(r.hits().is_empty());
}

#[tokio::test]
async fn unreachable_then_back_after_reboot() {
    let mut r = rig(5000, true);
    r.watch.step().await;
    r.device.set_uptime(None); // rebooting: no answer
    r.now.advance(5.0);
    r.watch.step().await;
    assert!(r.hits().is_empty());
    r.device.set_uptime(Some(9));
    r.now.advance(30.0);
    r.watch.step().await;
    assert_eq!(r.hits(), [(r.url.clone(), 9)]);
}

#[tokio::test]
async fn new_address_that_just_booted_counts() {
    // DHCP gave the rebooted clock a new IP: first look at it, freshly booted.
    let mut r = rig(30, true);
    r.watch.step().await;
    assert_eq!(r.hits(), [(r.url.clone(), 30)]);
}

#[tokio::test]
async fn failed_delivery_retries_on_next_probe_only_while_fresh() {
    let mut r = rig(5000, true);
    r.delivered.store(false, Ordering::SeqCst);
    r.watch.step().await;
    r.now.advance(40.0);
    r.device.set_uptime(Some(10));
    r.watch.step().await; // restart seen, data not taken
    r.now.advance(5.0);
    r.device.set_uptime(Some(15));
    r.delivered.store(true, Ordering::SeqCst);
    r.watch.step().await; // retried, delivered
    r.now.advance(5.0);
    r.device.set_uptime(Some(20));
    r.watch.step().await; // done: no more retries
    assert_eq!(r.hits(), [(r.url.clone(), 10), (r.url.clone(), 15)]);
}

#[tokio::test]
async fn retries_stop_after_the_fresh_window() {
    let mut r = rig(100, true);
    r.delivered.store(false, Ordering::SeqCst);
    r.watch.step().await; // new clock, booted 100 s ago: try
    r.now.advance(30.0);
    r.device.set_uptime(Some(130));
    r.watch.step().await; // past FRESH_BOOT: leave it to the poll
    assert_eq!(r.hits(), [(r.url.clone(), 100)]);
}

#[tokio::test]
async fn no_targets_no_probe() {
    let mut r = rig(5000, false);
    assert!(r.watch.step().await.is_empty());
    assert!(r.device.calls().is_empty());
    assert!(r.hits().is_empty());
}

#[tokio::test]
async fn a_target_that_goes_away_is_forgotten() {
    let device = FakeClockDevice::start(None);
    device.set_uptime(Some(5000));
    let url = device.usage_url();
    let listed = Arc::new(Mutex::new(vec![url.clone()]));
    let targets: TargetsFn = {
        let listed = Arc::clone(&listed);
        Arc::new(move || listed.lock().unwrap().clone())
    };
    let hits: Arc<Mutex<Vec<u64>>> = Arc::default();
    let on_restart: OnRestart = {
        let hits = Arc::clone(&hits);
        Arc::new(move |_, up| {
            hits.lock().unwrap().push(up);
            Box::pin(async { true })
        })
    };
    let now = FakeClock::new(1000.0);
    let mut watch = BootWatch::new(targets, on_restart, now.arc());
    watch.step().await;
    listed.lock().unwrap().clear();
    watch.step().await; // forgotten
    listed.lock().unwrap().push(url);
    device.set_uptime(Some(20)); // back, freshly booted: counts as a first look
    watch.step().await;
    assert_eq!(*hits.lock().unwrap(), [20]);
}

#[tokio::test]
async fn non_json_or_error_answers_are_no_answer() {
    let mut r = rig(5000, true);
    r.device
        .override_path("/api/heap", clawd_testkit::FakeResponse::status(500));
    r.watch.step().await; // no estimate stored; nothing fires
    r.device.override_path(
        "/api/heap",
        clawd_testkit::FakeResponse::text(200, "<html>"),
    );
    assert!(r.watch.step().await.is_empty());
}

#[tokio::test]
async fn run_probes_until_stopped() {
    let r = rig(10, true); // a fresh boot: the first probe reports it
    let hits = Arc::clone(&r.hits);
    let (stop_tx, stop_rx) = new_stop();
    let task = tokio::spawn(r.watch.run(stop_rx));
    for _ in 0..100 {
        if !hits.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(hits.lock().unwrap().len(), 1);
    stop_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    drop(r.device);
}
