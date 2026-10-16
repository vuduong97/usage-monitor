//! `firstrun.py`: the setup wizard's bookkeeping, kept in `desktop.json`.
//!
//! * `setup_done`: wizard steps finished or skipped ([`STEPS`]). A fresh install has none, so
//!   it sees every step. 0.1.x had only `first_run_done` (its users already found the clock),
//!   so an upgrade sees only the new steps.
//! * `setup_shown`: the app version that last opened the wizard by itself, so it opens once per
//!   version while steps are left.
//! * `agents_seen`: agents already offered to the user; a newly installed agent is offered
//!   once, never again whatever the answer.

use std::collections::BTreeSet;
use std::io;

use clawd_contracts::{AppPaths, Prefs, APP_VERSION};
use serde_json::Value;

use crate::prefs::{load_prefs, update_prefs};
use crate::pyjson::py_float_repr;

/// `firstrun.STEPS`.
pub const STEPS: [&str; 3] = ["clock", "agents", "usage"];

/// `firstrun.migrate(prefs)`: `prefs` with `setup_done` (from `first_run_done` when that is all
/// there is). Returns a copy; never changes other keys.
pub fn migrate(prefs: &Prefs) -> Prefs {
    let mut out = prefs.clone();
    if !prefs.has_setup_done_list() {
        let done = if prefs.first_run_done() {
            vec![Value::from("clock")]
        } else {
            Vec::new()
        };
        out.insert("setup_done".to_owned(), Value::Array(done));
    }
    out
}

/// `firstrun.load()`: prefs after persisting the migration (one `update_prefs` when needed).
pub fn load(paths: &AppPaths) -> io::Result<Prefs> {
    let prefs = load_prefs(paths);
    if prefs.has_setup_done_list() {
        return Ok(prefs);
    }
    update_prefs(paths, |prefs| *prefs = migrate(prefs))
}

/// `firstrun.pending(prefs)`: the steps not done yet, in [`STEPS`] order. `None` = read the file.
pub fn pending(paths: &AppPaths, prefs: Option<&Prefs>) -> Vec<String> {
    let migrated = migrate(&prefs.cloned().unwrap_or_else(|| load_prefs(paths)));
    pending_of(&migrated)
}

fn pending_of(migrated: &Prefs) -> Vec<String> {
    let done = migrated.setup_done();
    STEPS
        .iter()
        .filter(|step| !done.iter().any(|d| d == *step))
        .map(|step| (*step).to_owned())
        .collect()
}

/// `firstrun.mark_done(steps)`: record steps (unknown ones ignored); returns what is pending.
pub fn mark_done(paths: &AppPaths, steps: &[&str]) -> io::Result<Vec<String>> {
    let updated = update_prefs(paths, |prefs| {
        let mut migrated = migrate(prefs);
        // every element is kept, whatever its type: only known new steps are appended
        let mut done = match migrated.get("setup_done") {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        for step in steps {
            let known = STEPS.contains(step);
            let present = done.iter().any(|d| d.as_str() == Some(step));
            if known && !present {
                done.push(Value::from(*step));
            }
        }
        migrated.insert("setup_done".to_owned(), Value::Array(done));
        *prefs = migrated;
    })?;
    Ok(pending_of(&migrate(&updated)))
}

/// `firstrun.should_open(prefs)`: steps left and not yet shown by this version
/// ([`clawd_contracts::APP_VERSION`]).
pub fn should_open(paths: &AppPaths, prefs: Option<&Prefs>) -> bool {
    let migrated = migrate(&prefs.cloned().unwrap_or_else(|| load_prefs(paths)));
    !pending_of(&migrated).is_empty() && migrated.setup_shown() != Some(APP_VERSION)
}

/// `firstrun.mark_shown()`: `setup_shown = APP_VERSION`.
pub fn mark_shown(paths: &AppPaths) -> io::Result<()> {
    update_prefs(paths, |prefs| {
        let mut migrated = migrate(prefs);
        migrated.insert("setup_shown".to_owned(), Value::from(APP_VERSION));
        *prefs = migrated;
    })
    .map(|_| ())
}

/// Python's `str(x)` for the scalars an `agents_seen` list can hold.
fn python_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(n) if n.is_f64() => n.as_f64().map_or_else(|| n.to_string(), py_float_repr),
        other => other.to_string(),
    }
}

