//! `usage.read_config_value` and the option readers (ports of `test_http_push`'s
//! `test_ble_setting_defaults_on`, `test_codex_usage`'s `test_usage_source_values`).

use clawd_config::{
    read_chime_setting, read_clock_setting, read_config_value, read_onoff_setting,
    read_usage_source,
};
use clawd_testkit::TempConfig;

fn with_config(text: &str) -> TempConfig {
    let cfg = TempConfig::new();
    cfg.write_config("config", text);
    cfg
}

#[test]
fn missing_file_reads_as_unset() {
    let cfg = TempConfig::new();
    assert_eq!(read_config_value(cfg.paths(), "clock"), None);
    assert_eq!(read_clock_setting(cfg.paths()), "auto");
    assert_eq!(read_chime_setting(cfg.paths()), "off");
}

#[test]
fn parses_comments_case_and_spacing() {
    let cfg = with_config(
        "# header\n  Clock =  24  # trailing\nbroken line\n=novalue\nhttp_targets=a=b\n\r\nEVENT_PORT = 0\n",
    );
    let p = cfg.paths();
    assert_eq!(read_config_value(p, "clock").as_deref(), Some("24"));
    assert_eq!(read_config_value(p, "http_targets").as_deref(), Some("a=b"));
    assert_eq!(read_config_value(p, "event_port").as_deref(), Some("0"));
    assert_eq!(read_config_value(p, "nope"), None);
    // `name` is compared as given: the caller passes it lower-case
    assert_eq!(read_config_value(p, "Clock"), None);
}

#[test]
fn first_matching_line_wins() {
    let cfg = with_config("clock = 12\nclock = 24\nchime = on\nchime = off\n");
    assert_eq!(
        read_config_value(cfg.paths(), "clock").as_deref(),
        Some("12")
    );
    assert_eq!(read_chime_setting(cfg.paths()), "on");
}

#[test]
fn clock_setting_skips_invalid_values_and_defaults_to_auto() {
    let cfg = with_config("clock = nonsense\nclock = AUTO\nclock = 12\n");
    assert_eq!(read_clock_setting(cfg.paths()), "auto");
    let cfg = with_config("clock = 7\n");
    assert_eq!(read_clock_setting(cfg.paths()), "auto");
    let cfg = with_config("clock = bogus\nclock = Off\n");
    assert_eq!(read_clock_setting(cfg.paths()), "off");
}

/// `test_http_push::test_ble_setting_defaults_on`
#[test]
fn onoff_setting_defaults_and_is_case_insensitive() {
    let cfg = with_config("");
    assert_eq!(read_onoff_setting(cfg.paths(), "ble", "on"), "on");
    let cfg = with_config("ble = off\n");
    assert_eq!(read_onoff_setting(cfg.paths(), "ble", "on"), "off");
    let cfg = with_config("ble = ON\nother = maybe\n");
    assert_eq!(read_onoff_setting(cfg.paths(), "ble", "off"), "on");
    assert_eq!(read_onoff_setting(cfg.paths(), "other", "off"), "off");
}

#[test]
fn unreadable_config_is_unset() {
    let cfg = TempConfig::new();
    // a directory where the file should be: Python's OSError branch
    std::fs::create_dir_all(cfg.paths().config_file()).unwrap();
    assert_eq!(read_config_value(cfg.paths(), "clock"), None);
}

/// `test_codex_usage::test_usage_source_values`
#[test]
fn usage_source_values() {
    let cfg = TempConfig::new();
    assert_eq!(read_usage_source(cfg.paths()), "claude");
    for (raw, want) in [
        (r#""codex""#, "codex"),
        (r#""off""#, "off"),
        (r#""claude:/x/.claude-work""#, "claude:/x/.claude-work"),
        (r#""claude:""#, "claude"),
        (r#""gemini""#, "claude"),
        ("3", "claude"),
        ("null", "claude"),
    ] {
        cfg.write_config("desktop.json", format!(r#"{{"usage_source": {raw}}}"#));
        assert_eq!(read_usage_source(cfg.paths()), want, "usage_source = {raw}");
    }
    cfg.write_config("desktop.json", "[1, 2]");
    assert_eq!(read_usage_source(cfg.paths()), "claude");
    cfg.write_config("desktop.json", "{broken");
    assert_eq!(read_usage_source(cfg.paths()), "claude");
}
