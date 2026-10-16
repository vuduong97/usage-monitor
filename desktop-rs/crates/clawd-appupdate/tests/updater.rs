//! `appupdate.Updater` (Python: tests/test_appupdate.py "the updater: Windows / macOS") with a
//! fake GitHub, fake `codesign`/`ditto` and a launcher that records instead of starting.

mod common;

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use clawd_appupdate::updater::LOCK_STALE;
use clawd_appupdate::{
    can_apply, running_bundle, sha256_file, LaunchKind, Source, Updater, UpdaterState,
};
use clawd_contracts::{SUMS_NAME, SUMS_SIG_NAME};
use common::*;

fn windows_args() -> Vec<String> {
    ["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]
        .map(String::from)
        .to_vec()
}

// ---- can this install replace itself --------------------------------------------------------
#[test]
fn test_can_apply_by_platform() {
    assert!(!can_apply(Some(false), Some("mac"), None).0); // from source
    assert_eq!(
        can_apply(Some(true), Some("windows"), None),
        (true, String::new())
    );
    assert!(!can_apply(Some(true), Some("linux"), None).0);
    assert_eq!(
        can_apply(Some(false), Some("windows"), None).1,
        "Bản chạy từ mã nguồn không tự cập nhật."
    );
}

#[test]
fn test_can_apply_on_mac_paths() {
    let dir = tempfile::tempdir().unwrap();
    let apps = dir.path().join("Applications");
    std::fs::create_dir_all(&apps).unwrap();
    let exe = apps
        .join("Clawd.app")
        .join("Contents")
        .join("MacOS")
        .join("Clawd");
    assert_eq!(
        can_apply(Some(true), Some("mac"), Some(&exe)),
        (true, String::new())
    );
    assert_eq!(running_bundle(Some(&exe)), Some(apps.join("Clawd.app")));

    let volume = Path::new("/Volumes/Clawd/Clawd.app/Contents/MacOS/Clawd");
    assert!(can_apply(Some(true), Some("mac"), Some(volume))
        .1
        .contains("Applications"));
    let translocated =
        Path::new("/private/var/folders/x/AppTranslocation/ABC/d/Clawd.app/Contents/MacOS/Clawd");
    assert!(!can_apply(Some(true), Some("mac"), Some(translocated)).0);
    assert!(
        can_apply(Some(true), Some("mac"), Some(Path::new("/usr/bin/python3")))
            .1
            .contains("Clawd.app")
    );
    assert_eq!(running_bundle(Some(Path::new("/usr/bin/python3"))), None);
}

#[cfg(unix)]
#[test]
fn test_can_apply_needs_a_writable_applications_folder() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let apps = dir.path().join("Applications");
    std::fs::create_dir_all(&apps).unwrap();
    let exe = apps.join("Clawd.app/Contents/MacOS/Clawd");
    std::fs::set_permissions(&apps, std::fs::Permissions::from_mode(0o500)).unwrap();
    // root can write anywhere: only assert when the folder really is read-only for us
    let readonly = std::fs::File::create(apps.join("probe")).is_err();
    let (ok, why) = can_apply(Some(true), Some("mac"), Some(&exe));
    std::fs::set_permissions(&apps, std::fs::Permissions::from_mode(0o700)).unwrap();
    if readonly {
        assert!(!ok);
        assert!(why.starts_with("Không có quyền ghi vào"), "{why}");
    }
}

