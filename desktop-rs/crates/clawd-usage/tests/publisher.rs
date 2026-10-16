//! Ported from `tests/test_http_push.py` (HttpSinks + Publisher without BLE), `tests/test_attention.py`
//! (clears) and the publisher half of `tests/test_engine.py`. BLE tests (`test_http_push.py:52`,
//! `:172`) are dropped with BLE.
//!
//! Config readers come from `clawd-config` (phase 06): until it is merged these tests only pass
//! with the local scratch implementation.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clawd_contracts::{new_stop, TargetsFn, UsagePayload};
use clawd_testkit::usage::FakeClockSink;
use clawd_testkit::{FakeHttpServer, FakeResponse};
use clawd_usage::payload::add_clock_fields;
use clawd_usage::publisher::POLL_INTERVAL;
use clawd_usage::{HttpSinks, PollFn, Publisher};
use common::{payload, real_now, set_mtime, Fx};
use serde_json::{json, Value};

fn sample() -> UsagePayload {
    payload(
        json!({"s": 42, "sr": 133, "w": 67, "wr": 3000, "st": "allowed", "acct": "pro", "ok": true}),
    )
}

fn sinks(fx: &Fx) -> HttpSinks {
    HttpSinks::with_client(fx.env.client.clone(), None)
}

fn targets(urls: &[String]) -> TargetsFn {
    let urls = urls.to_vec();
    Arc::new(move || urls.clone())
}

/// A poller that always answers `(payload, dead)` and counts its calls.
fn canned(result: (Option<UsagePayload>, bool)) -> (PollFn, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let poll: PollFn = Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        let result = result.clone();
        Box::pin(async move { result })
    });
    (poll, calls)
}

fn publisher(fx: &Fx, urls: &[String], poll: PollFn) -> Publisher {
    Publisher::new(fx.env.clone(), sinks(fx), Some(targets(urls))).with_poller(poll)
}

/// The attention files carry real mtimes: run the publisher on the real clock.
fn real_time(fx: &Fx) {
    fx.clock.set(real_now());
}

// ---- clock fields -------------------------------------------------------------------------------

#[test]
fn clock_time_is_sent_by_default() {
    let fx = Fx::new();
    let mut p = UsagePayload::new();
    add_clock_fields(&fx.env, &mut p); // no config file at all
    assert!(p.t().is_some());
    assert!(matches!(p.tf(), Some(12 | 24)));
    fx.config("clock = 12\n");
    let mut p = UsagePayload::new();
    add_clock_fields(&fx.env, &mut p);
    assert_eq!(p.tf(), Some(12));
    fx.config("clock = off\n");
    let mut p = UsagePayload::new();
    add_clock_fields(&fx.env, &mut p);
    assert!(p.is_empty());
}

// ---- HttpSinks -------------------------------------------------------------------------------------

#[tokio::test]
async fn http_sinks_deliver_to_every_target() {
    let fx = Fx::new();
    let (a, b) = (FakeClockSink::start(), FakeClockSink::start());
    let urls = [a.url(), b.url()];
    assert!(sinks(&fx).send(&urls, &sample(), None).await);
    assert_eq!(a.payloads(), vec![json!(sample())]);
    assert_eq!(b.payloads(), vec![json!(sample())]);
}

#[tokio::test]
async fn http_sinks_failure_is_reported() {
    let fx = Fx::new();
    let refusing = FakeClockSink::start_with_status(400);
    assert!(!sinks(&fx).send(&[refusing.url()], &sample(), None).await);
    assert!(
        !sinks(&fx)
            .send(
                &["http://127.0.0.1:1/api/usage".to_owned()],
                &sample(),
                None
            )
            .await
    );
}

#[tokio::test]
async fn http_sinks_one_dead_target_does_not_hide_a_live_one() {
    let fx = Fx::new();
    let live = FakeClockSink::start();
    let urls = ["http://127.0.0.1:1/api/usage".to_owned(), live.url()];
    assert!(sinks(&fx).send(&urls, &sample(), None).await);
}

