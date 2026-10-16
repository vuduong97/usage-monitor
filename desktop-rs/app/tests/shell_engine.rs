//! Ports of `tests/test_engine.py` (token, pause, screen sync, stop) plus the panic containment
//! of red team #12. The wiring of `run` needs the real crates (03-10): `#[ignore]`d below.
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_app::engine::{
    credential_key, gated_targets, guard, panic_message, screen_of, supervise, sync_screen_loop,
    watch_token_loop, Backoff, EngineState,
};
use clawd_clock::FinderSnapshot;
use clawd_contracts::{new_stop, ClockSource, ClockTarget};
use common::*;
use serde_json::json;

fn on_payload(state: &mut EngineState, v: serde_json::Value) {
    state.on_payload(&payload(v), NOW);
}

#[test]
fn no_token_payload_marks_token_missing() {
    let mut e = EngineState::default();
    on_payload(&mut e, json!({"ok": false}));
    assert_eq!(e.token, Some(false));
    on_payload(&mut e, json!({"s": 1, "ok": true}));
    assert_eq!(e.token, Some(true));
}

#[test]
fn codex_or_no_usage_says_nothing_about_a_claude_login() {
    let mut e = EngineState::default();
    on_payload(&mut e, json!({"ok": false, "src": "off"}));
    assert_eq!(e.token, None);
    on_payload(&mut e, json!({"ok": false, "src": "codex"}));
    assert_eq!(e.token, None);
    on_payload(&mut e, json!({"s": 3, "ok": true, "src": "codex"}));
    assert_eq!(e.token, None);
}

#[test]
fn a_settings_pause_empties_the_targets() {
    let state = Mutex::new(EngineState::default());
    let finder = || vec!["http://10.0.0.7/api/usage".to_owned()];
    assert_eq!(gated_targets(&state, None, finder), finder());
    assert!(gated_targets(&state, Some("firmware update".into()), finder).is_empty());
    assert_eq!(
        state.lock().unwrap().paused.as_deref(),
        Some("firmware update")
    );
    // The window crashed: the lease ran out.
    assert_eq!(gated_targets(&state, None, finder), finder());
    assert_eq!(state.lock().unwrap().paused, None);
}

#[test]
fn the_status_carries_what_the_engine_knows() {
    let mut e = EngineState::default();
    on_payload(&mut e, json!({"s": 1, "ok": true}));
    e.note_pause(Some("firmware update".into()));
    let finder = FinderSnapshot {
        manual_config: false,
        clock: Some(clock_a()),
        config_targets: vec![],
        last_ok: Some(NOW),
        failing: false,
        searching: false,
    };
    let status = e.status(&finder);
    assert_eq!(status.clock, Some(clock_a()));
    assert_eq!(status.last_ok, Some(NOW));
    assert_eq!(status.paused.as_deref(), Some("firmware update"));
    assert_eq!(status.token, Some(true));
    assert!(
        !e.note_pause(Some("firmware update".into())),
        "unchanged: no log line"
    );
}

#[test]
fn the_clocks_main_screen_is_an_integer_in_its_config() {
    assert_eq!(screen_of(&json!({"usage": {"screen": 1}})), Some(1));
    assert_eq!(screen_of(&json!({"usage": {"screen": true}})), None);
    assert_eq!(screen_of(&json!({"usage": {}})), None);
    assert_eq!(screen_of(&json!({})), None);
}

#[test]
fn the_password_key_is_the_name_else_the_address() {
    let mut c = ClockTarget::new("10.0.0.7", ClockSource::Saved);
    assert_eq!(credential_key(&c), "10.0.0.7");
    c.name = "clawd".into();
    assert_eq!(credential_key(&c), "clawd");
}

#[tokio::test]
async fn sync_screen_survives_a_failed_prefs_write() {
    // desktop.json busy in the other process: the next sync still runs.
    let (stop_tx, stop) = new_stop();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let set: Arc<dyn Fn(i64) -> std::io::Result<()> + Send + Sync> = {
        let calls = Arc::clone(&calls);
        Arc::new(move |screen| {
            let mut calls = calls.lock().unwrap();
            calls.push(screen);
            if calls.len() == 1 {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            } else {
                Ok(())
            }
        })
    };
    let task = tokio::spawn(sync_screen_loop(
        stop,
        Duration::from_secs(300),
        Duration::from_millis(10),
        || false,
        || async { Some(1) },
        set,
    ));
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        if calls.lock().unwrap().len() >= 2 {
            break;
        }
    }
    stop_tx.send_replace(true);
    task.await.unwrap();
    assert_eq!(calls.lock().unwrap()[..2], [1, 1]);
}

#[tokio::test]
async fn sync_screen_waits_while_paused() {
    let (stop_tx, stop) = new_stop();
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&reads);
    let task = tokio::spawn(sync_screen_loop(
        stop,
        Duration::from_millis(10),
        Duration::from_millis(10),
        || true,
        move || {
            counted.fetch_add(1, Ordering::SeqCst);
            async { Some(1) }
        },
        Arc::new(|_| Ok(())),
    ));
    tokio::time::sleep(Duration::from_millis(60)).await;
    stop_tx.send_replace(true);
    task.await.unwrap();
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "no reads during a pause lease"
    );
}