// ---- the updater: Windows -----------------------------------------------------------------
#[tokio::test]
async fn test_check_downloads_verifies_and_stages() {
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".repeat(100))]);
    let rig = Rig::new();
    let up = rig.windows_updater(&gh, "0.1.0");
    assert_eq!(up.state().status, "idle");
    let st = up.check(false).await;
    assert_eq!(st.status, "ready", "{st:?}");
    assert_eq!(st.version, "0.2.0");
    assert!(st.can_apply);
    assert_eq!(st.current, "0.1.0");
    assert!(st.notes.contains("xoay"));
    assert!(st.page.ends_with("v0.2.0"));
    assert_eq!(st.progress, 1.0);
    assert_eq!(st.checked, 1000.0);
    let s = up.staged().expect("staged");
    assert_eq!(s.path.file_name().unwrap(), EXE_NAME);
    assert_eq!(sha256_file(&s.path).unwrap(), s.sha256);
    // a second process (the tray, the settings window) sees the same without asking GitHub
    let other = rig.bystander(&gh, "0.1.0", "windows");
    let seen = other.state();
    assert_eq!(
        (seen.status.as_str(), seen.version.as_str()),
        ("ready", "0.2.0")
    );
    // the stage holds ready.json, the installer and the signed sums beside it
    let listing = names(&rig.stage());
    assert!(listing.contains(&"ready.json".to_owned()), "{listing:?}");
    assert!(listing.contains(&SUMS_NAME.to_owned()) && listing.contains(&SUMS_SIG_NAME.to_owned()));
}

#[tokio::test]
async fn test_a_failed_download_keeps_the_app_as_it_is() {
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".repeat(100))]);
    gh.tamper(EXE_NAME);
    let rig = Rig::new();
    let up = rig.windows_updater(&gh, "0.1.0");
    let st = up.check(false).await;
    assert_eq!(st.status, "error");
    assert!(st.error.contains("SHA-256"), "{}", st.error);
    assert!(st.error.starts_with("Không tải được bản 0.2.0: "));
    assert!(up.staged().is_none());
    assert!(names(&rig.stage()).iter().all(|n| !n.contains(".exe")));
}

#[tokio::test]
async fn test_current_when_there_is_nothing_newer() {
    let gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    let up = Rig::new().windows_updater(&gh, "0.2.0");
    assert_eq!(up.check(false).await.status, "current");
}

#[tokio::test]
async fn test_the_check_is_rate_limited_but_force_asks_again() {
    let gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    let rig = Rig::new();
    let up = rig.windows_updater(&gh, "0.2.0");
    up.check(false).await;
    let n = gh.gh.calls().len();
    up.check(false).await; // the window opening: too soon
    assert_eq!(gh.gh.calls().len(), n);
    up.check(true).await; // the button
    assert!(gh.gh.calls().len() > n);
    let n = gh.gh.calls().len();
    rig.clock.advance(clawd_appupdate::source::RECHECK + 1.0);
    up.check(false).await;
    assert!(gh.gh.calls().len() > n);
}

#[tokio::test]
async fn test_an_install_that_cannot_replace_itself_only_announces() {
    let gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    let rig = Rig::new();
    let mut opts = rig.options("0.1.0", "windows", "x86_64", &gh.key);
    opts.packaged = Some(false);
    let up = Updater::new(Some(gh.source()), opts);
    let st = up.check(false).await;
    assert_eq!(
        (st.status.as_str(), st.version.as_str()),
        ("available", "0.2.0")
    );
    assert!(!st.can_apply);
    assert!(st.why.contains("mã nguồn"));
    assert!(!rig.stage().exists());
    assert!(
        !gh.gh.calls().iter().any(|c| c.ends_with(EXE_NAME)),
        "nothing downloaded"
    );
    assert_eq!(up.apply(None), (false, st.why));
}

#[tokio::test]
async fn test_github_unreachable_is_an_error_not_a_crash() {
    let gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    gh.gh.go_offline("no network");
    let up = Rig::new().windows_updater(&gh, "0.1.0");
    let st = up.check(false).await;
    assert_eq!(st.status, "error");
    assert!(
        st.error.starts_with("Không hỏi được GitHub: ConnectError"),
        "{}",
        st.error
    );
}

#[tokio::test]
async fn test_a_staged_update_that_is_now_current_is_cleaned_up() {
    let gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    let rig = Rig::new();
    let up = rig.windows_updater(&gh, "0.1.0");
    up.check(false).await;
    assert!(up.staged().is_some());
    let after = rig.bystander(&gh, "0.2.0", "windows"); // the new version runs
    assert!(after.staged().is_none());
    assert!(!rig.stage().exists());
}

