//! Ported from `tests/test_attention.py` and the non-golden half of `tests/test_codex_usage.py`.
//! (The attention ages, `to_ascii`, `encode_payload` and Codex mapping cases are golden: `golden.rs`.)

mod common;

use std::collections::HashMap;

use clawd_contracts::Env;
use clawd_usage::codex;
use clawd_usage::payload::{
    encode_payload, take_attention_flag, take_clears, to_ascii, ATTENTION_MAX_AGE,
};
use common::{payload, set_mtime, Fx, NOW};
use serde_json::json;

#[test]
fn attention_flag_lifecycle() {
    let fx = Fx::new();
    assert_eq!(take_attention_flag(&fx.env, None), None); // absent

    let flag = fx.cfg.write_config("attention", "");
    set_mtime(&flag, NOW);
    assert_eq!(take_attention_flag(&fx.env, None).unwrap().m, ""); // a bare touch still beeps
    assert!(!flag.exists(), "consumed");

    let flag = fx.cfg.write_config(
        "attention",
        json!({"m": "Você  aprova\na ação?", "s": "abc"}).to_string(),
    );
    set_mtime(&flag, NOW);
    let alert = take_attention_flag(&fx.env, None).unwrap();
    assert_eq!(
        (alert.m.as_str(), alert.s.as_str()),
        ("Voce aprova a acao?", "abc")
    );
    assert_eq!(alert.t, NOW);
    assert_eq!(
        to_ascii(&"x".repeat(200), 120),
        format!("{}...", "x".repeat(117))
    );

    let flag = fx.cfg.write_config("attention", "");
    set_mtime(&flag, NOW - ATTENTION_MAX_AGE - 1.0);
    assert_eq!(take_attention_flag(&fx.env, None), None, "stale");
    assert!(!flag.exists(), "a stale flag is still consumed");
}

#[test]
fn clears_are_consumed() {
    let fx = Fx::new();
    let clear = fx.cfg.write_config("clear-abc", "");
    set_mtime(&clear, NOW - 3.0);
    fx.cfg.write_config("attention", "");
    fx.cfg.write_config("config", "");
    let clears = take_clears(&fx.env);
    assert_eq!(clears, HashMap::from([("abc".to_owned(), NOW - 3.0)]));
    assert!(take_clears(&fx.env).is_empty(), "consumed");
    assert!(
        fx.cfg.paths().attention_flag().exists(),
        "only clear-* files are touched"
    );
}

#[test]
fn encode_payload_fits() {
    let base = json!({"s": 4, "ok": true, "b": 1});
    let with_m = |m: &str| {
        let mut p = payload(base.clone());
        p.insert("m".into(), json!(m));
        p
    };
    let short = encode_payload(&with_m("hi"), 180);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&short).unwrap()["m"],
        "hi"
    );

    let long = encode_payload(&with_m(&"x".repeat(200)), 120);
    assert!(long.len() <= 120);
    let long: serde_json::Value = serde_json::from_slice(&long).unwrap();
    assert!(long["m"].as_str().unwrap().ends_with("..."));

    let tiny = encode_payload(&with_m(&"x".repeat(200)), 26);
    assert!(serde_json::from_slice::<serde_json::Value>(&tiny)
        .unwrap()
        .get("m")
        .is_none());
}

#[test]
fn encode_payload_never_splits_a_character() {
    let mut p = payload(json!({"s": 1}));
    p.insert("m".into(), json!("ä".repeat(60)));
    let bytes = encode_payload(&p, 50);
    assert!(bytes.len() <= 50 && bytes.is_ascii());
    serde_json::from_slice::<serde_json::Value>(&bytes).expect("valid JSON");
}

#[test]
fn codex_home_moves_the_sessions_dir() {
    let fx = Fx::new();
    let paths = fx.cfg.paths();
    let env = Env::from_map(HashMap::from([(
        "CODEX_HOME".to_owned(),
        "/x/cx".to_owned(),
    )]));
    assert_eq!(
        codex::sessions_dir(paths, &env),
        std::path::PathBuf::from("/x/cx/sessions")
    );
    let tilde = Env::from_map(HashMap::from([(
        "CODEX_HOME".to_owned(),
        "~/cx".to_owned(),
    )]));
    assert_eq!(
        codex::sessions_dir(paths, &tilde),
        paths.home().join("cx").join("sessions")
    );
    assert_eq!(
        codex::sessions_dir(paths, &Env::empty()),
        paths.home().join(".codex").join("sessions")
    );
}