/// `{str(x) for x in prefs.get("agents_seen") or []}`: iterates like Python (a string by
/// character, an object by key); a number or `true` (a `TypeError` there) reads as none.
fn agents_seen_of(prefs: &Prefs) -> BTreeSet<String> {
    match prefs.get("agents_seen") {
        Some(Value::Array(items)) => items.iter().map(python_str).collect(),
        Some(Value::String(s)) => s.chars().map(String::from).collect(),
        Some(Value::Object(map)) => map.keys().cloned().collect(),
        _ => BTreeSet::new(),
    }
}

/// `firstrun.seen()`: agents already offered.
pub fn seen(paths: &AppPaths) -> BTreeSet<String> {
    agents_seen_of(&load_prefs(paths))
}

/// `firstrun.mark_seen(ids)`: add to `agents_seen` (sorted, deduplicated; no write when nothing
/// new).
pub fn mark_seen(paths: &AppPaths, ids: &[&str]) -> io::Result<()> {
    update_prefs(paths, |prefs| {
        let mut migrated = migrate(prefs);
        let had = agents_seen_of(&migrated);
        let mut all = had.clone();
        all.extend(ids.iter().map(|id| (*id).to_owned()));
        if all != had {
            migrated.insert(
                "agents_seen".to_owned(),
                Value::Array(all.into_iter().map(Value::String).collect()),
            );
        }
        *prefs = migrated;
    })
    .map(|_| ())
}