#[tokio::test]
async fn test_apply_on_windows_runs_the_installer_silently() {
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".repeat(100))]);
    let rig = Rig::new();
    let up = rig.windows_updater(&gh, "0.1.0");
    up.check(false).await;
    assert_eq!(up.apply(Some(1234)), (true, String::new()));
    let started = rig.launcher.started();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].program.file_name().unwrap(), EXE_NAME);
    assert_eq!(started[0].args, windows_args());
    assert_eq!(started[0].kind, LaunchKind::Installer);
}

#[tokio::test]
async fn test_apply_refuses_while_firmware_is_installing_and_a_changed_file() {
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".repeat(100))]);
    let rig = Rig::new();
    let up = rig.windows_updater(&gh, "0.1.0");
    up.check(false).await;
    set_ota_busy(rig.paths());
    let (ok, why) = up.apply(None);
    assert!(!ok && why.contains("firmware"), "{why}");
    assert!(rig.launcher.started().is_empty());
    std::fs::remove_file(rig.paths().ota_busy_file()).unwrap();
    std::fs::write(up.staged().unwrap().path, b"swapped after the download").unwrap();
    let (ok, why) = up.apply(None);
    assert!(!ok && why.contains("bị đổi"), "{why}");
    assert!(rig.launcher.started().is_empty());
    assert!(up.staged().is_none());
    let empty = Rig::new().bystander(&gh, "0.1.0", "windows");
    assert_eq!(
        empty.apply(None),
        (false, "Chưa có bản nào được tải xong.".to_owned())
    );
}

#[tokio::test]
async fn a_launcher_that_cannot_start_the_installer_reports_it() {
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    let rig = Rig::new();
    let up = rig.windows_updater(&gh, "0.1.0");
    up.check(false).await;
    rig.launcher.fail_with(io::ErrorKind::PermissionDenied);
    let (ok, why) = up.apply(None);
    assert!(
        !ok && why.starts_with("Không bắt đầu được cập nhật: "),
        "{why}"
    );
}

fn write_foreign_lock(rig: &Rig) -> PathBuf {
    let lock = rig.stage().with_file_name("app-update.lock");
    std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
    std::fs::write(&lock, "1").unwrap(); // not our pid
    lock
}

#[tokio::test]
async fn test_the_apply_lock_keeps_two_processes_from_downloading() {
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    let rig = Rig::new();
    let b = rig.windows_updater(&gh, "0.1.0");
    // b sees a live lock held by another pid: it does not call GitHub and reports a download
    let lock = write_foreign_lock(&rig);
    let st = b.check(false).await;
    assert_eq!(st.status, "downloading");
    assert!(gh.gh.calls().is_empty());
    // a dead downloader's lock expires
    rig.clock.advance(LOCK_STALE + 1.0);
    let old = 1000.0 + LOCK_STALE + 1.0 - LOCK_STALE - 5.0;
    let file = std::fs::OpenOptions::new().write(true).open(&lock).unwrap();
    file.set_modified(UNIX_EPOCH + Duration::from_secs_f64(old))
        .unwrap();
    drop(file);
    assert_eq!(b.check(false).await.status, "ready");
    assert!(!lock.exists(), "the lock is released after the check");
}

#[tokio::test]
async fn test_a_failed_swap_is_not_offered_again() {
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    let rig = Rig::new();
    let up = rig.windows_updater(&gh, "0.1.0");
    up.check(false).await;
    std::fs::write(rig.stage().join("failed"), "0.2.0").unwrap();
    let st = up.state();
    assert_eq!(st.status, "error");
    assert!(st.error.contains("trang phát hành"));
    assert!(!st.can_apply);
    gh.gh.clear_calls();
    let again = rig.windows_updater(&gh, "0.1.0").check(true).await;
    assert_eq!(again.status, "error");
    assert!(
        !gh.gh.calls().iter().any(|c| c.ends_with(EXE_NAME)),
        "{:?}",
        gh.gh.calls()
    );
}

