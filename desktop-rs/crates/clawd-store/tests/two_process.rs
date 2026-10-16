//! `desktop.json` is written by several processes: no update may be lost.
//!
//! `two_rust_processes_lose_no_update` re-runs this test binary as three worker processes.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use clawd_contracts::AppPaths;
use clawd_store::prefs::{load_prefs, update_prefs};
use serde_json::json;

const WORKER_HOME: &str = "CLAWD_STORE_TEST_WORKER_HOME";
const WORKER_ID: &str = "CLAWD_STORE_TEST_WORKER_ID";
const UPDATES: usize = 12;

fn temp_paths() -> (tempfile::TempDir, AppPaths) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = AppPaths::new(dir.path().join("home"));
    (dir, paths)
}

/// Runs only when the parent test started this binary as a worker; otherwise a no-op.
#[test]
fn worker_entry() {
    let (Ok(home), Ok(id)) = (std::env::var(WORKER_HOME), std::env::var(WORKER_ID)) else {
        return;
    };
    let paths = AppPaths::new(PathBuf::from(home));
    for i in 0..UPDATES {
        update_prefs(&paths, |prefs| {
            prefs.insert(format!("w{id}_{i}"), json!(i));
        })
        .expect("update");
    }
}

#[test]
fn two_rust_processes_lose_no_update() {
    let (_dir, paths) = temp_paths();
    let exe = std::env::current_exe().expect("current exe");
    let workers = 3;
    let children: Vec<_> = (0..workers)
        .map(|id| {
            Command::new(&exe)
                .args(["--exact", "worker_entry", "--test-threads=1"])
                .env(WORKER_HOME, paths.home())
                .env(WORKER_ID, id.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn worker")
        })
        .collect();
    for mut child in children {
        assert!(child.wait().expect("wait").success(), "worker failed");
    }
    let prefs = load_prefs(&paths);
    assert_eq!(prefs.len(), workers * UPDATES, "an update was lost");
    for id in 0..workers {
        for i in 0..UPDATES {
            assert_eq!(prefs.get(&format!("w{id}_{i}")), Some(&json!(i)));
        }
    }
    let leftovers = std::fs::read_dir(paths.config_dir())
        .expect("config dir")
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(leftovers, 0);
}
