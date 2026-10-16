//! `read_config_dirs`, `http_targets`, CLIProxy settings and `detect_hour_format`: ports of the
//! matching parts of `test_macos_multidir.py`, `test_http_push.py`, `test_usage_sources.py`.

use clawd_config::cliproxy::{
    read_cliproxy_key_on, CLIPROXY_KEYCHAIN_SERVICE, CLIPROXY_LEGACY_SERVICE,
};
use clawd_config::{
    detect_hour_format, normalize_http_target, read_cliproxy_url, read_config_dirs,
    read_http_targets, Host,
};
use clawd_testkit::config::{FakeReply, FakeRunner};
use clawd_testkit::TempConfig;

fn with_config(text: &str) -> TempConfig {
    let cfg = TempConfig::new();
    cfg.write_config("config", text);
    cfg
}

// ---- read_config_dirs (test_macos_multidir.py) ---------------------------------------------

#[test]
fn config_dirs_defaults_to_claude_when_unset() {
    let cfg = TempConfig::new(); // no config file
    assert_eq!(
        read_config_dirs(cfg.paths()),
        vec![cfg.paths().default_claude_dir()]
    );
}

#[test]
fn config_dirs_defaults_when_key_absent() {
    let cfg = with_config("clock = auto\nchime = on\n");
    assert_eq!(
        read_config_dirs(cfg.paths()),
        vec![cfg.paths().default_claude_dir()]
    );
}

#[test]
fn config_dirs_parses_comma_list_and_expands_tilde() {
    let cfg = with_config("config_dirs = ~/.claude, ~/.claude-work  # two plans\n");
    assert_eq!(
        read_config_dirs(cfg.paths()),
        vec![cfg.home().join(".claude"), cfg.home().join(".claude-work")]
    );
}

#[test]
fn config_dirs_blank_entries_and_empty_value_fall_back() {
    let cfg = with_config("config_dirs = , ,\n");
    assert_eq!(
        read_config_dirs(cfg.paths()),
        vec![cfg.paths().default_claude_dir()]
    );
    let cfg = with_config("config_dirs =\n");
    assert_eq!(
        read_config_dirs(cfg.paths()),
        vec![cfg.paths().default_claude_dir()]
    );
}

#[test]
fn config_dirs_last_line_wins_and_plain_paths_stay() {
    let cfg = with_config("config_dirs = ~/a\nCONFIG_DIRS = ~, ~/b/, rel/dir, ./c\n");
    assert_eq!(
        read_config_dirs(cfg.paths()),
        vec![
            cfg.home().to_path_buf(),
            cfg.home().join("b"),
            "rel/dir".into(),
            "c".into()
        ]
    );
}

// ---- http targets (test_http_push.py) ------------------------------------------------------

/// `test_http_push::test_normalize_http_target`
#[test]
fn normalize_http_target_cases() {
    let n = normalize_http_target;
    assert_eq!(
        n("192.168.0.125").as_deref(),
        Some("http://192.168.0.125/api/usage")
    );
    assert_eq!(
        n(" clawd.local:8080 ").as_deref(),
        Some("http://clawd.local:8080/api/usage")
    );
    assert_eq!(
        n("http://10.0.0.5/").as_deref(),
        Some("http://10.0.0.5/api/usage")
    );
    assert_eq!(
        n("http://10.0.0.5/custom/push").as_deref(),
        Some("http://10.0.0.5/custom/push")
    );
    assert_eq!(n(""), None);
    assert_eq!(n("ftp://host"), None);
}

#[test]
fn normalize_http_target_url_details() {
    let n = normalize_http_target;
    assert_eq!(
        n("HTTPS://Host:8443?x=1#frag").as_deref(),
        Some("https://Host:8443/api/usage?x=1")
    );
    assert_eq!(
        n("http://user:pw@10.0.0.5/p?q=1&r=2").as_deref(),
        Some("http://user:pw@10.0.0.5/p?q=1&r=2")
    );
    assert_eq!(
        n("http://[::1]:80").as_deref(),
        Some("http://[::1]:80/api/usage")
    );
    assert_eq!(n("http:///path"), None); // no host
    assert_eq!(n("http://:80"), None);
    assert_eq!(n("http://[::1"), None); // Python raises ValueError
    assert_eq!(n("   "), None);
}

/// `test_http_push::test_read_http_targets`
#[test]
fn read_http_targets_dedups_and_skips_invalid() {
    let cfg = with_config("http_targets = 192.168.0.125, ,bad://x, 192.168.0.125 # dup\n");
    assert_eq!(
        read_http_targets(cfg.paths()),
        vec!["http://192.168.0.125/api/usage"]
    );
    let cfg = with_config("clock = auto\n");
    assert!(read_http_targets(cfg.paths()).is_empty());
    let cfg = with_config("http_targets = b.local, a.local:81, http://c/x\n");
    assert_eq!(
        read_http_targets(cfg.paths()),
        vec![
            "http://b.local/api/usage",
            "http://a.local:81/api/usage",
            "http://c/x"
        ]
    );
}

// ---- CLIProxy (test_usage_sources.py) ------------------------------------------------------