#[tokio::test]
async fn a_second_check_while_one_runs_returns_at_once() {
    let gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    let rig = Rig::new();
    let up = std::sync::Arc::new(rig.windows_updater(&gh, "0.1.0"));
    let (a, b) = tokio::join!(up.check(true), up.check(true));
    // one of them did the work, the other only read the state; GitHub's API was asked once
    let asks = gh
        .gh
        .calls()
        .iter()
        .filter(|c| c.as_str() == API_LATEST)
        .count();
    assert!(asks >= 1);
    assert!(a.status == "ready" || b.status == "ready");
}

// ---- trust: the hash lives in memory, a restart re-verifies the signature -----------------------
async fn staged_windows() -> (Rig, GitHub, Updater) {
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".repeat(100))]);
    let rig = Rig::new();
    let up = rig.windows_updater(&gh, "0.1.0");
    assert_eq!(up.check(false).await.status, "ready");
    (rig, gh, up)
}

#[tokio::test]
async fn the_hash_in_memory_wins_over_a_rewritten_ready_json() {
    let (rig, _gh, up) = staged_windows().await;
    let ready = rig.stage().join("ready.json");
    let real = up.staged().unwrap().sha256;
    let text = std::fs::read_to_string(&ready)
        .unwrap()
        .replace(&real, &"0".repeat(64));
    std::fs::write(&ready, text).unwrap();
    assert_eq!(up.staged().unwrap().sha256, real);
    assert_eq!(up.apply(Some(1)), (true, String::new()));
}

#[tokio::test]
async fn a_restart_trusts_ready_json_only_when_the_signature_verifies_again() {
    let (rig, gh, up) = staged_windows().await;
    let real = up.staged().unwrap().sha256;
    // untouched: the fresh process verifies SHA256SUMS + .minisig and accepts it
    let fresh = rig.bystander(&gh, "0.1.0", "windows");
    assert_eq!(fresh.staged().unwrap().sha256, real);

    // ready.json says another hash: not what the signed sums list
    let ready = rig.stage().join("ready.json");
    let original = std::fs::read_to_string(&ready).unwrap();
    std::fs::write(&ready, original.replace(&real, &"0".repeat(64))).unwrap();
    assert!(rig.bystander(&gh, "0.1.0", "windows").staged().is_none());
    assert!(!rig.stage().exists(), "an untrusted stage is cleaned");
}

#[tokio::test]
async fn a_restart_refuses_missing_wrong_or_swapped_signatures() {
    for case in ["no-sig", "no-sums", "other-key", "sums-edited"] {
        let (rig, gh, _up) = staged_windows().await;
        let stage = rig.stage();
        let mut key = None;
        match case {
            "no-sig" => std::fs::remove_file(stage.join(SUMS_SIG_NAME)).unwrap(),
            "no-sums" => std::fs::remove_file(stage.join(SUMS_NAME)).unwrap(),
            "other-key" => key = Some(TestKey::new()),
            _ => {
                let mut sums = std::fs::read(stage.join(SUMS_NAME)).unwrap();
                sums.extend_from_slice(b"# edited\n");
                std::fs::write(stage.join(SUMS_NAME), sums).unwrap();
            }
        }
        let mut opts = rig.options(
            "0.1.0",
            "windows",
            "x86_64",
            key.as_ref().unwrap_or(&gh.key),
        );
        opts.public_key = key
            .as_ref()
            .map_or_else(|| gh.key.public(), TestKey::public);
        let fresh = Updater::new(None, opts);
        assert!(fresh.staged().is_none(), "{case}");
    }
}

#[tokio::test]
async fn leftovers_of_the_python_app_are_cleaned_up() {
    // 0.1.4 stages ready.json + the installer, with no signed sums
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    let rig = Rig::new();
    let stage = rig.stage();
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::write(stage.join(EXE_NAME), b"installer").unwrap();
    std::fs::write(
        stage.join("ready.json"),
        format!(
            r#"{{"version": "0.2.0", "sha256": "{}", "path": "{EXE_NAME}", "notes": "", "page": ""}}"#,
            sha(b"installer")
        ),
    )
    .unwrap();
    let up = rig.bystander(&gh, "0.1.0", "windows");
    assert!(up.staged().is_none());
    assert!(!stage.exists());
    assert_eq!(
        up.apply(None),
        (false, "Chưa có bản nào được tải xong.".to_owned())
    );
}

