//! Port of `tests/test_clockapi.py` against the testkit's fake clock (a local HTTP server).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_clock::api::{ClockApiOptions, SleepFn, OTA_MAX, POLL_GAP, RETRY_GAP, STYLE_MAX};
use clawd_clock::{config_diff, ClockApi};
use clawd_contracts::{Clock, ClockError};
use clawd_testkit::clock::FakeClockDevice;
use clawd_testkit::{FakeClock, FakeResponse};
use serde_json::{json, Value};

/// No waiting between tries; `timeout` only where a test needs a clock that is too slow.
fn fast_with(timeout: Option<Duration>) -> ClockApiOptions {
    ClockApiOptions {
        retry_gap: Duration::ZERO,
        poll_gap: Duration::from_millis(20),
        timeout,
        sleep: None,
        client: None,
    }
}

type SleepLog = Arc<Mutex<Vec<Duration>>>;

/// Python's `sleep=lambda s: t.__setitem__(0, t[0] + s)`: no real sleeping, the fake clock moves.
fn fake_sleep(clock: &FakeClock, log: &SleepLog) -> SleepFn {
    let clock = clock.clone();
    let log = Arc::clone(log);
    Arc::new(move |gap| {
        log.lock().unwrap().push(gap);
        clock.advance(gap.as_secs_f64());
        Box::pin(async {})
    })
}

/// Default gaps (`RETRY_GAP`, `POLL_GAP`) and timeouts; only the sleep is fake.
fn sleepless(base: &str, clock: &FakeClock, log: &SleepLog) -> ClockApi {
    let options = ClockApiOptions {
        sleep: Some(fake_sleep(clock, log)),
        ..ClockApiOptions::default()
    };
    ClockApi::with_options(base, None, options)
}

fn fast() -> ClockApiOptions {
    fast_with(None)
}

/// A client that gives up after 1 s (the slow clock answers 2 s late).
fn impatient(device: &FakeClockDevice) -> ClockApi {
    ClockApi::with_options(&device.url(), None, fast_with(Some(Duration::from_secs(1))))
}

fn api(device: &FakeClockDevice, auth: Option<(&str, &str)>) -> ClockApi {
    ClockApi::with_options(
        &device.url(),
        auth.map(|(u, p)| (u.to_owned(), p.to_owned())),
        fast(),
    )
}

// ---- config_diff ----------------------------------------------------------------------

#[test]
fn diff_sends_only_changed_keys_nested() {
    let old =
        json!({"brightness": 45, "rotation": 0, "usage": {"screen": 0, "layout": 0, "anim": 9}});
    let new =
        json!({"brightness": 60, "rotation": 0, "usage": {"screen": 0, "layout": 1, "anim": 9}});
    assert_eq!(
        config_diff(&old, &new),
        json!({"brightness": 60, "usage": {"layout": 1}})
    );
}

#[test]
fn diff_sends_a_changed_list_whole_and_nothing_when_equal() {
    let old = json!({"wifi": [{"ssid": "A"}, {"ssid": "B"}]});
    let new = json!({"wifi": [{"ssid": "B"}, {"ssid": "A"}]});
    assert_eq!(
        config_diff(&old, &new),
        json!({"wifi": [{"ssid": "B"}, {"ssid": "A"}]})
    );
    assert_eq!(config_diff(&old, &old.clone()), json!({}));
}

#[test]
fn diff_new_keys_go_and_missing_keys_stay() {
    assert_eq!(
        config_diff(&json!({"a": 1, "b": 2}), &json!({"a": 1, "c": 3})),
        json!({"c": 3})
    );
    assert_eq!(
        config_diff(&json!({"usage": 5}), &json!({"usage": {"layout": 1}})),
        json!({"usage": {"layout": 1}})
    );
    assert_eq!(config_diff(&json!({"a": 1}), &json!([1])), json!({}));
}

// ---- requests -------------------------------------------------------------------------

#[tokio::test]
async fn save_posts_the_diff_and_skips_an_empty_one() {
    let device = FakeClockDevice::start(None);
    let c = api(&device, None);
    assert_eq!(
        c.save(&json!({})).await.unwrap(),
        json!({"ok": true, "reboot": false, "unchanged": true})
    );
    assert!(device.saved().is_empty());
    assert_eq!(
        c.save(&json!({"brightness": 60})).await.unwrap()["ok"],
        true
    );
    assert_eq!(device.saved(), vec![json!({"brightness": 60})]);
    assert_eq!(c.save(&json!({"wifi": []})).await.unwrap()["reboot"], true);
}

