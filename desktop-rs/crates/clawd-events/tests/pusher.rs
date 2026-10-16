//! `StatePusher` against a fake clock device: `test_pusher_sends_on_change_resends_and_clears`,
//! `test_pusher_retries_after_failed_delivery_and_no_targets`,
//! `test_probe_desk_turns_new_picks_on_and_keeps_them_without_answer`, plus the run loop.

use std::sync::Arc;
use std::time::Duration;

use clawd_contracts::{new_stop, Clock, PetConfig, TargetsFn};
use clawd_events::localtime::LocalTime;
use clawd_events::pusher::RESEND_EVERY;
use clawd_events::rng::SystemRng;
use clawd_events::{claude_event, Brain, Pet, SharedBrain, StatePusher};
use clawd_testkit::events::FakeClockServer;
use clawd_testkit::FakeClock;
use serde_json::{json, Value};

const T0: f64 = 1_760_000_000.0;

/// A Brain whose local time is 14:00 (no quiet-hours dim, no evening yawn).
fn brain_at_two_pm() -> SharedBrain {
    let pet = Pet::with_parts(
        PetConfig::default(),
        None,
        Box::new(|_| LocalTime {
            year: 2026,
            month: 10,
            day: 11,
            hour: 14,
            minute: 0,
        }),
        Box::new(SystemRng::seeded(1)),
    );
    Brain::new(pet, Box::new(|| false)).shared()
}

fn hook(name: &str, extra: Value) -> Value {
    let mut payload = json!({"hook_event_name": name, "session_id": "s1"});
    payload
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    claude_event(&payload).expect("a mapped hook")
}

fn pusher(
    brain: &SharedBrain,
    targets: TargetsFn,
    clock: &FakeClock,
    idle: Option<f64>,
) -> StatePusher {
    StatePusher::with_clock(
        Arc::clone(brain),
        targets,
        clock.arc(),
        Arc::new(move || idle),
    )
}

fn one_target(device: &FakeClockServer) -> TargetsFn {
    let url = device.push_url();
    Arc::new(move || vec![url.clone()])
}

#[tokio::test]
async fn sends_on_change_resends_and_clears() {
    let brain = brain_at_two_pm();
    let device = FakeClockServer::start().unwrap();
    let clock = FakeClock::new(T0);
    let p = pusher(&brain, one_target(&device), &clock, Some(1.0));

    assert!(p.step().await, "first state: a clear");
    assert_eq!(
        device.state_bodies().last().unwrap(),
        &json!({"a": "", "ttl": 60})
    );
    assert!(device.requests().last().unwrap().path() == "/api/state");
    assert!(!p.step().await, "nothing changed, nothing live");

    brain.lock().unwrap().ingest(
        &hook("PreToolUse", json!({"tool_name": "Bash"})),
        clock.now(),
    );
    assert!(p.step().await);
    assert_eq!(device.state_bodies().last().unwrap()["a"], "typing");

    clock.advance(5.0);
    assert!(!p.step().await, "unchanged within RESEND_EVERY");
    clock.advance(RESEND_EVERY);
    assert!(p.step().await, "keep-alive");
    assert_eq!(device.state_bodies().last().unwrap()["a"], "typing");

    brain
        .lock()
        .unwrap()
        .ingest(&hook("SessionEnd", json!({})), clock.now());
    // after agent activity: straight back to the usage screens (the minDisplay hold is over)
    clock.advance(2.0);
    assert!(p.step().await);
    assert_eq!(
        device.state_bodies().last().unwrap(),
        &json!({"a": "", "ttl": 60})
    );
    assert_eq!(device.state_bodies().len(), 4);
}

#[tokio::test]
async fn the_body_is_python_compact_json_with_the_content_type() {
    let brain = brain_at_two_pm();
    let device = FakeClockServer::start().unwrap();
    let clock = FakeClock::new(T0);
    let p = pusher(&brain, one_target(&device), &clock, Some(1.0));
    brain.lock().unwrap().ingest(
        &hook("PreToolUse", json!({"tool_name": "Bash"})),
        clock.now(),
    );
    assert!(p.step().await);
    let request = device
        .requests()
        .into_iter()
        .find(|r| r.path() == "/api/state")
        .unwrap();
    assert_eq!(
        String::from_utf8(request.body.clone()).unwrap(),
        r#"{"a":"typing","ttl":60,"v":["typing boss"],"txt":"Running"}"#
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.method, "POST");
}

#[tokio::test]
async fn retries_after_failed_delivery_and_does_nothing_without_targets() {
    let brain = brain_at_two_pm();
    let device = FakeClockServer::start().unwrap();
    device.set_state_status(500);
    let clock = FakeClock::new(T0);
    let p = pusher(&brain, one_target(&device), &clock, None);
    assert!(!p.step().await);
    assert!(!p.step().await, "still pending: tried again");
    assert_eq!(device.state_bodies().len(), 2);
    device.set_state_status(200);
    assert!(p.step().await, "delivered once the clock answers");
    assert!(!p.step().await);

    let idle = pusher(&brain, Arc::new(Vec::new), &clock, None);
    assert!(!idle.step().await);
}