/// `firstrun.new_agents(found, installed)`: agents on this machine never offered before and
/// not connected yet.
pub fn new_agents(paths: &AppPaths, found: &[String], installed: &[String]) -> Vec<String> {
    let mut skip = seen(paths);
    skip.extend(installed.iter().cloned());
    found
        .iter()
        .filter(|id| !skip.contains(*id))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_paths() -> (tempfile::TempDir, AppPaths) {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = AppPaths::new(dir.path().join("home"));
        (dir, paths)
    }

    fn prefs(v: Value) -> Prefs {
        serde_json::from_value(v).expect("prefs")
    }

    fn write(paths: &AppPaths, v: Value) {
        crate::prefs::save_prefs(paths, &prefs(v)).expect("save");
    }

    fn raw(paths: &AppPaths) -> String {
        std::fs::read_to_string(paths.prefs_file()).expect("read")
    }

    #[test]
    fn migrate_derives_setup_done_from_first_run_done() {
        let m = migrate(&prefs(json!({"first_run_done": true, "x": 1})));
        assert_eq!(m.setup_done(), vec!["clock"]);
        assert_eq!(m.get("x"), Some(&json!(1)));
        assert!(migrate(&prefs(json!({}))).setup_done().is_empty());
        assert!(migrate(&prefs(json!({"first_run_done": false}))).has_setup_done_list());
        // a wrong-typed setup_done is replaced; a list (even with junk) is kept
        let m = migrate(&prefs(json!({"setup_done": "clock", "first_run_done": 1})));
        assert_eq!(m.get("setup_done"), Some(&json!(["clock"])));
        let keep = prefs(json!({"setup_done": ["agents", 3]}));
        assert_eq!(migrate(&keep), keep);
    }

    #[test]
    fn load_persists_the_migration_once() {
        let (_dir, paths) = temp_paths();
        write(&paths, json!({"first_run_done": true, "keep": [1]}));
        let got = load(&paths).expect("load");
        assert_eq!(got.setup_done(), vec!["clock"]);
        assert_eq!(load_prefs(&paths).setup_done(), vec!["clock"]);
        assert_eq!(load_prefs(&paths).get("keep"), Some(&json!([1])));
        let before = raw(&paths);
        load(&paths).expect("load again");
        assert_eq!(raw(&paths), before);
    }

    #[test]
    fn load_of_a_missing_file_creates_it_with_an_empty_list() {
        let (_dir, paths) = temp_paths();
        assert!(load(&paths).expect("load").setup_done().is_empty());
        assert_eq!(raw(&paths), "{\n  \"setup_done\": []\n}");
    }

    #[test]
    fn pending_lists_steps_in_order() {
        let (_dir, paths) = temp_paths();
        assert_eq!(pending(&paths, None), vec!["clock", "agents", "usage"]);
        let p = prefs(json!({"setup_done": ["usage", "clock"]}));
        assert_eq!(pending(&paths, Some(&p)), vec!["agents"]);
        let upgrade = prefs(json!({"first_run_done": true}));
        assert_eq!(pending(&paths, Some(&upgrade)), vec!["agents", "usage"]);
    }

    #[test]
    fn mark_done_ignores_unknown_steps_and_keeps_junk() {
        let (_dir, paths) = temp_paths();
        write(
            &paths,
            json!({"setup_done": ["clock", 7], "other": {"a": 1}}),
        );
        let left = mark_done(&paths, &["usage", "bogus", "usage", "clock"]).expect("mark");
        assert_eq!(left, vec!["agents"]);
        let p = load_prefs(&paths);
        assert_eq!(p.get("setup_done"), Some(&json!(["clock", 7, "usage"])));
        assert_eq!(p.get("other"), Some(&json!({"a": 1})));
        let before = raw(&paths);
        mark_done(&paths, &["clock"]).expect("mark again");
        assert_eq!(raw(&paths), before);
    }

    #[test]
    fn mark_done_on_a_fresh_install_appends_the_key_last() {
        let (_dir, paths) = temp_paths();
        write(&paths, json!({"b": 1, "a": 2}));
        assert_eq!(
            mark_done(&paths, &["agents"]).expect("mark"),
            vec!["clock", "usage"]
        );
        let keys: Vec<_> = load_prefs(&paths).keys().cloned().collect();
        assert_eq!(keys, vec!["b", "a", "setup_done"]);
    }

    #[test]
    fn should_open_once_per_version_while_steps_remain() {
        let (_dir, paths) = temp_paths();
        assert!(should_open(&paths, None));
        mark_shown(&paths).expect("shown");
        assert_eq!(load_prefs(&paths).setup_shown(), Some(APP_VERSION));
        assert!(!should_open(&paths, None));
        write(&paths, json!({"setup_done": [], "setup_shown": "0.0.1"}));
        assert!(should_open(&paths, None));
        write(&paths, json!({"setup_done": ["clock", "agents", "usage"]}));
        assert!(!should_open(&paths, None));
        let wrong_type = prefs(json!({"setup_shown": 5}));
        assert!(should_open(&paths, Some(&wrong_type)));
    }

    #[test]
    fn seen_and_mark_seen_sort_and_dedupe() {
        let (_dir, paths) = temp_paths();
        assert!(seen(&paths).is_empty());
        mark_seen(&paths, &["pi", "codex", "pi"]).expect("mark");
        assert_eq!(
            load_prefs(&paths).get("agents_seen"),
            Some(&json!(["codex", "pi"]))
        );
        let before = raw(&paths);
        mark_seen(&paths, &["codex"]).expect("nothing new");
        assert_eq!(raw(&paths), before);
        mark_seen(&paths, &["amp"]).expect("mark");
        assert_eq!(
            seen(&paths).into_iter().collect::<Vec<_>>(),
            vec!["amp", "codex", "pi"]
        );
    }

    #[test]
    fn seen_tolerates_wrong_types() {
        let (_dir, paths) = temp_paths();
        write(&paths, json!({"agents_seen": "codex"}));
        assert_eq!(seen(&paths).len(), 5); // Python iterates the string's characters
        write(&paths, json!({"agents_seen": 5}));
        assert!(seen(&paths).is_empty());
        write(&paths, json!({"agents_seen": ["a", 3, null, true, 1.5]}));
        let got: Vec<_> = seen(&paths).into_iter().collect();
        assert_eq!(got, vec!["1.5", "3", "None", "True", "a"]);
    }

    #[test]
    fn new_agents_skips_seen_and_installed() {
        let (_dir, paths) = temp_paths();
        mark_seen(&paths, &["codex"]).expect("mark");
        let found: Vec<String> = ["codex", "pi", "amp", "cursor"].map(String::from).to_vec();
        let installed: Vec<String> = vec!["amp".to_owned()];
        assert_eq!(new_agents(&paths, &found, &installed), vec!["pi", "cursor"]);
    }
}