#[tokio::test]
async fn http_sinks_report_each_push_to_the_hook() {
    let fx = Fx::new();
    let (live, refusing) = (
        FakeClockSink::start(),
        FakeClockSink::start_with_status(500),
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let hook: clawd_usage::ResultHook =
        Arc::new(move |url, ok| log.lock().unwrap().push((url.to_owned(), ok)));
    let sinks = HttpSinks::with_client(fx.env.client.clone(), Some(hook));
    sinks
        .send(&[live.url(), refusing.url()], &sample(), None)
        .await;
    let mut seen = seen.lock().unwrap().clone();
    seen.sort();
    let mut want = vec![(live.url(), true), (refusing.url(), false)];
    want.sort();
    assert_eq!(seen, want);
}

#[tokio::test]
async fn http_sinks_body_is_compact_ascii_json_in_key_order() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    let mut p = sample();
    p.insert("rd".into(), json!("Đồng"));
    sinks(&fx).send(&[clock.url()], &p, None).await;
    assert_eq!(
        clock.raw_bodies(),
        [
            r#"{"s":42,"sr":133,"w":67,"wr":3000,"st":"allowed","acct":"pro","ok":true,"rd":"\u0110\u1ed3ng"}"#
        ]
    );
    let req = &clock.server.requests_to("/api/usage")[0];
    assert_eq!(req.header("content-type"), Some("application/json"));
}

#[tokio::test]
async fn an_offline_clock_does_not_hold_the_push_past_five_seconds() {
    let fx = Fx::new();
    let slow = FakeHttpServer::start().unwrap();
    slow.route(
        "POST",
        "/api/usage",
        FakeResponse::status(200).with_delay(Duration::from_secs(8)),
    );
    let started = Instant::now();
    let ok = sinks(&fx)
        .send(&[format!("{}/api/usage", slow.url())], &sample(), None)
        .await;
    assert!(!ok);
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "took {:?}",
        started.elapsed()
    );
}

// ---- Publisher ---------------------------------------------------------------------------------------

