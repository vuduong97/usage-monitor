//! Skill animation rules end to end: hook payloads -> events -> the Tracker's view, in the
//! priority the owner chose (ultrathink > testing > debugging > reading > skill rule > typing
//! boss > wizard > typing; a rule also replaces "thinking"; questions, errors, compaction and
//! many agents are untouched).

use clawd_events::{claude_event_with_skill, pi_event, SkillRule, SkillRules, Tracker};
use serde_json::{json, Value};

const T0: f64 = 1_000_000.0;

fn rules(list: &[(&str, &str, &str)]) -> SkillRules {
    let list: Vec<SkillRule> = list
        .iter()
        .map(|(skill, anim, text)| SkillRule {
            skill: (*skill).into(),
            anim: (*anim).into(),
            text: (*text).into(),
        })
        .collect();
    let (rules, problems) = SkillRules::from_rules(&list);
    assert!(problems.is_empty(), "{problems:?}");
    rules
}

fn tracker(list: &[(&str, &str, &str)]) -> Tracker {
    let mut t = Tracker::new(None);
    t.rules = rules(list);
    t.desk_new = true;
    t
}

/// Feed one Claude hook payload for session `sid` at `T0 + t`.
fn claude(t: &mut Tracker, at: f64, sid: &str, name: &str, extra: Value) {
    let mut p = json!({"hook_event_name": name, "session_id": sid});
    p.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let ev = claude_event_with_skill(&p).expect("a Claude event");
    assert!(t.ingest(&ev, T0 + at));
}

fn tool(t: &mut Tracker, at: f64, name: &str, input: Value) {
    claude(
        t,
        at,
        "s1",
        "PreToolUse",
        json!({"tool_name": name, "tool_input": input}),
    );
}

fn done(t: &mut Tracker, at: f64, name: &str) {
    claude(t, at, "s1", "PostToolUse", json!({"tool_name": name}));
}

/// `(anim, text)` shown at `T0 + at`.
fn shown(t: &mut Tracker, at: f64) -> (String, String) {
    let v = t.view(T0 + at).expect("a view");
    (v.anim, v.text)
}

fn pair(anim: &str, text: &str) -> (String, String) {
    (anim.to_owned(), text.to_owned())
}

#[test]
fn a_cook_turn_follows_the_chosen_priority() {
    let mut t = tracker(&[
        ("ak-cook", "wizard", "Cooking"),
        ("ak-journal", "happy", "Journal"),
    ]);
    claude(
        &mut t,
        0.0,
        "s1",
        "UserPromptSubmit",
        json!({"prompt": "/ak-cook build the tab"}),
    );
    assert_eq!(
        shown(&mut t, 0.0),
        pair("wizard", "Cooking"),
        "a rule replaces thinking"
    );

    tool(&mut t, 1.0, "Read", json!({"file_path": "src/a.rs"}));
    assert_eq!(
        shown(&mut t, 1.0),
        pair("reading", "Reading"),
        "reading wins"
    );
    done(&mut t, 2.0, "Read");

    tool(&mut t, 3.0, "Edit", json!({"file_path": "src/a.rs"}));
    assert_eq!(shown(&mut t, 3.0), pair("wizard", "Cooking"));
    done(&mut t, 4.0, "Edit");

    tool(&mut t, 5.0, "Skill", json!({"skill": "ak:journal"}));
    assert_eq!(
        shown(&mut t, 5.0),
        pair("wizard", "Cooking"),
        "the typed skill keeps its rule"
    );
    done(&mut t, 6.0, "Skill");

    tool(&mut t, 7.0, "Bash", json!({"command": "cargo test -p x"}));
    assert_eq!(
        shown(&mut t, 7.0),
        pair("debugger", "Testing"),
        "a test run wins"
    );
    done(&mut t, 8.0, "Bash");

    tool(&mut t, 9.0, "Bash", json!({"command": "ls"}));
    assert_eq!(shown(&mut t, 9.0), pair("wizard", "Cooking"));
    done(&mut t, 10.0, "Bash");

    assert_eq!(
        shown(&mut t, 41.0),
        pair("ultrathink", "Thinking hard"),
        "30 s without a tool: ultrathink wins"
    );

    claude(&mut t, 42.0, "s1", "Stop", json!({}));
    assert_eq!(shown(&mut t, 42.0), pair("happy", "Done!"));

    // the next turn has no skill: plain behaviour again
    claude(
        &mut t,
        60.0,
        "s1",
        "UserPromptSubmit",
        json!({"prompt": "hello"}),
    );
    assert_eq!(shown(&mut t, 60.0), pair("thinking", "Thinking"));
    tool(&mut t, 61.0, "Edit", json!({}));
    assert_eq!(shown(&mut t, 61.0).0, "typing");
}