#[tokio::test]
async fn save_refused_is_a_clock_error_with_the_python_repr() {
    let device = FakeClockDevice::start(None);
    device.override_path(
        "/api/config",
        FakeResponse::json(200, &json!({"ok": false, "why": "x"})),
    );
    let err = api(&device, None).save(&json!({"a": 1})).await.unwrap_err();
    assert_eq!(err.to_string(), "save refused: {'ok': False, 'why': 'x'}");
}

#[tokio::test]
async fn password_missing_is_auth_needed() {
    let device = FakeClockDevice::start(Some("pw"));
    let err = api(&device, None).config().await.unwrap_err();
    assert_eq!(err, ClockError::AuthNeeded);
    let wrong = api(&device, Some(("admin", "nope")))
        .config()
        .await
        .unwrap_err();
    assert_eq!(wrong, ClockError::AuthNeeded);
}

#[tokio::test]
async fn digest_login_and_one_signed_upload() {
    let device = FakeClockDevice::start(Some("pw"));
    let c = api(&device, Some(("admin", "pw")));
    c.upload_firmware(&[0xe9; 1000]).await.unwrap();
    let uploads: Vec<_> = device
        .calls()
        .into_iter()
        .filter(|x| x.path == "/update")
        .collect();
    assert_eq!(uploads.len(), 1, "sent once");
    assert!(uploads[0].signed, "signed the first time");
    assert_eq!(
        device.challenges(),
        1,
        "one challenge for the whole session"
    );
    // later requests are signed up front: no new challenge
    c.config().await.unwrap();
    assert_eq!(device.challenges(), 1);
}

#[tokio::test]
async fn firmware_refusal_text_is_reported() {
    let device = FakeClockDevice::start(None);
    device.override_path("/update", FakeResponse::text(200, "FAIL: bad image"));
    let err = api(&device, None)
        .upload_firmware(b"abc")
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "update refused: FAIL: bad image");
}

#[tokio::test]
async fn upload_size_guard() {
    let device = FakeClockDevice::start(None);
    let c = api(&device, None);
    let empty = c.upload_firmware(b"").await.unwrap_err();
    assert_eq!(empty.to_string(), "firmware size 0 B is outside 1..524288");
    assert!(c.upload_firmware(&vec![b'x'; OTA_MAX + 1]).await.is_err());
}

#[tokio::test]
async fn get_retries_a_slow_clock() {
    let device = FakeClockDevice::start(None);
    device.set_down(2, Duration::from_secs(2));
    let c = impatient(&device);
    assert_eq!(
        c.config().await.unwrap()["brightness"],
        45,
        "2 timeouts, 3rd try answers"
    );
}

#[tokio::test]
async fn get_gives_up_after_two_retries_a_retry_gap_apart() {
    // a port nobody listens on: every try is a ConnectError, at once
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let (clock, log) = (FakeClock::new(0.0), SleepLog::default());
    let err = sleepless(&base, &clock, &log).config().await.unwrap_err();
    assert_eq!(
        err,
        ClockError::NoAnswer {
            base,
            kind: "ConnectError".into()
        }
    );
    assert_eq!(*log.lock().unwrap(), [RETRY_GAP, RETRY_GAP]);
}

#[tokio::test]
async fn a_post_is_not_retried() {
    let device = FakeClockDevice::start(None);
    device.set_down(1, Duration::from_secs(2));
    let err = impatient(&device).reboot().await.unwrap_err();
    assert!(matches!(err, ClockError::NoAnswer { .. }));
    // a starved test machine may show the request to the server late
    for _ in 0..100 {
        if !device.calls().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(device.calls().len(), 1, "one try, no retry");
}

#[tokio::test]
async fn scan_export_restore_reboot() {
    let device = FakeClockDevice::start(None);
    let c = api(&device, None);
    assert_eq!(c.scan().await.unwrap()[0]["ssid"], "Home");
    assert_eq!(c.status().await.unwrap()["ssid"], "Home");
    assert_eq!(c.export().await.unwrap()["wifi"][0]["pass"], "x");
    c.restore(&json!({"brightness": 1})).await.unwrap();
    c.reboot().await.unwrap();
    c.factory_reset().await.unwrap();
    let paths: Vec<String> = device.calls().into_iter().map(|c| c.path).collect();
    assert_eq!(
        paths[paths.len() - 3..],
        ["/api/import", "/api/reboot", "/api/factory"]
    );
}

fn heap_polls(device: &FakeClockDevice) -> usize {
    device
        .calls()
        .iter()
        .filter(|c| c.path == "/api/heap")
        .count()
}

#[tokio::test]
async fn wait_back_gives_up_at_the_150_s_deadline_without_sleeping() {
    let device = FakeClockDevice::start(None);
    device.set_uptime(Some(5000)); // still the old run: keeps waiting
    let (clock, log) = (FakeClock::new(1000.0), SleepLog::default());
    let c = sleepless(&device.url(), &clock, &log);
    let err = c.wait_back(999.0, None, &clock, None).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "the clock did not come back after the update"
    );
    // polls at t = 0, 2, ..., 148 of the 150 s; the loop ends once the clock reaches the end
    assert_eq!(heap_polls(&device), 75);
    assert_eq!(log.lock().unwrap().len(), 75);
    assert!(log.lock().unwrap().iter().all(|g| *g == POLL_GAP));
    assert_eq!(clock.now(), 1150.0);
}