#[tokio::test]
async fn publisher_pushes_to_a_wifi_clock() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    let (poll, calls) = canned((Some(sample()), false));
    publisher(&fx, &[clock.url()], poll).step().await;
    assert_eq!(clock.payloads(), vec![json!(sample())]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn publisher_is_idle_without_destinations() {
    let fx = Fx::new();
    real_time(&fx);
    let flag = fx
        .cfg
        .write_config("attention", r#"{"m": "hi", "s": "abc"}"#);
    let (poll, calls) = canned((Some(sample()), false));
    publisher(&fx, &[], poll).step().await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(flag.exists(), "attention flags are left for later");
}

#[tokio::test]
async fn publisher_reads_http_targets_from_the_config_by_default() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    fx.config(&format!("http_targets = {}\n", clock.url()));
    let (poll, _) = canned((Some(sample()), false));
    Publisher::new(fx.env.clone(), sinks(&fx), None)
        .with_poller(poll)
        .step()
        .await;
    assert_eq!(clock.payloads().len(), 1);
}

#[tokio::test]
async fn publisher_respects_the_poll_interval() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    let (poll, calls) = canned((Some(sample()), false));
    let publisher = publisher(&fx, &[clock.url()], poll);
    publisher.step().await;
    publisher.step().await; // within POLL_INTERVAL: nothing new
    assert_eq!(
        (calls.load(Ordering::SeqCst), clock.payloads().len()),
        (1, 1)
    );
    fx.clock.advance(POLL_INTERVAL + 1.0);
    publisher.step().await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn publisher_alert_rides_once_then_clear() {
    let fx = Fx::new();
    real_time(&fx);
    fx.config("beep = on\n");
    let clock = FakeClockSink::start();
    let (poll, _) = canned((Some(sample()), false));
    let publisher = publisher(&fx, &[clock.url()], poll);
    publisher.step().await; // initial poll
    fx.cfg
        .write_config("attention", r#"{"m": "Needs you", "s": "sess1"}"#);
    publisher.step().await; // the alert forces a poll
    publisher.step().await; // nothing pending
    let clear = fx.cfg.write_config("clear-sess1", "");
    set_mtime(&clear, real_now() + 5.0);
    publisher.step().await; // the clear forces a poll
    let bodies = clock.payloads();
    assert_eq!(bodies.len(), 3);
    assert!(bodies[0].get("m").is_none());
    assert_eq!(
        (&bodies[1]["m"], &bodies[1]["b"]),
        (&json!("Needs you"), &json!(1))
    );
    assert_eq!(bodies[2].get("x"), Some(&json!(1)));
    assert!(bodies[2].get("m").is_none());
    assert!(!clear.exists(), "clear files are consumed");
}

#[tokio::test]
async fn publisher_beeps_only_when_the_config_says_so() {
    let fx = Fx::new();
    real_time(&fx);
    let clock = FakeClockSink::start();
    let (poll, _) = canned((Some(sample()), false));
    fx.cfg
        .write_config("attention", r#"{"m": "Needs you", "s": "s"}"#);
    publisher(&fx, &[clock.url()], poll).step().await;
    let body = &clock.payloads()[0];
    assert!(body.get("b").is_none());
    assert_eq!(body["m"], "Needs you");
}

#[tokio::test]
async fn publisher_repeated_alert_for_the_same_session_beeps_once() {
    let fx = Fx::new();
    real_time(&fx);
    fx.config("beep = on\n");
    let clock = FakeClockSink::start();
    let (poll, _) = canned((Some(sample()), false));
    let publisher = publisher(&fx, &[clock.url()], poll);
    fx.cfg
        .write_config("attention", r#"{"m": "first", "s": "s1"}"#);
    publisher.step().await;
    fx.cfg
        .write_config("attention", r#"{"m": "second", "s": "s1"}"#);
    publisher.step().await; // same waiting session: no second beep, flag still consumed
    assert_eq!(clock.payloads().len(), 1);
    assert!(!fx.cfg.paths().attention_flag().exists());
}

#[tokio::test]
async fn publisher_no_data_beat_is_login_expired() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    let (poll, _) = canned((None, true));
    publisher(&fx, &[clock.url()], poll).step().await;
    let body = &clock.payloads()[0];
    // why = login: the clock says "Login expired" (fwSeq 7+) and keeps the time
    assert_eq!(
        (&body["ok"], &body["why"]),
        (&json!(false), &json!("login"))
    );
    assert!(body.get("src").is_none() && body["t"].is_i64());
    assert!(matches!(body["tf"].as_i64(), Some(12 | 24)));
    let keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["ok", "why", "t", "tf"]);
}

#[tokio::test]
async fn publisher_no_codex_data_is_not_a_login_problem() {
    let fx = Fx::new();
    fx.usage_source("codex");
    let clock = FakeClockSink::start();
    let (poll, _) = canned((None, true));
    publisher(&fx, &[clock.url()], poll).step().await;
    assert_eq!(clock.payloads(), vec![json!({"ok": false, "src": "codex"})]);
}

#[tokio::test]
async fn publisher_silent_when_the_live_source_did_not_answer() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    let (poll, calls) = canned((None, false));
    let publisher = publisher(&fx, &[clock.url()], poll);
    publisher.step().await;
    publisher.step().await;
    assert!(clock.payloads().is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 2, "retried on the next tick");
}

#[tokio::test]
async fn publisher_delivered_if_any_destination_ok() {
    let fx = Fx::new();
    let (dead, live) = (
        FakeClockSink::start_with_status(500),
        FakeClockSink::start(),
    );
    let (poll, calls) = canned((Some(sample()), false));
    let publisher = publisher(&fx, &[dead.url(), live.url()], poll);
    publisher.step().await;
    publisher.step().await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a dead screen must not block the schedule"
    );
}

#[tokio::test]
async fn publisher_retries_next_tick_when_nothing_took_the_data() {
    let fx = Fx::new();
    let dead = FakeClockSink::start_with_status(500);
    let (poll, calls) = canned((Some(sample()), false));
    let publisher = publisher(&fx, &[dead.url()], poll);
    publisher.step().await;
    publisher.step().await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn request_poll_forces_a_poll_inside_the_interval() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    let (poll, calls) = canned((Some(sample()), false));
    let publisher = publisher(&fx, &[clock.url()], poll);
    publisher.step().await;
    publisher.request_poll();
    publisher.step().await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn on_payload_sees_every_delivery() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    let (poll, _) = canned((Some(sample()), false));
    let publisher = publisher(&fx, &[clock.url()], poll);
    let seen: Arc<Mutex<Vec<UsagePayload>>> = Arc::default();
    let log = Arc::clone(&seen);
    publisher.set_on_payload(Arc::new(move |p| log.lock().unwrap().push(p.clone())));
    publisher.step().await;
    assert_eq!(seen.lock().unwrap().as_slice(), [sample()]);
}

#[tokio::test]
async fn resend_pushes_the_cached_payload_without_polling() {
    let fx = Fx::new();
    real_time(&fx);
    fx.config("beep = on\nclock = 24\n");
    let (first, second) = (FakeClockSink::start(), FakeClockSink::start());
    let (poll, calls) = canned((
        Some(payload(json!({"s": 1, "ok": true, "t": 5, "tf": 24}))),
        false,
    ));
    let publisher = publisher(&fx, &[first.url()], poll);
    fx.cfg
        .write_config("attention", r#"{"m": "alert", "s": "x"}"#);
    publisher.step().await;
    assert!(first.payloads()[0].get("m").is_some());
    assert!(publisher.resend(&second.url()).await);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "never calls the usage API");
    let body: Value = second.payloads().remove(0);
    assert!(
        body.get("m").is_none() && body.get("b").is_none(),
        "one-shot extras are not replayed"
    );
    assert_eq!(body["s"], 1);
    assert_ne!(body["t"], 5, "the wall-clock time is refreshed");
}

#[tokio::test]
async fn resend_with_nothing_cached_asks_for_a_poll() {
    let fx = Fx::new();
    let (target, other) = (FakeClockSink::start(), FakeClockSink::start());
    let (poll, calls) = canned((Some(sample()), false));
    let publisher = publisher(&fx, &[target.url()], poll);
    assert!(publisher.resend(&other.url()).await);
    assert!(other.payloads().is_empty());
    publisher.step().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(target.payloads().len(), 1);
}

#[tokio::test]
async fn resend_reports_a_clock_that_did_not_take_it() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    let (poll, _) = canned((Some(sample()), false));
    let publisher = publisher(&fx, &[clock.url()], poll);
    publisher.step().await;
    assert!(!publisher.resend("http://127.0.0.1:1/api/usage").await);
}

// ---- run loop ----------------------------------------------------------------------------------------

#[tokio::test]
async fn run_survives_a_panicking_poll_and_stops_on_signal() {
    let fx = Fx::new();
    let clock = FakeClockSink::start();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let poll: PollFn = Arc::new(move || {
        let n = counter.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            assert!(n != 0, "boom: first poll fails");
            (Some(sample()), false)
        })
    });
    let publisher = Arc::new(publisher(&fx, &[clock.url()], poll));
    let (stop_tx, stop_rx) = new_stop();
    let task = tokio::spawn({
        let publisher = Arc::clone(&publisher);
        async move { publisher.run(stop_rx).await }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(clock.payloads().is_empty());
    publisher.request_poll(); // wakes the loop instead of waiting a 5 s tick
    tokio::time::timeout(Duration::from_secs(5), async {
        while clock.payloads().is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the loop survived the panic and pushed");
    stop_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("stops promptly")
        .unwrap();
}
