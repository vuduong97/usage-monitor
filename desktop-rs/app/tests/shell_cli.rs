//! The command line (`cli.py`) and `test_firstrun.test_cleanup_disconnects_every_agent`.
use clawd_app::cli::{
    autostart_report, cleanup_with, find_line, parse, remembered_clock, Command, FIND_SECONDS,
};
use clawd_contracts::{ClockSource, ClockTarget};
use serde_json::json;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

fn ok(a: &[&str]) -> Command {
    parse(&args(a)).expect("parses")
}

#[test]
fn no_arguments_is_the_tray() {
    assert_eq!(ok(&[]), Command::Tray);
    assert_eq!(ok(&["tray"]), Command::Tray);
    assert_eq!(
        ok(&["-psn_0_12345"]),
        Command::Tray,
        "Finder's -psn_ is dropped"
    );
}

#[test]
fn the_subcommands_parse() {
    assert_eq!(ok(&["run"]), Command::Run);
    assert_eq!(ok(&["cleanup"]), Command::Cleanup);
    assert_eq!(ok(&["hooks", "on"]), Command::Hooks { on: true });
    assert_eq!(ok(&["hooks", "off"]), Command::Hooks { on: false });
    assert_eq!(ok(&["autostart", "on"]), Command::Autostart { on: true });
    assert_eq!(ok(&["autostart", "off"]), Command::Autostart { on: false });
    assert_eq!(
        ok(&["find"]),
        Command::Find {
            seconds: FIND_SECONDS
        }
    );
    assert_eq!(
        ok(&["find", "--seconds", "2.5"]),
        Command::Find { seconds: 2.5 }
    );
    assert_eq!(ok(&["find", "--seconds=3"]), Command::Find { seconds: 3.0 });
}

#[test]
fn settings_takes_clock_key_and_tab() {
    assert_eq!(
        ok(&["settings"]),
        Command::Settings {
            clock: String::new(),
            key: String::new(),
            tab: String::new()
        }
    );
    assert_eq!(
        ok(&[
            "settings",
            "--piped",
            "--clock",
            "http://10.0.0.5",
            "--key=clawd",
            "--tab",
            "setup"
        ]),
        Command::Settings {
            clock: "http://10.0.0.5".into(),
            key: "clawd".into(),
            tab: "setup".into()
        }
    );
}

#[test]
fn bad_input_exits_2_and_version_exits_0() {
    for bad in [
        &["hooks"][..],
        &["hooks", "maybe"],
        &["autostart"],
        &["find", "--seconds", "soon"],
        &["find", "--seconds"],
        &["settings", "--tab", "nope"],
        &["settings", "--clock"],
        &["bogus"],
        &["run", "extra"],
        &["--autostart"],
    ] {
        let exit = parse(&args(bad)).expect_err(&format!("{bad:?}"));
        assert_eq!(exit.code, 2, "{bad:?}");
        assert!(exit.stderr);
    }
    let version = parse(&args(&["--version"])).unwrap_err();
    assert_eq!((version.code, version.stderr), (0, false));
    assert_eq!(
        version.text,
        format!("Clawd {}", clawd_contracts::APP_VERSION)
    );
    assert_eq!(parse(&args(&["--help"])).unwrap_err().code, 0);
}

#[test]
fn cleanup_prints_every_agent_and_still_runs_when_autostart_fails() {
    let mut out = Vec::new();
    cleanup_with(
        || Ok(true),
        || vec!["claude: Đã ngắt.".to_owned(), "pi: Chưa nối.".to_owned()],
        || "12:00:00".to_owned(),
        &mut out,
    );
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "[12:00:00] claude: Đã ngắt.\n[12:00:00] pi: Chưa nối.\n[12:00:00] cleanup done\n"
    );

    let mut out = Vec::new();
    cleanup_with(
        || Err(std::io::Error::other("registry locked")),
        || vec!["claude: Đã ngắt.".to_owned()],
        || "t".to_owned(),
        &mut out,
    );
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("autostart: registry locked") && text.contains("claude: Đã ngắt."));
    assert!(text.ends_with("cleanup done\n"));
}

#[test]
fn autostart_says_what_happened() {
    assert_eq!(autostart_report(true, Ok(true)), ("autostart on".into(), 0));
    assert_eq!(
        autostart_report(false, Ok(true)),
        ("autostart off".into(), 0)
    );
    assert_eq!(
        autostart_report(true, Ok(false)),
        ("autostart: not supported here".into(), 1)
    );
    assert_eq!(
        autostart_report(true, Err(std::io::Error::other("denied"))).1,
        1
    );
}

#[test]
fn find_lines_are_aligned_like_python() {
    let mut c = ClockTarget::new("10.0.0.7", ClockSource::Mdns);
    c.name = "clawd-1a2b".into();
    c.version = "1.4.2".into();
    assert_eq!(
        find_line(&c),
        format!(
            "mDNS  {:<20} {:<28} v1.4.2",
            "clawd-1a2b", "http://10.0.0.7"
        )
    );
}

#[test]
#[ignore = "needs 08 (clawd_clock::manual_clock)"]
fn the_remembered_clock_is_the_typed_one_else_the_saved_one() {
    assert_eq!(
        remembered_clock(&json!({"manual_host": "10.0.0.5"})),
        ("http://10.0.0.5".to_owned(), "10.0.0.5".to_owned())
    );
    assert_eq!(
        remembered_clock(&json!({"host": "10.0.0.7", "port": 8080, "name": "clawd"})),
        ("http://10.0.0.7:8080".to_owned(), "clawd".to_owned())
    );
    assert_eq!(remembered_clock(&json!({})), (String::new(), String::new()));
}