#[test]
fn errors_and_debug_turns_beat_a_rule() {
    let mut t = tracker(&[("ak-cook", "wizard", "Cooking")]);
    claude(
        &mut t,
        0.0,
        "s1",
        "UserPromptSubmit",
        json!({"prompt": "/ak-cook"}),
    );
    tool(&mut t, 1.0, "Edit", json!({}));
    claude(
        &mut t,
        2.0,
        "s1",
        "PostToolUseFailure",
        json!({"tool_name": "Edit"}),
    );
    assert_eq!(shown(&mut t, 2.0), pair("error", "Edit failed"));
    tool(&mut t, 8.0, "Edit", json!({}));
    assert_eq!(
        shown(&mut t, 8.0),
        pair("debugger", "Debugging"),
        "a failure makes it a debug turn"
    );
}

#[test]
fn questions_and_permission_prompts_beat_a_rule() {
    let mut t = tracker(&[("ak-cook", "wizard", "Cooking")]);
    claude(
        &mut t,
        0.0,
        "s1",
        "UserPromptSubmit",
        json!({"prompt": "/ak-cook"}),
    );
    claude(
        &mut t,
        1.0,
        "s1",
        "PermissionRequest",
        json!({"tool_name": "Bash", "tool_input": {"command": "rm x"}}),
    );
    assert_eq!(shown(&mut t, 1.0), pair("notification", "Allow Bash: rm x"));
}

#[test]
fn a_skill_the_agent_loads_counts_when_the_typed_one_has_no_rule() {
    let mut t = tracker(&[("ak-plan", "carrying", "Planning")]);
    claude(
        &mut t,
        0.0,
        "s1",
        "UserPromptSubmit",
        json!({"prompt": "/ak-advise x"}),
    );
    assert_eq!(shown(&mut t, 0.0), pair("thinking", "Thinking"));
    tool(&mut t, 1.0, "Skill", json!({"skill": "ak:plan"}));
    assert_eq!(shown(&mut t, 1.0), pair("carrying", "Planning"));
    done(&mut t, 2.0, "Skill");
    tool(&mut t, 3.0, "Write", json!({}));
    assert_eq!(shown(&mut t, 3.0), pair("carrying", "Planning"));
}

#[test]
fn empty_text_keeps_what_clawd_would_say() {
    let mut t = tracker(&[("ak-cook", "building", "")]);
    claude(
        &mut t,
        0.0,
        "s1",
        "UserPromptSubmit",
        json!({"prompt": "/ak:cook"}),
    );
    assert_eq!(shown(&mut t, 0.0), pair("building", "Thinking"));
    tool(&mut t, 1.0, "Edit", json!({}));
    assert_eq!(shown(&mut t, 1.0), pair("building", "Editing"));
    tool(&mut t, 2.0, "Bash", json!({"command": "ls"}));
    assert_eq!(shown(&mut t, 2.0), pair("building", "Running"));
}

#[test]
fn a_rule_beats_typing_boss_and_the_long_task_wizard() {
    let mut t = tracker(&[("ak-cook", "carrying", "Cooking")]);
    claude(
        &mut t,
        0.0,
        "s1",
        "UserPromptSubmit",
        json!({"prompt": "/ak-cook"}),
    );
    for i in 0..12 {
        tool(&mut t, 1.0 + f64::from(i), "Edit", json!({}));
    }
    assert_eq!(shown(&mut t, 13.0), pair("carrying", "Cooking"));
    tool(&mut t, 1900.0, "Edit", json!({}));
    assert_eq!(shown(&mut t, 1900.0), pair("carrying", "Cooking"));
}

