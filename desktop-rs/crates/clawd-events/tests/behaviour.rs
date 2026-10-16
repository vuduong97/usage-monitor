//! Behaviour the goldens cannot pin because it is random or touches files: idle-pick rotation
//! with a real RNG, question phrases, `pet.json`, the turn callback.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use clawd_contracts::PetConfig;
use clawd_events::localtime::LocalTime;
use clawd_events::pet::{pass_secs, IDLE_GAP, IDLE_PICKS, IDLE_PICKS_NEW};
use clawd_events::rng::SystemRng;
use clawd_events::tracker::{Tracker, ATTENTION_TTL};
use clawd_events::vocab::QUESTION_TEXTS;
use clawd_events::{pi_event, Brain, Pet};
use serde_json::{json, Value};

const T0: f64 = 1_760_000_000.0;

fn at(hour: u32, minute: u32, day: u32) -> Box<dyn Fn(f64) -> LocalTime + Send + Sync> {
    Box::new(move |_| LocalTime {
        year: 2026,
        month: 10,
        day,
        hour,
        minute,
    })
}

fn idle_brain(seed: u64) -> Brain {
    let pet = Pet::with_parts(
        PetConfig::default(),
        None,
        at(14, 0, 11),
        Box::new(SystemRng::seeded(seed)),
    );
    Brain::new(pet, Box::new(|| false))
}

/// `[(t, anim, seq)]` every time `desired()` changes (the Python test's `run_idle`).
fn run_idle(brain: &mut Brain, start: f64, secs: f64) -> Vec<(f64, String, Vec<String>)> {
    let (mut out, mut last, mut t) = (Vec::new(), None, start);
    while t < start + secs {
        let d = brain.desired(t);
        let key = (d.a.clone(), d.seq.clone());
        if last.as_ref() != Some(&key) {
            out.push((t, d.a.clone(), d.seq.clone()));
            last = Some(key);
        }
        t += 0.5;
    }
    out
}

fn names_of(anim: &str, seq: &[String]) -> Vec<String> {
    if seq.len() > 1 {
        seq.to_vec()
    } else {
        vec![anim.to_owned()]
    }
}

#[test]
fn idle_picks_slip_in_between_usage_screens() {
    for name in IDLE_PICKS.iter().flat_map(|p| p.iter()) {
        let _ = pass_secs(name); // every pick has a pass length
    }
    let tied = [
        "reading",
        "thinking",
        "ultrathink",
        "typing",
        "typing boss",
        "building",
        "wizard",
        "debugger",
        "headphones",
        "juggling",
        "carrying",
        "sweeping",
        "error",
        "dizzy",
        "notification",
        "happy",
        "react left",
        "react right",
        "annoyed",
    ];
    assert!(IDLE_PICKS
        .iter()
        .flat_map(|p| p.iter())
        .all(|n| !tied.contains(n)));

    let mut brain = idle_brain(1);
    brain.pet.desk_new = true;
    let changes = run_idle(&mut brain, T0, 3.0 * 3600.0);
    assert_eq!(changes[0].1, "", "starts on the usage screens");
    let picks: Vec<_> = changes.iter().filter(|c| !c.1.is_empty()).collect();
    assert!(picks.len() >= 25, "{} picks", picks.len());
    for pair in changes.windows(2) {
        let ((t0, a, seq), (t1, a1, _)) = (&pair[0], &pair[1]);
        if !a.is_empty() {
            // one pass, then usage again
            let secs: f64 = names_of(a, seq).iter().map(|n| pass_secs(n)).sum();
            assert!(
                a1.is_empty() && ((t1 - t0) - secs).abs() <= 0.5,
                "{a} {}",
                t1 - t0
            );
        } else {
            // a gap of 3-6 minutes
            let gap = t1 - t0;
            assert!(
                IDLE_GAP.0 - 0.5 <= gap && gap <= IDLE_GAP.1 + 0.5,
                "gap {gap}"
            );
        }
    }
    let names: Vec<Vec<String>> = picks.iter().map(|c| names_of(&c.1, &c.2)).collect();
    assert!(
        names.windows(2).all(|w| w[0] != w[1]),
        "never the same pick twice in a row"
    );
    let seen: HashSet<Vec<String>> = names.iter().cloned().collect();
    let all: HashSet<Vec<String>> = IDLE_PICKS
        .iter()
        .map(|p| p.iter().map(|s| (*s).to_owned()).collect())
        .collect();
    assert_eq!(seen, all);
    assert!(names.contains(&vec!["yawn".to_owned(), "sleeping".into(), "wake".into()]));
}

#[test]
fn an_older_desk_pak_skips_the_new_animations() {
    let mut brain = idle_brain(2);
    let picks: Vec<_> = run_idle(&mut brain, T0, 3.0 * 3600.0)
        .into_iter()
        .filter(|c| !c.1.is_empty())
        .map(|c| names_of(&c.1, &c.2))
        .collect();
    assert!(!picks.is_empty());
    assert!(picks
        .iter()
        .all(|p| !p.iter().any(|n| IDLE_PICKS_NEW.contains(&n.as_str()))));
}