#[tokio::test]
async fn clean_removes_the_stage_and_forgets_the_hash() {
    let (rig, _gh, up) = staged_windows().await;
    up.clean();
    assert!(!rig.stage().exists());
    assert!(up.staged().is_none());
}

// ---- the updater: macOS (codesign and ditto replaced) -----------------------------------------

#[tokio::test]
async fn test_mac_download_unpack_and_checks() {
    let m = mac_setup(Mac::default());
    let st = m.up.check(false).await;
    assert_eq!(st.status, "ready", "{st:?}");
    let s = m.up.staged().unwrap();
    let stage = m.rig.cfg.root().join("stage");
    assert_eq!(s.path, stage.join("Clawd.app"));
    assert!(s.path.join("Contents").join("Info.plist").is_file());
    assert!(!stage.join(ZIP_NAME).exists() && !stage.join("unpack").exists());
    let calls = m.rig.runner.calls();
    assert!(calls
        .iter()
        .any(|(p, a)| p == "codesign" && a[0] == "--verify"));
    assert!(calls
        .iter()
        .any(|(p, a)| p == "ditto" && a[..2] == ["-x", "-k"]));
}

#[tokio::test]
async fn test_mac_refuses_a_bad_bundle() {
    let cases: Vec<(Mac, &str)> = vec![
        (
            Mac {
                new_team: Some("OTHERTEAM01"),
                ..Mac::default()
            },
            "Team ID",
        ),
        (
            Mac {
                new_team: None,
                ..Mac::default()
            },
            "Team ID",
        ), // ad-hoc build replacing a Developer ID one
        (
            Mac {
                verify_rc: 1,
                ..Mac::default()
            },
            "chữ ký",
        ),
        (
            Mac {
                version: "0.1.9",
                ..Mac::default()
            },
            "0.1.9",
        ), // the zip holds another version than announced
        (
            Mac {
                bundle_id: "com.evil.app",
                ..Mac::default()
            },
            "không phải app Clawd",
        ),
    ];
    for (mac, why) in cases {
        let m = mac_setup(mac);
        let st = m.up.check(false).await;
        assert_eq!(st.status, "error", "{why}");
        assert!(st.error.contains(why), "{why}: {}", st.error);
        assert!(m.up.staged().is_none());
    }
}

#[tokio::test]
async fn test_mac_an_ad_hoc_running_app_does_not_self_update() {
    // No Team ID to match the new signature against: it only announces, and downloads nothing.
    let m = mac_setup(Mac {
        running_team: None,
        ..Mac::default()
    });
    let st = m.up.check(false).await;
    assert_eq!(st.status, "available");
    assert!(!st.can_apply);
    assert!(st.why.contains("ad-hoc"));
    assert!(!m.gh.gh.calls().iter().any(|c| c.ends_with(ZIP_NAME)));
    assert!(!m.up.apply(None).0);
}

#[tokio::test]
async fn test_mac_apply_starts_the_swap_script() {
    let m = mac_setup(Mac::default());
    m.up.check(false).await;
    std::fs::write(m.rig.cfg.root().join("stage").join("failed"), "0.1.9").unwrap();
    assert_eq!(m.up.apply(Some(4321)), (true, String::new()));
    let started = m.rig.launcher.started();
    assert_eq!(started.len(), 1);
    let stage = m.rig.cfg.root().join("stage");
    let spec = &started[0];
    assert_eq!(spec.program, PathBuf::from("/bin/sh"));
    assert_eq!(spec.kind, LaunchKind::SwapScript);
    let expect = [
        stage.join("swap.sh").to_string_lossy().into_owned(),
        "4321".to_owned(),
        stage.join("Clawd.app").to_string_lossy().into_owned(),
        m.running.to_string_lossy().into_owned(),
        stage.to_string_lossy().into_owned(),
        "0.2.0".to_owned(),
    ];
    assert_eq!(spec.args, expect);
    assert_eq!(
        std::fs::read_to_string(stage.join("swap.sh")).unwrap(),
        clawd_appupdate::codesign::mac_swap_script()
    );
    assert!(
        !stage.join("failed").exists(),
        "the stale marker goes before a new swap"
    );
}