#[tokio::test]
async fn wait_back_deadline_argument_is_respected() {
    let device = FakeClockDevice::start(None);
    device.set_uptime(Some(5000));
    let (clock, log) = (FakeClock::new(0.0), SleepLog::default());
    let c = sleepless(&device.url(), &clock, &log);
    let deadline = Some(Duration::from_secs(10));
    assert!(c.wait_back(-1.0, deadline, &clock, None).await.is_err());
    assert_eq!(heap_polls(&device), 5); // t = 0, 2, 4, 6, 8
}

#[tokio::test]
async fn wait_back_needs_a_boot_after_the_upload() {
    let device = FakeClockDevice::start(None);
    device.set_uptime(Some(0)); // booted at the moment of each poll
    let (clock, log) = (FakeClock::new(1000.0), SleepLog::default());
    let c = sleepless(&device.url(), &clock, &log);
    let ticks = AtomicUsize::new(0);
    let tick = || {
        ticks.fetch_add(1, Ordering::SeqCst);
    };
    // the clock must have started after t = 1010: first true at the poll made at t = 1012
    let heap = c
        .wait_back(1010.0, Some(Duration::from_secs(60)), &clock, Some(&tick))
        .await
        .unwrap();
    assert_eq!(heap["uptime"], 0);
    assert_eq!(ticks.load(Ordering::SeqCst), 7, "tick runs on every poll");
    assert_eq!(clock.now(), 1012.0);
}

#[tokio::test]
async fn wait_back_survives_a_clock_that_is_not_answering_yet() {
    let device = FakeClockDevice::start(None);
    device.set_uptime(None); // connection closed: ClockError inside, swallowed
    let (clock, log) = (FakeClock::new(0.0), SleepLog::default());
    let c = sleepless(&device.url(), &clock, &log);
    let deadline = Some(Duration::from_secs(10));
    let err = c.wait_back(0.0, deadline, &clock, None).await.unwrap_err();
    assert!(matches!(err, ClockError::Other(_)));
    assert_eq!(heap_polls(&device), 5);
}

#[tokio::test]
async fn a_bulk_read_that_stalls_times_out_per_operation_and_is_retried() {
    // get_file is a bulk call: its 60 s (here 1 s) is a per-read limit on a client of its own
    let device = FakeClockDevice::start(None);
    device.set_down(1, Duration::from_secs(2));
    let options = fast_with(Some(Duration::from_secs(1)));
    let c = ClockApi::with_options(&device.url(), None, options);
    assert_eq!(c.get_file("/clawd.pak", 1000).await.unwrap().len(), 100);
    assert_eq!(heap_polls(&device), 0);
    assert_eq!(device.calls().len(), 2, "the stalled try and its retry");
}

#[tokio::test]
async fn get_file_reads_one_file_signed_and_minds_its_size() {
    let device = FakeClockDevice::start(Some("pw"));
    let a = api(&device, Some(("admin", "pw")));
    let got = a.get_file("/clawd.pak", 1000).await.unwrap();
    assert_eq!(got.len(), 100);
    assert!(got.starts_with(b"PAK0"));
    let last = device.calls().pop().unwrap();
    assert_eq!(
        (last.method.as_str(), last.path.as_str(), last.signed),
        ("GET", "/api/fs/get", true)
    );
    let big = a.get_file("/clawd.pak", 50).await.unwrap_err();
    assert_eq!(big.to_string(), "/clawd.pak is 100 B, more than 50");
    assert!(matches!(
        a.get_file("/nope", 1000).await.unwrap_err(),
        ClockError::Http { status: 404, .. }
    ));
    let locked = FakeClockDevice::start(Some("pw"));
    let nologin = api(&locked, None);
    assert_eq!(
        nologin.get_file("/clawd.pak", 1000).await.unwrap_err(),
        ClockError::AuthNeeded
    );
}