#[tokio::test]
async fn the_token_watch_ends_with_the_first_payload() {
    let (_stop_tx, stop) = new_stop();
    let state = Arc::new(Mutex::new(EngineState::default()));
    let checks = Arc::new(AtomicUsize::new(0));
    let (counted, in_loop) = (Arc::clone(&checks), Arc::clone(&state));
    let task = tokio::spawn(watch_token_loop(
        stop,
        Arc::clone(&state),
        Duration::from_millis(10),
        move || {
            // The first check finds a token; the payload arrives after the second.
            let n = counted.fetch_add(1, Ordering::SeqCst) + 1;
            let state = Arc::clone(&in_loop);
            async move {
                if n == 2 {
                    state.lock().unwrap().payload = Some(payload(json!({"s": 1, "ok": true})));
                }
                n == 1
            }
        },
    ));
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("ends by itself")
        .unwrap();
    assert_eq!(checks.load(Ordering::SeqCst), 2);
    assert_eq!(
        state.lock().unwrap().token,
        Some(true),
        "a payload owns the token after that"
    );
}

#[tokio::test]
async fn a_missing_local_token_is_not_proof() {
    let (stop_tx, stop) = new_stop();
    let state = Arc::new(Mutex::new(EngineState {
        token: Some(true),
        ..EngineState::default()
    }));
    let task = tokio::spawn(watch_token_loop(
        stop,
        Arc::clone(&state),
        Duration::from_millis(10),
        || async { false },
    ));
    tokio::time::sleep(Duration::from_millis(40)).await;
    stop_tx.send_replace(true);
    task.await.unwrap();
    assert_eq!(
        state.lock().unwrap().token,
        None,
        "unknown, not 'login expired'"
    );
}

#[tokio::test]
async fn a_panicking_loop_is_restarted_and_the_app_lives() {
    let (stop_tx, stop) = new_stop();
    let runs = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&runs);
    let backoff = Backoff {
        first: Duration::from_millis(5),
        max: Duration::from_millis(20),
        healthy_after: Duration::from_secs(60),
    };
    let supervisor = tokio::spawn(supervise("test loop", stop.clone(), backoff, move || {
        let run = counted.fetch_add(1, Ordering::SeqCst) + 1;
        let mut stop = stop.clone();
        async move {
            if run < 3 {
                panic!("boom in run {run}");
            }
            let _ = stop.wait_for(|s| *s).await;
        }
    }));
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        if runs.load(Ordering::SeqCst) >= 3 {
            break;
        }
    }
    assert_eq!(
        runs.load(Ordering::SeqCst),
        3,
        "restarted twice after the panics"
    );
    assert!(!supervisor.is_finished(), "the supervisor itself survived");
    stop_tx.send_replace(true);
    tokio::time::timeout(Duration::from_secs(5), supervisor)
        .await
        .expect("stops")
        .unwrap();
}

#[test]
fn guard_contains_a_panic_in_a_sync_callback() {
    assert_eq!(guard("ok", || 7), Some(7));
    // The default hook prints the panic; the test only needs it not to cross `guard`.
    assert_eq!(guard("bad", || -> i32 { panic!("callback blew up") }), None);
}

#[test]
fn panic_messages_are_readable() {
    let text = std::panic::catch_unwind(|| panic!("plain")).unwrap_err();
    assert_eq!(panic_message(&*text), "plain");
    let owned = std::panic::catch_unwind(|| panic!("n={}", 3)).unwrap_err();
    assert_eq!(panic_message(&*owned), "n=3");
    let other = std::panic::catch_unwind(|| std::panic::panic_any(5_u8)).unwrap_err();
    assert_eq!(panic_message(&*other), "non-string panic payload");
}

// ---- the wiring needs the real crates ---------------------------------------------------

#[test]
#[ignore = "needs 03 (PauseReader) and 08 (ClockFinder): test_a_settings_pause_empties_the_targets end to end"]
fn engine_handle_targets_follow_the_pause_lease() {
    unimplemented!("B'': EngineHandle::with_parts over a temp home and a fake clock");
}

#[test]
#[ignore = "needs 03-10: test_engine_pushes_usage_to_the_found_clock, test_daemon_running_checks_the_state_endpoint"]
fn engine_run_pushes_usage_to_the_found_clock() {
    unimplemented!("B'': EngineHandle::start against a loopback clock and event port");
}

#[test]
#[ignore = "needs 03/08: test_stop_twice_after_the_loop_closed_is_harmless"]
fn stop_twice_and_poll_now_after_stop_are_harmless() {
    unimplemented!("B'': EngineHandle::new over a temp home; stop(); stop(); poll_now()");
}