#[tokio::test]
async fn test_mac_apply_checks_the_bundle_again() {
    let m = mac_setup(Mac::default());
    m.up.check(false).await;
    // the staged app was swapped for another version while it sat on disk
    let plist = m.rig.cfg.root().join("stage/Clawd.app/Contents/Info.plist");
    std::fs::write(&plist, info_plist("9.9.9", "vn.clawd.desktop")).unwrap();
    let (ok, why) = m.up.apply(Some(1));
    assert!(
        !ok && why.starts_with("Không bắt đầu được cập nhật: "),
        "{why}"
    );
    assert!(m.rig.launcher.started().is_empty());
}

#[test]
fn a_source_less_updater_never_asks_github() {
    let rig = Rig::new();
    let gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    let up = rig.bystander(&gh, "0.1.0", "windows");
    let st: UpdaterState = up.state();
    assert_eq!(st.status, "idle");
    let _ = Source::new("o/r", "https://api.github.com").unwrap();
    assert!(gh.gh.calls().is_empty());
}

// ---- ready.json may only name the file this updater staged ---------------------------------------
fn rewrite_ready_path(stage: &std::path::Path, old: &str, new: &str) {
    let ready = stage.join("ready.json");
    let from = format!("\"path\":{}", serde_json::to_string(old).unwrap());
    let to = format!("\"path\":{}", serde_json::to_string(new).unwrap());
    let text = std::fs::read_to_string(&ready).unwrap();
    assert!(text.contains(&from), "{text}");
    std::fs::write(&ready, text.replace(&from, &to)).unwrap();
}

#[tokio::test]
async fn windows_ready_json_must_name_this_versions_installer() {
    let outside = tempfile::tempdir().unwrap();
    let absolute = outside.path().join("evil.exe");
    std::fs::write(&absolute, b"installer").unwrap();
    let absolute = absolute.to_string_lossy().into_owned();
    for evil in [
        "../evil.exe".to_owned(),
        r"..\evil.exe".to_owned(),
        absolute,
        "sub/Clawd-0.2.0-windows-setup.exe".to_owned(),
        "Clawd-0.2.0-mac-arm64.zip".to_owned(),
    ] {
        let (rig, gh, up) = staged_windows().await;
        std::fs::write(rig.stage().parent().unwrap().join("evil.exe"), b"installer").unwrap();
        std::fs::create_dir_all(rig.stage().join("sub")).unwrap();
        std::fs::write(rig.stage().join("sub").join(EXE_NAME), b"installer").unwrap();
        rewrite_ready_path(&rig.stage(), EXE_NAME, &evil);
        // the process that downloaded it (hash in memory) and a fresh one both refuse
        assert!(up.staged().is_none(), "same process: {evil}");
        assert!(
            rig.bystander(&gh, "0.1.0", "windows").staged().is_none(),
            "{evil}"
        );
        assert!(!up.apply(None).0, "{evil}");
        assert!(rig.launcher.started().is_empty(), "{evil}");
    }
}

#[tokio::test]
async fn mac_ready_json_must_name_clawd_app() {
    for evil in [
        "../Clawd.app",
        "Other.app",
        "Clawd.app/../x",
        "/Applications/Clawd.app",
    ] {
        let m = mac_setup(Mac::default());
        assert_eq!(m.up.check(false).await.status, "ready");
        let stage = m.rig.cfg.root().join("stage");
        rewrite_ready_path(&stage, "Clawd.app", evil);
        assert!(m.up.staged().is_none(), "{evil}");
        assert!(m.rig.launcher.started().is_empty(), "{evil}");
    }
}