#[test]
fn no_codex_no_data() {
    let fx = Fx::new();
    assert!(codex::read(fx.cfg.paths(), &Env::empty(), Some(NOW)).is_none());
    assert!(!codex::available(fx.cfg.paths(), &Env::empty()));
}

#[test]
fn codex_home_env_is_read_through_the_injected_env() {
    let fx = Fx::new();
    let moved = fx.cfg.root().join("elsewhere");
    let rl =
        json!({"primary": {"used_percent": 9.0, "window_minutes": 300, "resets_at": NOW + 600.0}});
    let row = json!({"timestamp": "2026-10-11T09:00:00Z", "payload": {"rate_limits": rl}});
    std::fs::create_dir_all(moved.join("sessions/2026/10/11")).unwrap();
    std::fs::write(
        moved.join("sessions/2026/10/11/rollout-x.jsonl"),
        format!("{row}\n"),
    )
    .unwrap();
    let env = Env::from_map(HashMap::from([(
        "CODEX_HOME".to_owned(),
        moved.display().to_string(),
    )]));
    assert_eq!(
        codex::read(fx.cfg.paths(), &env, Some(NOW)).unwrap().s(),
        Some(9)
    );
    assert!(codex::read(fx.cfg.paths(), &Env::empty(), Some(NOW)).is_none());
}

#[test]
fn codex_only_the_tail_of_a_big_session_file_is_read() {
    let fx = Fx::new();
    let rl =
        json!({"primary": {"used_percent": 4.0, "window_minutes": 300, "resets_at": NOW + 600.0}});
    let row = json!({"timestamp": "2026-10-11T09:00:00Z", "payload": {"rate_limits": rl}});
    let filler = "x".repeat(1000);
    // the limits row is before the last TAIL bytes: not seen; a newer one after the filler is
    let mut text = format!("{row}\n");
    for _ in 0..(codex::TAIL / 1000 + 2) {
        text.push_str(&format!("{{\"pad\":\"{filler}\"}}\n"));
    }
    let path = fx
        .cfg
        .write_home(".codex/sessions/2026/10/11/rollout-big.jsonl", &text);
    assert!(codex::read(fx.cfg.paths(), &Env::empty(), Some(NOW)).is_none());
    let newer = json!({"timestamp": "2026-10-11T09:30:00Z", "payload": {"rate_limits": json!({"primary": {"used_percent": 6.0, "window_minutes": 300, "resets_at": NOW + 600.0}})}});
    std::fs::write(&path, format!("{text}{newer}\n")).unwrap();
    assert_eq!(
        codex::read(fx.cfg.paths(), &Env::empty(), Some(NOW))
            .unwrap()
            .s(),
        Some(6)
    );
}

#[test]
fn codex_only_looks_at_the_newest_files() {
    let fx = Fx::new();
    let rl =
        json!({"primary": {"used_percent": 4.0, "window_minutes": 300, "resets_at": NOW + 600.0}});
    let row = json!({"timestamp": "2026-10-11T09:00:00Z", "payload": {"rate_limits": rl}});
    fx.cfg.write_home(
        ".codex/sessions/2026/01/01/rollout-old.jsonl",
        format!("{row}\n"),
    );
    for i in 0..codex::FILES {
        fx.cfg.write_home(
            &format!(".codex/sessions/2026/10/{:02}/rollout-{i}.jsonl", i + 1),
            "{\"type\":\"response_item\"}\n",
        );
    }
    let root = codex::sessions_dir(fx.cfg.paths(), &Env::empty());
    assert_eq!(codex::newest_files(&root, 3).len(), 3);
    assert!(
        codex::read(fx.cfg.paths(), &Env::empty(), Some(NOW)).is_none(),
        "FILES newest files have no limits"
    );
}
