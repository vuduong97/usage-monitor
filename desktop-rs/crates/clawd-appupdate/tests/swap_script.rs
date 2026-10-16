//! The swap script run for real (Python: `test_swap_script_*`), on a fake machine under a temp
//! dir: stub `ditto` (`cp -R`) and `open` (records). POSIX only: `sh` is not on Windows, and no
//! other platform ever runs the script.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use clawd_appupdate::codesign::mac_swap_script;

struct Env {
    root: tempfile::TempDir,
    app: PathBuf,
    stage: PathBuf,
    path: String,
}

/// A fake machine: stub `ditto` and `open`, an old app and a staged new one.
fn swap_env(ditto_ok: bool) -> Env {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let ditto = if ditto_ok {
        "#!/bin/sh\ncp -R \"$1\" \"$2\"\n"
    } else {
        "#!/bin/sh\nexit 1\n"
    };
    std::fs::write(bin.join("ditto"), ditto).unwrap();
    let open = format!(
        "#!/bin/sh\necho \"$@\" >> \"{}/opened\"\n",
        root.path().display()
    );
    std::fs::write(bin.join("open"), open).unwrap();
    for name in ["ditto", "open"] {
        std::fs::set_permissions(bin.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let app = root.path().join("Applications").join("Clawd.app");
    let stage = root.path().join("stage");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("version"), "old").unwrap();
    std::fs::create_dir_all(stage.join("Clawd.app")).unwrap();
    std::fs::write(stage.join("Clawd.app").join("version"), "new").unwrap();
    std::fs::write(
        stage.join("ready.json"),
        r#"{"version": "0.2.0", "sha256": "0000000000000000000000000000000000000000000000000000000000000000", "path": "Clawd.app"}"#,
    )
    .unwrap();
    std::fs::write(stage.join("swap.sh"), mac_swap_script()).unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Env {
        root,
        app,
        stage,
        path,
    }
}

fn command(env: &Env, pid: u32) -> Command {
    let mut c = Command::new("/bin/sh");
    c.arg(env.stage.join("swap.sh"))
        .arg(pid.to_string())
        .arg(env.stage.join("Clawd.app"))
        .arg(&env.app)
        .arg(&env.stage)
        .arg("0.2.0")
        .env("PATH", &env.path)
        .stdin(Stdio::null());
    c
}

/// The pid of a process that has already exited.
fn dead_pid() -> u32 {
    let mut child = Command::new("true").spawn().unwrap();
    child.wait().unwrap();
    child.id()
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn test_swap_script_replaces_the_app_and_opens_it() {
    let env = swap_env(true);
    let out = command(&env, dead_pid()).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(env.app.join("version")).unwrap(),
        "new"
    );
    assert_eq!(
        names(env.app.parent().unwrap()),
        vec!["Clawd.app"],
        "no .old / .update left behind"
    );
    let opened = std::fs::read_to_string(env.root.path().join("opened")).unwrap();
    assert_eq!(opened.trim(), env.app.to_string_lossy());
    assert!(!env.stage.join("ready.json").exists() && !env.stage.join("Clawd.app").exists());
    assert!(std::fs::read_to_string(env.stage.join("apply.log"))
        .unwrap()
        .contains("updated to 0.2.0"));
}

#[test]
fn test_swap_script_waits_for_the_app_to_quit() {
    let env = swap_env(true);
    let mut sleeper = Command::new("sleep").arg("1.5").spawn().unwrap();
    let started = Instant::now();
    let mut script = command(&env, sleeper.id()).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        std::fs::read_to_string(env.app.join("version")).unwrap(),
        "old",
        "the app has not quit yet"
    );
    sleeper.wait().unwrap(); // reaped, like launchd does for the real app
    assert_eq!(script.wait().unwrap().code(), Some(0));
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert_eq!(
        std::fs::read_to_string(env.app.join("version")).unwrap(),
        "new"
    );
}

#[test]
fn test_swap_script_leaves_the_old_app_when_the_copy_fails() {
    let env = swap_env(false);
    let out = command(&env, dead_pid()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        std::fs::read_to_string(env.app.join("version")).unwrap(),
        "old"
    );
    assert_eq!(names(env.app.parent().unwrap()), vec!["Clawd.app"]);
    assert_eq!(
        std::fs::read_to_string(env.stage.join("failed"))
            .unwrap()
            .trim(),
        "0.2.0"
    );
    assert!(
        env.stage.join("ready.json").exists(),
        "the staged update stays for the next start"
    );
    let opened = std::fs::read_to_string(env.root.path().join("opened")).unwrap();
    assert_eq!(
        opened.trim(),
        env.app.to_string_lossy(),
        "the old app starts again"
    );
}