#[test]
fn many_agents_keep_their_animation() {
    let mut t = tracker(&[("ak-cook", "wizard", "Cooking")]);
    claude(
        &mut t,
        0.0,
        "s1",
        "UserPromptSubmit",
        json!({"prompt": "hi"}),
    );
    tool(&mut t, 0.5, "Edit", json!({}));
    claude(
        &mut t,
        1.0,
        "s2",
        "UserPromptSubmit",
        json!({"prompt": "/ak-cook"}),
    );
    claude(
        &mut t,
        1.5,
        "s2",
        "PreToolUse",
        json!({"tool_name": "Edit"}),
    );
    assert_eq!(shown(&mut t, 2.0), pair("headphones", "2 agents"));
}

#[test]
fn an_older_desk_pak_never_gets_a_newer_animation() {
    let mut t = tracker(&[("ak-cook", "coffee hand", "Cooking")]);
    t.desk_new = false;
    claude(
        &mut t,
        0.0,
        "s1",
        "UserPromptSubmit",
        json!({"prompt": "/ak-cook"}),
    );
    tool(&mut t, 1.0, "Edit", json!({}));
    assert_eq!(shown(&mut t, 1.0), pair("typing", "Editing"));
    t.desk_new = true;
    assert_eq!(shown(&mut t, 1.0), pair("coffee hand", "Cooking"));
}

#[test]
fn pi_sends_the_same_names() {
    let mut t = tracker(&[("ak-cook", "wizard", "Cooking")]);
    let pi = |t: &mut Tracker, at: f64, body: Value| {
        assert!(t.ingest(&pi_event(&body).unwrap(), T0 + at));
    };
    pi(
        &mut t,
        0.0,
        json!({"ev": "prompt", "sid": "S", "skill": "ak-cook"}),
    );
    pi(
        &mut t,
        1.0,
        json!({"ev": "tool", "sid": "S", "tool": "edit"}),
    );
    assert_eq!(shown(&mut t, 1.0), pair("wizard", "Cooking"));
    pi(
        &mut t,
        2.0,
        json!({"ev": "tool", "sid": "S", "tool": "grep"}),
    );
    assert_eq!(shown(&mut t, 2.0), pair("reading", "Searching"));
    pi(&mut t, 3.0, json!({"ev": "stop", "sid": "S"}));
    assert_eq!(shown(&mut t, 3.0), pair("happy", "Done!"));
}

#[test]
fn the_brain_follows_the_rules_file_without_a_restart() {
    use clawd_events::skill_rules::{save_rules, RULES_CHECK_EVERY};
    use clawd_events::{Brain, Pet, PetConfig};

    let dir = std::env::temp_dir().join(format!("clawd-brain-rules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("animation-rules.json");
    let mut brain = Brain::new(Pet::new(PetConfig::default(), None), Box::new(|| false));
    brain.pet.desk_new = true;
    brain.watch_skill_rules(path.clone(), T0);
    let ev = |body: Value| pi_event(&body).unwrap();
    brain.ingest(
        &ev(json!({"ev": "prompt", "sid": "S", "skill": "ak-cook"})),
        T0,
    );
    brain.ingest(
        &ev(json!({"ev": "tool", "sid": "S", "tool": "edit"})),
        T0 + 1.0,
    );
    assert_eq!(brain.desired(T0 + 2.0).a, "typing", "no rules yet");

    save_rules(&path, &rules(&[("ak-cook", "wizard", "Cooking")])).unwrap();
    brain.ingest(
        &ev(json!({"ev": "tool", "sid": "S", "tool": "edit"})),
        T0 + 3.0,
    );
    let d = brain.desired(T0 + 3.0 + RULES_CHECK_EVERY);
    assert_eq!((d.a.as_str(), d.txt.as_str()), ("wizard", "Cooking"));

    save_rules(&path, &SkillRules::default()).unwrap();
    brain.ingest(
        &ev(json!({"ev": "tool", "sid": "S", "tool": "edit"})),
        T0 + 10.0,
    );
    assert_eq!(
        brain.desired(T0 + 10.0 + RULES_CHECK_EVERY).a,
        "typing",
        "rules removed"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
