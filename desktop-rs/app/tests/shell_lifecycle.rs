//! The `ShellCmd` handler (`tray.py:476-505`) and the settings-window session rules
//! (`tray.py:445-466`), driven through fakes: no engine, no window.
mod common;

use std::sync::Mutex;

use clawd_app::lifecycle::{apply_shell_cmd, ShellBackend};
use clawd_app::state::ShellCmd;
use clawd_app::windows::{clock_key, known_tab, new_session, BACKGROUND, TABS};
use clawd_contracts::{ClockSource, ClockTarget};
use common::*;

#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<String>>,
}

impl Fake {
    fn log(&self, s: impl Into<String>) {
        self.calls.lock().unwrap().push(s.into());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl ShellBackend for Fake {
    fn set_manual(&self, raw: &str) -> bool {
        self.log(format!("set_manual {raw}"));
        true
    }
    fn retry_clock(&self) {
        self.log("retry");
    }
    fn use_clock(&self, c: ClockTarget) {
        self.log(format!(
            "use {} {} {} {} {}",
            c.name,
            c.host,
            c.port,
            c.version,
            c.source.as_str()
        ));
    }
    fn poll_now(&self) {
        self.log("poll_now");
    }
    fn refresh(&self) {
        self.log("refresh");
    }
    fn restart_for_update(&self) {
        self.log("restart_for_update");
    }
    fn rescan_setup(&self) {
        self.log("rescan");
    }
}

fn run(cmd: ShellCmd) -> Vec<String> {
    let fake = Fake::default();
    apply_shell_cmd(&fake, cmd);
    fake.calls()
}

#[test]
fn a_typed_clock_pins_the_finder_and_refreshes() {
    assert_eq!(
        run(ShellCmd::SetClock {
            host: "10.0.0.5".into()
        }),
        ["set_manual 10.0.0.5", "refresh"]
    );
}

#[test]
fn an_empty_host_is_search_again() {
    assert_eq!(
        run(ShellCmd::SetClock {
            host: String::new()
        }),
        ["retry", "refresh"]
    );
}

#[test]
fn a_wizard_pick_uses_that_clock_as_found_by_mdns() {
    let cmd = ShellCmd::UseClock {
        name: "clawd".into(),
        host: "10.0.0.7".into(),
        port: 8080,
        version: "1.4".into(),
    };
    assert_eq!(run(cmd), ["use clawd 10.0.0.7 8080 1.4 mdns", "refresh"]);
    let no_port = ShellCmd::UseClock {
        name: String::new(),
        host: "10.0.0.7".into(),
        port: 0,
        version: String::new(),
    };
    assert_eq!(
        run(no_port)[0],
        "use  10.0.0.7 80  mdns",
        "port 0 means 80 (`msg.get('port') or 80`)"
    );
}

#[test]
fn the_other_commands_do_their_one_thing() {
    assert_eq!(run(ShellCmd::Refresh), ["refresh"]);
    assert_eq!(run(ShellCmd::UsageSource), ["poll_now"]);
    assert_eq!(run(ShellCmd::AppUpdateApply), ["restart_for_update"]);
    assert_eq!(run(ShellCmd::Setup), ["rescan"]);
}

#[test]
fn the_password_key_prefers_the_name_then_the_typed_address() {
    let mut c = ClockTarget::new("10.0.0.7", ClockSource::Saved);
    assert_eq!(clock_key(&c, ""), "10.0.0.7");
    assert_eq!(clock_key(&c, "10.0.0.9"), "10.0.0.9");
    c.name = "clawd".into();
    assert_eq!(clock_key(&c, "10.0.0.9"), "clawd");
}

#[test]
fn each_opening_gets_a_fresh_session_of_the_remembered_clock() {
    let clock = clock_a();
    let s = new_session(Some(&clock), "", "setup");
    assert_eq!(
        (s.base.as_str(), s.key.as_str(), s.tab.as_str()),
        ("http://10.0.0.7", "clawd", "setup")
    );
    let none = new_session(None, "", "");
    assert_eq!(
        (none.base.as_str(), none.key.as_str(), none.tab.as_str()),
        ("", "", "")
    );
}

#[test]
fn only_the_three_tabs_the_page_knows_are_passed_on() {
    for tab in TABS {
        assert_eq!(known_tab(tab), tab);
    }
    assert_eq!(known_tab("firmware"), "");
    assert_eq!(known_tab(""), "");
    assert_eq!(new_session(None, "", "bogus").tab, "");
}

#[test]
fn the_window_shows_the_dark_background_before_the_page_paints() {
    assert_eq!(
        (BACKGROUND.0, BACKGROUND.1, BACKGROUND.2, BACKGROUND.3),
        (0x14, 0x14, 0x13, 0xff)
    );
}

#[test]
#[ignore = "needs 08 (ClockStore): settings-only mode writes clock.json (settings_app.Channel.send)"]
fn without_a_tray_set_clock_and_use_clock_edit_clock_json() {
    unimplemented!("B'': apply_standalone_cmd over a temp ClockStore");
}

#[test]
#[ignore = "needs 03/05: first_run (test_first_run_installs_no_hooks) and Clawd.exe cleanup with OsStoreNames::for_test"]
fn first_run_enables_autostart_once_and_installs_no_hooks() {
    unimplemented!("B'': lifecycle::first_run over a temp home");
}

// ---- opening the settings window (review P1-1) ------------------------------------------

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use clawd_app::windows::{clear_session_after_failed_build, open_once, webview_args};

/// A fake window manager: `window` is what `get_webview_window` would say, `create` takes a
/// moment (WebView2 creation) before the window exists.
#[derive(Default)]
struct FakeWindows {
    window: Mutex<Option<u8>>,
    builds: AtomicUsize,
    forwards: AtomicUsize,
}

impl FakeWindows {
    async fn open(&self, gate: &tokio::sync::Mutex<()>, succeed: bool) -> bool {
        open_once(
            gate,
            || *self.window.lock().unwrap(),
            |_| {
                self.forwards.fetch_add(1, Ordering::SeqCst);
            },
            || async {
                self.builds.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                if succeed {
                    *self.window.lock().unwrap() = Some(1);
                }
                succeed
            },
        )
        .await
    }
}

#[tokio::test]
async fn two_clicks_in_a_row_build_one_window_and_forward_the_other() {
    let (windows, gate) = (FakeWindows::default(), tokio::sync::Mutex::new(()));
    let (a, b) = tokio::join!(windows.open(&gate, true), windows.open(&gate, true));
    assert!(a && b);
    assert_eq!(windows.builds.load(Ordering::SeqCst), 1);
    assert_eq!(windows.forwards.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn without_the_gate_both_clicks_would_build() {
    // The contrast that shows what the gate is for: two gates = no mutual exclusion.
    let windows = FakeWindows::default();
    let (g1, g2) = (tokio::sync::Mutex::new(()), tokio::sync::Mutex::new(()));
    tokio::join!(windows.open(&g1, true), windows.open(&g2, true));
    assert_eq!(windows.builds.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_failed_build_reports_false_and_the_next_click_tries_again() {
    let (windows, gate) = (FakeWindows::default(), tokio::sync::Mutex::new(()));
    assert!(!windows.open(&gate, false).await);
    assert!(windows.open(&gate, true).await);
    assert_eq!(windows.builds.load(Ordering::SeqCst), 2);
    assert_eq!(windows.forwards.load(Ordering::SeqCst), 0);
}

#[test]
fn a_failed_build_keeps_the_session_of_a_live_window() {
    let cleared = std::cell::Cell::new(0);
    clear_session_after_failed_build(true, || cleared.set(cleared.get() + 1));
    assert_eq!(cleared.get(), 0, "the live window still owns its session");
    clear_session_after_failed_build(false, || cleared.set(cleared.get() + 1));
    assert_eq!(cleared.get(), 1);
}

#[test]
fn webview_arguments_come_from_the_environment_in_debug_builds_only() {
    use clawd_app::windows::WEBVIEW_ARGS_ENV;
    use clawd_contracts::Env;
    let env = Env::from_map([(WEBVIEW_ARGS_ENV.to_owned(), "--disable-gpu".to_owned())].into());
    let want = cfg!(debug_assertions).then(|| "--disable-gpu".to_owned());
    assert_eq!(webview_args(&env), want);
    assert_eq!(webview_args(&Env::empty()), None);
}