#[test]
fn question_phrase_is_picked_once_per_question_and_never_twice_in_a_row() {
    assert!(QUESTION_TEXTS.iter().all(|t| t.len() <= 20 && t.is_ascii()));
    let mut tracker = Tracker::with_rng(None, Box::new(SystemRng::seeded(7)));
    let mut event = |tracker: &mut Tracker, ev: &str, extra: Value| {
        let mut payload = json!({"ev": ev, "sid": "p1"});
        payload
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        tracker.ingest(&pi_event(&payload).unwrap(), T0)
    };
    let mut seen = Vec::new();
    for _ in 0..40 {
        event(
            &mut tracker,
            "attention",
            json!({"tool": "ask_user_question"}),
        );
        let first = tracker.view(T0).unwrap().text;
        event(&mut tracker, "attention", json!({})); // a generic nudge: same phrase
        assert_eq!(tracker.view(T0).unwrap().text, first);
        event(&mut tracker, "attention_seen", json!({}));
        event(&mut tracker, "attention", json!({})); // calls again while open: same phrase
        assert_eq!(tracker.view(T0).unwrap().text, first);
        event(&mut tracker, "attention_done", json!({}));
        seen.push(first);
    }
    assert!(seen.windows(2).all(|w| w[0] != w[1]));
    assert_eq!(
        seen.iter().map(String::as_str).collect::<HashSet<_>>(),
        QUESTION_TEXTS.iter().copied().collect::<HashSet<_>>()
    );
}

#[test]
fn a_question_expires_with_the_attention_ttl() {
    let mut tracker = Tracker::new(None);
    tracker.ingest(
        &pi_event(&json!({"ev": "attention", "sid": "p1", "tool": "ask_user_question"})).unwrap(),
        T0,
    );
    assert!(tracker.view(T0).is_some());
    assert!(tracker.view(T0 + ATTENTION_TTL + 1.0).is_none());
    assert!(!tracker.sessions["pi:p1"].asking);
}

#[test]
fn turn_callback_fires_for_stop_of_main_sessions_only() {
    let turns = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&turns);
    let mut tracker = Tracker::new(Some(Box::new(move |now| seen.lock().unwrap().push(now))));
    let mut ev = |kind: &str, sid: &str, sub: bool, at: f64| {
        tracker.ingest(
            &pi_event(&json!({"ev": kind, "sid": sid, "sub": sub})).unwrap(),
            at,
        );
    };
    ev("prompt", "main", false, T0);
    ev("stop", "child", true, T0 + 1.0);
    ev("abort", "main", false, T0 + 2.0);
    assert!(turns.lock().unwrap().is_empty());
    ev("stop", "main", false, T0 + 3.0);
    assert_eq!(*turns.lock().unwrap(), vec![T0 + 3.0]);
}

#[test]
fn the_brain_counts_a_turn_when_a_session_stops() {
    let mut brain = idle_brain(1);
    brain.ingest(&pi_event(&json!({"ev": "prompt", "sid": "a"})).unwrap(), T0);
    brain.ingest(
        &pi_event(&json!({"ev": "stop", "sid": "a"})).unwrap(),
        T0 + 1.0,
    );
    assert_eq!(brain.pet.turns, 1);
    assert_eq!(brain.state_body(T0 + 1.0)["turns"], 1);
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("clawd-events-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn pet_json_keeps_greeting_and_turns_across_restarts() {
    let dir = scratch("pet");
    let store = dir.join("pet.json");
    let pet = |hour, day| {
        Pet::with_parts(
            PetConfig::default(),
            Some(store.clone()),
            at(hour, 30, day),
            Box::new(SystemRng::seeded(1)),
        )
    };

    // away, then back in the morning: "Good morning!" once, the greeting day is saved
    let mut p = pet(8, 11);
    p.observe(T0, Some(10.0), None);
    p.observe(T0 + 1.0, Some(301.0), None);
    p.observe(T0 + 100.0, Some(2.0), None);
    assert_eq!(p.moment(T0 + 100.0).unwrap().text, "Good morning!");
    p.observe(T0 + 200.0, Some(400.0), None);
    p.observe(T0 + 300.0, Some(1.0), None);
    assert_eq!(p.moment(T0 + 300.0).unwrap().text, "Welcome back!");
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&store).unwrap()).unwrap();
    assert_eq!(saved["greeted"], "2026-10-11");

    // ten turns: the milestone, persisted in Python's `json.dumps` form
    let mut p = pet(15, 11);
    for _ in 0..10 {
        p.on_turn(T0);
    }
    assert_eq!(p.moment(T0).unwrap().text, "10 turns today!");
    assert_eq!(
        std::fs::read_to_string(&store).unwrap(),
        r#"{"day": "2026-10-11", "turns": 10, "greeted": "2026-10-11"}"#
    );
    assert_eq!(
        pet(15, 11).turns,
        10,
        "a restart the same day keeps the count"
    );
    let mut next_day = pet(9, 12);
    next_day.on_turn(T0);
    assert_eq!(next_day.turns, 1, "a new day starts over");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_damaged_pet_json_starts_empty_and_a_missing_folder_is_created() {
    let dir = scratch("damaged");
    std::fs::create_dir_all(&dir).unwrap();
    let store = dir.join("sub").join("pet.json");
    let mut p = Pet::with_parts(
        PetConfig::default(),
        Some(store.clone()),
        at(12, 0, 11),
        Box::new(SystemRng::seeded(1)),
    );
    assert_eq!(p.turns, 0);
    p.on_turn(T0);
    assert!(store.exists(), "the folder was created");
    for broken in ["not json", "[1,2]", r#"{"turns": "x"}"#, ""] {
        std::fs::write(&store, broken).unwrap();
        let p = Pet::with_parts(
            PetConfig::default(),
            Some(store.clone()),
            at(12, 0, 11),
            Box::new(SystemRng::seeded(1)),
        );
        assert_eq!(p.turns, 0, "{broken:?}");
    }
    std::fs::write(&store, r#"{"day": "2026-10-11", "turns": "7"}"#).unwrap();
    let p = Pet::with_parts(
        PetConfig::default(),
        Some(store.clone()),
        at(12, 0, 11),
        Box::new(SystemRng::seeded(1)),
    );
    assert_eq!(p.turns, 7, "Python int() accepts a numeric string");
    std::fs::remove_dir_all(&dir).unwrap();
}