#[tokio::test]
async fn style_install_signed_once_clear_and_refusals() {
    let device = FakeClockDevice::start(Some("pw"));
    let c = api(&device, Some(("admin", "pw")));
    let mut pack = b"CSTY".to_vec();
    pack.extend_from_slice(&[0; 200]);
    assert_eq!(
        c.install_style(&pack).await.unwrap(),
        json!({"active": true, "id": "synth"})
    );
    let posts: Vec<_> = device
        .calls()
        .into_iter()
        .filter(|x| x.method == "POST" && x.path == "/api/style")
        .collect();
    assert_eq!(posts.len(), 1, "challenge collected first: sent once");
    assert!(posts[0].signed);
    let refused = c.install_style(b"nope").await.unwrap_err();
    assert!(
        refused
            .to_string()
            .starts_with("the clock refused the style pack ("),
        "{refused}"
    );
    assert!(c.install_style(b"").await.is_err());
    assert!(c.install_style(&vec![0; STYLE_MAX + 1]).await.is_err());
    assert_eq!(c.clear_style().await.unwrap(), json!({"active": false}));
    let open = FakeClockDevice::start(None);
    assert_eq!(api(&open, None).style().await.unwrap()["slot"], 32768);
}

#[tokio::test]
async fn style_install_without_the_password_says_refused_not_auth_needed() {
    // Python wraps every ClockError (AuthNeeded too) of the POST in "the clock refused ..."
    let device = FakeClockDevice::start(Some("pw"));
    let err = api(&device, None).install_style(b"CSTY").await.unwrap_err();
    assert!(
        matches!(err, ClockError::Other(ref t) if t.contains("wants its web password")),
        "{err:?}"
    );
}

#[tokio::test]
async fn upload_pack_checks_kind_size_and_answer() {
    let device = FakeClockDevice::start(None);
    let c = api(&device, None);
    assert_eq!(
        c.upload_pack("nopak", b"x").await.unwrap_err().to_string(),
        "unknown pack 'nopak'"
    );
    assert!(c.upload_pack("clawdpak", b"").await.is_err());
    assert_eq!(c.upload_pack("deskpak", b"abc").await.unwrap()["anims"], 12);
    let upload = &device.server().requests_to("/api/deskpak")[0];
    assert!(String::from_utf8_lossy(&upload.body).contains("filename=\"desk.pak\""));
    device.set_pack_answer(json!({"ok": true, "anims": 0}));
    let refused = c.upload_pack("clawdpak", b"abc").await.unwrap_err();
    assert_eq!(
        refused.to_string(),
        "the clock refused clawdpak: {'ok': True, 'anims': 0}"
    );
}

#[tokio::test]
async fn odd_answers_are_clock_errors() {
    let device = FakeClockDevice::start(None);
    device.override_path("/api/config", FakeResponse::json(200, &json!([1])));
    device.override_path("/api/status", FakeResponse::text(200, "<html>"));
    device.override_path("/api/scan", FakeResponse::json(200, &json!({"x": 1})));
    let c = api(&device, None);
    assert_eq!(
        c.config().await.unwrap_err().to_string(),
        "odd /api/config answer"
    );
    assert_eq!(
        c.status().await.unwrap_err().to_string(),
        "not JSON from /api/status"
    );
    assert!(c.scan().await.unwrap().is_empty());
}

#[tokio::test]
async fn http_errors_keep_status_and_200_chars_of_body() {
    let device = FakeClockDevice::start(None);
    device.override_path("/api/export", FakeResponse::text(500, &"e".repeat(300)));
    let err = api(&device, None).export().await.unwrap_err();
    match err {
        ClockError::Http { status, body } => {
            assert_eq!(status, 500);
            assert_eq!(body.len(), 200);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn set_auth_switches_the_login() {
    let device = FakeClockDevice::start(Some("pw"));
    let c = api(&device, None);
    assert_eq!(c.config().await.unwrap_err(), ClockError::AuthNeeded);
    c.set_auth(Some(("admin".into(), "pw".into())));
    assert_eq!(c.config().await.unwrap()["brightness"], 45);
}

#[tokio::test]
async fn a_malformed_host_is_a_clock_error_not_a_crash() {
    let c = ClockApi::with_options("http://10..97.40.207", None, fast());
    assert!(c.heap().await.is_err());
    let c = ClockApi::with_options("http://[::1", None, fast());
    assert!(matches!(c.heap().await, Err(ClockError::Other(_))));
    let c = ClockApi::with_options("not a url", None, fast());
    assert!(matches!(c.heap().await, Err(ClockError::Other(_))));
}

#[test]
fn base_drops_a_trailing_slash() {
    assert_eq!(
        ClockApi::new("http://10.0.0.5/", None).base(),
        "http://10.0.0.5"
    );
}

#[tokio::test]
async fn requests_are_serialised_per_client() {
    let device = FakeClockDevice::start(None);
    let c = api(&device, None);
    let (a, b) = tokio::join!(c.config(), c.status());
    assert!(a.is_ok() && b.is_ok());
    let _: Value = c.heap().await.unwrap();
}
