//! `desktop.json` is written by several processes: no update may be lost.
//!
//! * `two_rust_processes_lose_no_update` re-runs this test binary as three worker processes.
//! * `python_and_rust_share_the_prefs_lock` (ignored, run by hand) interlocks with the real Python
//!   `paths.update_prefs` through `tools/golden/store_lock_interop.py`:
//!   `cargo test -p clawd-store --test two_process -- --ignored`
//!   (needs `CLAWD_TEST_PYTHON` or `python` on PATH; the script never touches the real home).

use std::path::{Path, PathBuf};
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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

#[test]
#[ignore = "runs the real Python app's paths.update_prefs; start by hand"]
fn python_and_rust_share_the_prefs_lock() {
    let (_dir, paths) = temp_paths();
    std::fs::create_dir_all(paths.config_dir()).expect("mkdir");
    let python = std::env::var("CLAWD_TEST_PYTHON").unwrap_or_else(|_| "python".to_owned());
    let script = repo_root().join("tools/golden/store_lock_interop.py");
    let count = 40;
    let py = Command::new(python)
        .arg(&script)
        .arg(paths.home())
        .arg(count.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn python");
    for i in 0..count {
        update_prefs(&paths, |prefs| {
            prefs.insert(format!("rs_{i}"), json!(i));
        })
        .expect("rust update");
    }
    let out = py.wait_with_output().expect("python");
    assert!(out.status.success(), "python failed");
    assert!(String::from_utf8_lossy(&out.stdout).contains("done"));
    let prefs = load_prefs(&paths);
    assert_eq!(prefs.len(), 2 * count, "an update was lost: {prefs:?}");
}