#[tokio::test]
async fn an_unreachable_clock_is_a_failed_push_not_a_crash() {
    let brain = brain_at_two_pm();
    let clock = FakeClock::new(T0);
    // a port nobody listens on
    let dead = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let target = format!("http://127.0.0.1:{dead}/api/usage");
    let p = pusher(&brain, Arc::new(move || vec![target.clone()]), &clock, None);
    assert!(!p.step().await);
}

#[tokio::test]
async fn invalidate_makes_the_next_step_resend() {
    let brain = brain_at_two_pm();
    let device = FakeClockServer::start().unwrap();
    let clock = FakeClock::new(T0);
    let p = pusher(&brain, one_target(&device), &clock, None);
    assert!(p.step().await);
    assert!(!p.step().await);
    p.invalidate(); // a clock that restarted lost the state
    assert!(p.step().await);
    assert_eq!(device.state_bodies().len(), 2);
}

#[tokio::test]
async fn several_clocks_all_get_the_state_and_one_taker_is_enough() {
    let brain = brain_at_two_pm();
    let (a, b) = (
        FakeClockServer::start().unwrap(),
        FakeClockServer::start().unwrap(),
    );
    b.set_state_status(503);
    let (url_a, url_b) = (a.push_url(), b.push_url());
    let clock = FakeClock::new(T0);
    let p = pusher(
        &brain,
        Arc::new(move || vec![url_a.clone(), url_b.clone()]),
        &clock,
        None,
    );
    assert!(p.step().await);
    assert_eq!((a.state_bodies().len(), b.state_bodies().len()), (1, 1));
}

#[tokio::test]
async fn probe_desk_turns_new_picks_on_and_keeps_them_without_answer() {
    let brain = brain_at_two_pm();
    let device = FakeClockServer::start().unwrap();
    let clock = FakeClock::new(T0);
    let p = pusher(&brain, one_target(&device), &clock, None);
    device.set_desk(Some(31));
    p.probe_desk().await;
    assert!(brain.lock().unwrap().pet.desk_new);
    device.set_desk(Some(30));
    p.probe_desk().await;
    assert!(!brain.lock().unwrap().pet.desk_new);
    brain.lock().unwrap().pet.desk_new = true;
    device.set_desk(None); // 503: no answer keeps what was known
    p.probe_desk().await;
    assert!(brain.lock().unwrap().pet.desk_new);
    let heap = device
        .requests()
        .into_iter()
        .filter(|r| r.path() == "/api/heap")
        .count();
    assert_eq!(heap, 3);
}

#[tokio::test]
async fn the_lowest_desk_count_of_all_clocks_decides() {
    let brain = brain_at_two_pm();
    let (new, old) = (
        FakeClockServer::start().unwrap(),
        FakeClockServer::start().unwrap(),
    );
    new.set_desk(Some(40));
    old.set_desk(Some(20));
    let (url_new, url_old) = (new.push_url(), old.push_url());
    let clock = FakeClock::new(T0);
    let p = pusher(
        &brain,
        Arc::new(move || vec![url_new.clone(), url_old.clone()]),
        &clock,
        None,
    );
    brain.lock().unwrap().pet.desk_new = true;
    p.probe_desk().await;
    assert!(!brain.lock().unwrap().pet.desk_new);
}

#[tokio::test]
async fn run_pushes_on_wake_and_stops_on_the_stop_signal() {
    let brain = brain_at_two_pm();
    let device = FakeClockServer::start().unwrap();
    let clock = FakeClock::new(T0);
    let p = Arc::new(pusher(&brain, one_target(&device), &clock, Some(1.0)));
    let (stop_tx, stop_rx) = new_stop();
    let runner = tokio::spawn({
        let p = Arc::clone(&p);
        async move { p.run(stop_rx).await }
    });

    wait_until(|| !device.state_bodies().is_empty()).await;
    assert_eq!(device.state_bodies()[0], json!({"a": "", "ttl": 60}));
    // the desk probe ran at start (the fake clock reports 31 animations)
    wait_until(|| brain.lock().unwrap().pet.desk_new).await;

    let wake = {
        let mut b = brain.lock().unwrap();
        b.ingest(
            &hook("PreToolUse", json!({"tool_name": "Grep"})),
            clock.now(),
        );
        Arc::clone(&b.wake)
    };
    wake.notify_one();
    wait_until(|| device.state_bodies().iter().any(|b| b["a"] == "reading")).await;

    stop_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(3), runner)
        .await
        .expect("run returns after stop")
        .unwrap();
}

async fn wait_until(mut condition: impl FnMut() -> bool) {
    for _ in 0..100 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition not met within 5 s");
}
