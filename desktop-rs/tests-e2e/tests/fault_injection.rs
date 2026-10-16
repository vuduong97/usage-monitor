//! Fault injection (red team #12, `panic = "unwind"`): a panic inside one engine loop must not
//! take the app down, and the loop must run again. Asserted through counters and through the
//! traffic the fake clock keeps receiving.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clawd_app::engine::{guard, supervise, Backoff};
use clawd_contracts::new_stop;
use clawd_e2e::http::post_hook;
use clawd_e2e::rig::{flaky_poller, state_bodies, usage_bodies, wait_until, Rig, RigOptions};
use serde_json::json;

const FAST: Backoff = Backoff {
    first: Duration::from_millis(20),
    max: Duration::from_millis(80),
    healthy_after: Duration::from_secs(60),
};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime")
}

#[test]
fn supervise_restarts_a_panicking_loop_and_leaves_its_sibling_alone() {
    let rt = runtime();
    let (stop_tx, stop) = new_stop();
    let (crashy_runs, sibling_runs) =
        (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));

    let crashy = {
        let (runs, stop) = (Arc::clone(&crashy_runs), stop.clone());
        rt.spawn(supervise("crashy", stop.clone(), FAST, move || {
            let n = runs.fetch_add(1, Ordering::SeqCst);
            let mut stop = stop.clone();
            async move {
                if n < 3 {
                    panic!("injected panic #{n}");
                }
                let _ = stop.wait_for(|s| *s).await;
            }
        }))
    };
    let sibling = {
        let (runs, stop) = (Arc::clone(&sibling_runs), stop.clone());
        rt.spawn(supervise("sibling", stop.clone(), FAST, move || {
            runs.fetch_add(1, Ordering::SeqCst);
            let mut stop = stop.clone();
            async move {
                let _ = stop.wait_for(|s| *s).await;
            }
        }))
    };

    assert!(
        wait_until(5.0, || crashy_runs.load(Ordering::SeqCst) == 4),
        "three panics, then a stable fourth run: {}",
        crashy_runs.load(Ordering::SeqCst)
    );
    assert_eq!(
        sibling_runs.load(Ordering::SeqCst),
        1,
        "the sibling never restarted"
    );
    assert!(!crashy.is_finished(), "the supervisor outlives the panics");

    stop_tx.send_replace(true);
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            let _ = crashy.await;
            let _ = sibling.await;
        })
        .await
        .expect("both supervisors end on stop");
    });
    assert_eq!(
        crashy_runs.load(Ordering::SeqCst),
        4,
        "no restart after stop"
    );
}

#[test]
fn supervise_restarts_a_loop_that_ends_early_and_backs_off() {
    let rt = runtime();
    let (stop_tx, stop) = new_stop();
    let stamps = Arc::new(std::sync::Mutex::new(Vec::<std::time::Instant>::new()));
    let task = {
        let stamps = Arc::clone(&stamps);
        rt.spawn(supervise("quitter", stop, FAST, move || {
            stamps.lock().unwrap().push(std::time::Instant::now());
            async {} // returns at once: counts as ending before stop
        }))
    };
    assert!(wait_until(5.0, || stamps.lock().unwrap().len() >= 5));
    stop_tx.send_replace(true);
    rt.block_on(async { tokio::time::timeout(Duration::from_secs(5), task).await })
        .expect("ends on stop")
        .expect("no panic");
    let stamps = stamps.lock().unwrap().clone();
    let gaps: Vec<Duration> = stamps.windows(2).take(4).map(|w| w[1] - w[0]).collect();
    assert!(
        gaps[0] >= Duration::from_millis(15),
        "first wait ~20 ms: {gaps:?}"
    );
    assert!(
        gaps[3] >= Duration::from_millis(70),
        "capped wait ~80 ms: {gaps:?}"
    );
    assert!(gaps[3] > gaps[0], "the wait grows: {gaps:?}");
}

#[test]
fn guard_turns_a_panic_in_a_callback_into_none() {
    assert_eq!(guard("ok", || 7), Some(7));
    assert_eq!(
        guard("boom", || -> u8 { panic!("injected callback panic") }),
        None
    );
}

/// The real loops, supervised like `engine::run` does it: the state pusher panics at its first
/// start. The event server stays up (a hook is still answered 200), the pusher starts again and
/// the clock gets the state of a hook sent *during* the outage.
#[test]
fn a_panicking_state_pusher_restarts_and_the_hook_still_reaches_the_clock() {
    let mut rig = Rig::new(RigOptions::default());
    let runs = Arc::new(AtomicUsize::new(0));
    {
        let (pusher, stop, runs) = (Arc::clone(&rig.pusher), rig.stop.clone(), Arc::clone(&runs));
        rig.spawn(supervise(
            "state pusher",
            rig.stop.clone(),
            FAST,
            move || {
                let n = runs.fetch_add(1, Ordering::SeqCst);
                let (pusher, stop) = (Arc::clone(&pusher), stop.clone());
                async move {
                    if n == 0 {
                        panic!("injected: state pusher dies at start");
                    }
                    pusher.run(stop).await;
                }
            },
        ));
    }
    let port = rig.port();
    let prompt = json!({"hook_event_name": "UserPromptSubmit", "session_id": "f1"});
    let reply = post_hook(port, "claude", "UserPromptSubmit", &prompt).expect("post");
    assert_eq!(reply.status, 200, "the server survives the loop crash");

    assert!(
        wait_until(10.0, || runs.load(Ordering::SeqCst) >= 2),
        "pusher restarted"
    );
    assert!(
        wait_until(15.0, || !state_bodies(&rig.device).is_empty()),
        "after the restart the pusher delivers"
    );
    assert_eq!(
        runs.load(Ordering::SeqCst),
        2,
        "one crash, one restart, then stable"
    );
    rig.shutdown();
}

/// The usage publisher's poll panics twice: its own loop catches it (one cycle lost), the next
/// requested poll goes through and the clock gets the usage.
#[test]
fn a_panicking_usage_poll_loses_one_cycle_not_the_loop() {
    let payload = json!({"s": 11, "sr": 100, "w": 22, "wr": 5000, "st": "allowed", "acct": "pro", "ok": true});
    let (poll, calls) = flaky_poller(2, payload);
    let mut rig = Rig::new(RigOptions {
        poller: Some(poll),
        ..RigOptions::default()
    });
    rig.spawn_loops();

    assert!(
        wait_until(10.0, || calls.load(Ordering::SeqCst) >= 1),
        "first poll ran (and panicked)"
    );
    assert!(usage_bodies(&rig.device).is_empty());
    rig.publisher.request_poll();
    assert!(
        wait_until(10.0, || calls.load(Ordering::SeqCst) >= 2),
        "second poll ran"
    );
    rig.publisher.request_poll();
    assert!(
        wait_until(10.0, || !usage_bodies(&rig.device).is_empty()),
        "third poll delivers; calls = {}",
        calls.load(Ordering::SeqCst)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        usage_bodies(&rig.device)[0],
        r#"{"s":11,"sr":100,"w":22,"wr":5000,"st":"allowed","acct":"pro","ok":true}"#
    );
    rig.shutdown();
}