/// `test_usage_sources::test_cliproxy_url_from_config`
#[test]
fn cliproxy_url_from_config() {
    let cfg = TempConfig::new();
    assert_eq!(read_cliproxy_url(cfg.paths()), "http://localhost:8317");
    let cfg = with_config("cliproxy_url = 127.0.0.1:9999/\n");
    assert_eq!(read_cliproxy_url(cfg.paths()), "http://127.0.0.1:9999");
    let cfg = with_config("cliproxy_url = https://proxy.example//\n");
    assert_eq!(read_cliproxy_url(cfg.paths()), "https://proxy.example");
    let cfg = with_config("cliproxy_url =\n");
    assert_eq!(read_cliproxy_url(cfg.paths()), "http://localhost:8317");
}

fn mac() -> Host {
    Host::new(true, "clawd-test-user")
}

/// `test_usage_sources::test_cliproxy_key_falls_back_to_the_service_name_used_before_the_rename`
#[test]
fn cliproxy_key_falls_back_to_the_service_name_used_before_the_rename() {
    let runner = FakeRunner::new()
        .keychain_service(CLIPROXY_KEYCHAIN_SERVICE, FakeReply::exit(44, "not found"))
        .keychain_service(CLIPROXY_LEGACY_SERVICE, FakeReply::ok("old-key\n"));
    assert_eq!(
        read_cliproxy_key_on(&runner, &mac()).as_deref(),
        Some("old-key")
    );
    let services: Vec<String> = runner
        .args_of("security")
        .iter()
        .map(|a| a[2].clone())
        .collect();
    assert_eq!(services, ["clawd-cliproxy", "clawdmeter-cliproxy"]);

    // nothing in either: None
    let runner = FakeRunner::new()
        .keychain_service(CLIPROXY_KEYCHAIN_SERVICE, FakeReply::exit(44, ""))
        .keychain_service(CLIPROXY_LEGACY_SERVICE, FakeReply::exit(44, ""));
    assert_eq!(read_cliproxy_key_on(&runner, &mac()), None);
    assert_eq!(runner.calls().len(), 2);
}

#[test]
fn cliproxy_key_details() {
    // the current name wins, the legacy one is never asked
    let runner = FakeRunner::new()
        .keychain_service(CLIPROXY_KEYCHAIN_SERVICE, FakeReply::ok("new-key\n"))
        .keychain_service(CLIPROXY_LEGACY_SERVICE, FakeReply::ok("old-key\n"));
    assert_eq!(
        read_cliproxy_key_on(&runner, &mac()).as_deref(),
        Some("new-key")
    );
    assert_eq!(runner.calls().len(), 1);

    // hex-dumped secret
    let hex: String = "hex-key\n".bytes().map(|b| format!("{b:02x}")).collect();
    let runner = FakeRunner::new().keychain_service(CLIPROXY_KEYCHAIN_SERVICE, FakeReply::ok(&hex));
    assert_eq!(
        read_cliproxy_key_on(&runner, &mac()).as_deref(),
        Some("hex-key")
    );

    // a blank value in the current item moves on to the legacy one
    let runner = FakeRunner::new()
        .keychain_service(CLIPROXY_KEYCHAIN_SERVICE, FakeReply::ok("  \n"))
        .keychain_service(CLIPROXY_LEGACY_SERVICE, FakeReply::ok("old-key"));
    assert_eq!(
        read_cliproxy_key_on(&runner, &mac()).as_deref(),
        Some("old-key")
    );

    // `security` missing or hung: give up without trying the legacy name
    for reply in [FakeReply::not_found(), FakeReply::timed_out()] {
        let runner = FakeRunner::new().keychain_service(CLIPROXY_KEYCHAIN_SERVICE, reply);
        assert_eq!(read_cliproxy_key_on(&runner, &mac()), None);
        assert_eq!(runner.calls().len(), 1);
    }

    // off macOS: never asks
    let runner = FakeRunner::new().keychain_service(CLIPROXY_KEYCHAIN_SERVICE, FakeReply::ok("k"));
    assert_eq!(read_cliproxy_key_on(&runner, &Host::new(false, "u")), None);
    assert!(runner.calls().is_empty());
}

// ---- detect_hour_format --------------------------------------------------------------------

#[test]
fn detect_hour_format_follows_the_explicit_toggles() {
    let both_unset = FakeRunner::new()
        .defaults(
            "AppleICUForce24HourTime",
            FakeReply::exit(1, "does not exist"),
        )
        .defaults(
            "AppleICUForce12HourTime",
            FakeReply::exit(1, "does not exist"),
        );
    assert_eq!(detect_hour_format(&both_unset), 24);

    let twelve = FakeRunner::new()
        .defaults("AppleICUForce24HourTime", FakeReply::ok("0\n"))
        .defaults("AppleICUForce12HourTime", FakeReply::ok("1\n"));
    assert_eq!(detect_hour_format(&twelve), 12);

    // 24 is checked first and wins when both say 1
    let both = FakeRunner::new()
        .defaults("AppleICUForce24HourTime", FakeReply::ok("1\n"))
        .defaults("AppleICUForce12HourTime", FakeReply::ok("1\n"));
    assert_eq!(detect_hour_format(&both), 24);
    assert_eq!(both.calls().len(), 1);

    // `defaults` missing (Windows) or hung: 24
    assert_eq!(detect_hour_format(&FakeRunner::new()), 24);
    let hung = FakeRunner::new().defaults("AppleICUForce24HourTime", FakeReply::timed_out());
    assert_eq!(detect_hour_format(&hung), 24);
}
