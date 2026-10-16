//! Token sources: ports of `test_token_store.py`, `test_freeride.py` (decode part) and the
//! token half of `test_macos_multidir.py` (Python tokens `sk-ant-oat-*` renamed `sk-ant-test-*`).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use clawd_config::tokens::{
    decode_keychain_blob, extract_access_token, has_claude_token_on, read_token_for_on,
    read_token_keychain_for, read_token_store, KEYCHAIN_SERVICE,
};
use clawd_config::{has_claude_token, read_token_for, Host};
use clawd_contracts::Env;
use clawd_testkit::config::{FakeReply, FakeRunner};
use clawd_testkit::TempConfig;
use serde_json::{json, Value};

const NOW: f64 = 1_800_000_000.0;
const USER: &str = "clawd-test-user";

fn mac() -> Host {
    Host::new(true, USER)
}

fn linux() -> Host {
    Host::new(false, USER)
}

fn cred(overrides: Value) -> Value {
    let mut base = json!({
        "version": "1.0", "type": "oauth_token", "access_token": "sk-ant-test-LIVE",
        "refresh_token": "sk-ant-fake-R", "expires_at": ((NOW + 3600.0) * 1000.0) as i64,
    });
    for (k, v) in overrides.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

/// `test_token_store._store`
fn store(dir: &Path, cred: &Value, profile: &str, active: bool) {
    fs::create_dir_all(dir.join("credentials")).unwrap();
    if active {
        fs::write(dir.join("active_config"), format!("{profile}\n")).unwrap();
    }
    fs::write(
        dir.join("credentials").join(format!("{profile}.json")),
        cred.to_string(),
    )
    .unwrap();
}

fn keychain_blob(token: &str) -> String {
    format!(r#"{{"claudeAiOauth":{{"accessToken":"{token}"}}}}"#)
}

fn keychain_runner(reply: FakeReply) -> FakeRunner {
    FakeRunner::new().keychain_item(KEYCHAIN_SERVICE, USER, reply)
}

fn hex_of(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

// ---- test_token_store.py -------------------------------------------------------------

#[test]
fn reads_live_token() {
    let t = TempConfig::new();
    store(t.root(), &cred(json!({})), "default", true);
    assert_eq!(
        read_token_store(t.root(), Some(NOW)).as_deref(),
        Some("sk-ant-test-LIVE")
    );
}

#[test]
fn follows_active_profile() {
    let t = TempConfig::new();
    store(
        t.root(),
        &cred(json!({"access_token": "OTHER"})),
        "work",
        true,
    );
    assert_eq!(
        read_token_store(t.root(), Some(NOW)).as_deref(),
        Some("OTHER")
    );
}

#[test]
fn missing_active_config_means_default() {
    let t = TempConfig::new();
    store(t.root(), &cred(json!({})), "default", false);
    assert_eq!(
        read_token_store(t.root(), Some(NOW)).as_deref(),
        Some("sk-ant-test-LIVE")
    );
}

#[test]
fn unusable_entries_return_none() {
    for bad in [
        json!({"expires_at": ((NOW - 1.0) * 1000.0) as i64}), // expired (ms)
        json!({"expires_at": (NOW - 1.0) as i64}),            // expired (s)
        json!({"type": "api_key"}),                           // not an OAuth login
        json!({"access_token": ""}),
        json!({"access_token": "  "}),
        json!({"access_token": 5}),
    ] {
        let t = TempConfig::new();
        store(t.root(), &cred(bad.clone()), "default", true);
        assert_eq!(read_token_store(t.root(), Some(NOW)), None, "{bad}");
    }
}

#[test]
fn expiry_edges_follow_python() {
    for (expires, want) in [
        (json!(0), true),      // 0 = no expiry
        (json!(-5), true),     // negative = no expiry
        (json!("soon"), true), // not a number = no expiry
        (json!(null), true),
        (json!(NOW), false), // exp <= now is expired
        (json!(NOW + 1.0), true),
    ] {
        let t = TempConfig::new();
        store(
            t.root(),
            &cred(json!({"expires_at": expires})),
            "default",
            true,
        );
        assert_eq!(
            read_token_store(t.root(), Some(NOW)).is_some(),
            want,
            "{expires}"
        );
    }
}

#[test]
fn token_is_trimmed() {
    let t = TempConfig::new();
    store(
        t.root(),
        &cred(json!({"access_token": " sk-ant-test-X \n"})),
        "default",
        true,
    );
    assert_eq!(
        read_token_store(t.root(), Some(NOW)).as_deref(),
        Some("sk-ant-test-X")
    );
}

#[test]
fn absent_or_corrupt_store() {
    let t = TempConfig::new();
    assert_eq!(read_token_store(&t.root().join("nope"), Some(NOW)), None);
    fs::create_dir_all(t.root().join("credentials")).unwrap();
    let default = t.root().join("credentials").join("default.json");
    fs::write(&default, "{not json").unwrap();
    assert_eq!(read_token_store(t.root(), Some(NOW)), None);
    fs::write(&default, "[1]").unwrap();
    assert_eq!(read_token_store(t.root(), Some(NOW)), None);
}

#[test]
fn profile_cannot_escape_store() {
    let t = TempConfig::new();
    fs::write(t.root().join("active_config"), "../../etc/x").unwrap();
    assert_eq!(read_token_store(t.root(), Some(NOW)), None);
    fs::write(t.root().join("active_config"), ".hidden").unwrap();
    assert_eq!(read_token_store(t.root(), Some(NOW)), None);
}

#[test]
fn real_clock_is_used_without_now() {
    let t = TempConfig::new();
    store(
        t.root(),
        &cred(json!({"expires_at": 4_102_444_800_000_i64})),
        "default",
        true,
    );
    assert!(read_token_store(t.root(), None).is_some());
    store(
        t.root(),
        &cred(json!({"expires_at": 1000})),
        "default",
        true,
    );
    assert_eq!(read_token_store(t.root(), None), None);
}

#[test]
fn default_dir_prefers_store_over_keychain() {
    let t = TempConfig::new();
    store(
        &t.paths().anthropic_store_dir(),
        &cred(json!({"expires_at": 4_102_444_800_000_i64})),
        "default",
        true,
    );
    let runner = keychain_runner(FakeReply::ok(&keychain_blob("sk-ant-test-KEYCHAIN")));
    let got = read_token_for_on(t.paths(), &runner, &t.paths().default_claude_dir(), &mac());
    assert_eq!(got.as_deref(), Some("sk-ant-test-LIVE"));
    assert!(runner.calls().is_empty(), "the Keychain is never asked");
}

#[test]
fn default_dir_falls_back_to_keychain_when_store_expired() {
    let t = TempConfig::new();
    store(
        &t.paths().anthropic_store_dir(),
        &cred(json!({"expires_at": 1000})),
        "default",
        true,
    );
    let runner = keychain_runner(FakeReply::ok(&keychain_blob("sk-ant-test-KEYCHAIN")));
    let got = read_token_for_on(t.paths(), &runner, &t.paths().default_claude_dir(), &mac());
    assert_eq!(got.as_deref(), Some("sk-ant-test-KEYCHAIN"));
}

#[test]
fn other_dirs_ignore_store() {
    let t = TempConfig::new();
    store(
        &t.paths().anthropic_store_dir(),
        &cred(json!({})),
        "default",
        true,
    );
    let work = t.home().join(".claude-work");
    fs::create_dir_all(&work).unwrap();
    let runner = FakeRunner::new();
    assert_eq!(read_token_for_on(t.paths(), &runner, &work, &linux()), None);
}

// ---- test_freeride.py (read path) ------------------------------------------------------

#[test]
fn decode_passthrough_plain_json() {
    let blob = r#"{"claudeAiOauth": {"accessToken": "x"}}"#;
    assert_eq!(decode_keychain_blob(blob), blob);
}

#[test]
fn decode_hex_dump_round_trips() {
    let original = "{\"claudeAiOauth\": {\"accessToken\": \"x\"}}\n";
    let hex = hex_of(original);
    assert_eq!(decode_keychain_blob(&hex), original);
    assert_eq!(decode_keychain_blob(&hex.to_uppercase()), original);
    assert_eq!(decode_keychain_blob(&format!("{hex}\n")), original);
}

#[test]
fn decode_odd_length_is_not_treated_as_hex() {
    assert_eq!(decode_keychain_blob("abc"), "abc");
}

#[test]
fn decode_keeps_input_that_is_hex_but_not_utf8() {
    assert_eq!(decode_keychain_blob("ff00"), "ff00");
    assert_eq!(decode_keychain_blob(""), "");
}

// ---- test_macos_multidir.py (token half) -------------------------------------------------

#[test]
fn token_for_reads_dir_credentials_file() {
    let t = TempConfig::new();
    let dir = t.home().join("a");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".credentials.json"), keychain_blob("TOK_X")).unwrap();
    assert_eq!(
        read_token_for_on(t.paths(), &FakeRunner::new(), &dir, &linux()).as_deref(),
        Some("TOK_X")
    );
}

#[test]
fn token_for_missing_file_non_default_returns_none() {
    let t = TempConfig::new();
    let dir = t.home().join("a");
    fs::create_dir_all(&dir).unwrap();
    let runner = keychain_runner(FakeReply::ok(&keychain_blob("sk-ant-test-KEYCHAIN")));
    // not the default dir: even on macOS the Keychain is not consulted
    assert_eq!(read_token_for_on(t.paths(), &runner, &dir, &mac()), None);
    assert!(runner.calls().is_empty());
}

#[test]
fn token_for_default_dir_falls_back_to_keychain_on_macos() {
    let t = TempConfig::new();
    let runner = keychain_runner(FakeReply::ok(&keychain_blob("TOK_KEYCHAIN")));
    let default = t.paths().default_claude_dir();
    assert_eq!(
        read_token_for_on(t.paths(), &runner, &default, &mac()).as_deref(),
        Some("TOK_KEYCHAIN")
    );
    assert_eq!(
        runner.args_of("security"),
        vec![vec![
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-a",
            USER,
            "-w"
        ]]
    );
    // off macOS the Keychain is never asked
    let runner = keychain_runner(FakeReply::ok(&keychain_blob("TOK_KEYCHAIN")));
    assert_eq!(
        read_token_for_on(t.paths(), &runner, &default, &linux()),
        None
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn token_for_file_wins_over_keychain() {
    let t = TempConfig::new();
    let default = t.paths().default_claude_dir();
    fs::create_dir_all(&default).unwrap();
    fs::write(
        default.join(".credentials.json"),
        r#"{"accessToken":"TOK_FILE"}"#,
    )
    .unwrap();
    let runner = keychain_runner(FakeReply::ok(&keychain_blob("TOK_KEYCHAIN")));
    assert_eq!(
        read_token_for_on(t.paths(), &runner, &default, &mac()).as_deref(),
        Some("TOK_FILE")
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn blank_token_in_file_reads_as_absent() {
    let t = TempConfig::new();
    let dir = t.home().join("a");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"","refreshToken":"","expiresAt":0}}"#,
    )
    .unwrap();
    assert_eq!(
        read_token_for_on(t.paths(), &FakeRunner::new(), &dir, &linux()),
        None
    );
}

#[test]
fn blank_credentials_file_of_default_dir_does_not_fall_through_to_keychain() {
    // Python: an existing `.credentials.json` decides, even when it yields no token
    let t = TempConfig::new();
    let default = t.paths().default_claude_dir();
    fs::create_dir_all(&default).unwrap();
    fs::write(default.join(".credentials.json"), r#"{"accessToken":""}"#).unwrap();
    let runner = keychain_runner(FakeReply::ok(&keychain_blob("TOK_KEYCHAIN")));
    assert_eq!(
        read_token_for_on(t.paths(), &runner, &default, &mac()),
        None
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn blank_token_in_blob_reads_as_absent() {
    assert_eq!(extract_access_token(r#"{"accessToken":""}"#), None);
    assert_eq!(
        extract_access_token(r#"{"claudeAiOauth":{"accessToken":"","expiresAt":0}}"#),
        None
    );
}

// ---- blob shapes (fixtures credentials_direct.json / credentials_nested.json) --------------

#[test]
fn extract_access_token_shapes() {
    // direct (tests/fixtures/credentials_direct.json)
    assert_eq!(
        extract_access_token("{\n  \"accessToken\": \"sk-ant-test-5678\"\n}").as_deref(),
        Some("sk-ant-test-5678")
    );
    // nested (tests/fixtures/credentials_nested.json)
    let nested = r#"{"claudeAiOauth": {"accessToken": "sk-ant-test-1234",
        "refreshToken": "sk-ant-fake-5678", "expiresAt": 9999999999000, "scopes": ["user:inference"]}}"#;
    assert_eq!(
        extract_access_token(nested).as_deref(),
        Some("sk-ant-test-1234")
    );
    // direct wins over nested; a later nested object is found when the first has none
    assert_eq!(
        extract_access_token(r#"{"a": {"x": 1}, "b": {"accessToken": "B"}, "accessToken": "TOP"}"#)
            .as_deref(),
        Some("TOP")
    );
    assert_eq!(
        extract_access_token(r#"{"a": {"x": 1}, "b": {"accessToken": "B"}}"#).as_deref(),
        Some("B")
    );
    // deeper than one level: found by the text scan
    assert_eq!(
        extract_access_token(r#"{"a": {"b": {"accessToken" :  "DEEP"}}}"#).as_deref(),
        Some("DEEP")
    );
    // JSON that did not parse: the text scan still works, skipping an empty first match
    assert_eq!(
        extract_access_token(r#"{"accessToken": "", "accessToken" : "SECOND", oops"#).as_deref(),
        Some("SECOND")
    );
    // raw token, with whitespace around it
    assert_eq!(
        extract_access_token("  sk-ant-test-abcdefghijkl\n").as_deref(),
        Some("sk-ant-test-abcdefghijkl")
    );
    // not plausible: too short, or not token characters
    assert_eq!(extract_access_token("short"), None);
    assert_eq!(extract_access_token("not a token at all, really no"), None);
    assert_eq!(extract_access_token(""), None);
    assert_eq!(extract_access_token("  \n"), None);
    assert_eq!(extract_access_token("[]"), None);
}

// ---- Keychain read ------------------------------------------------------------------------

#[test]
fn keychain_hex_blob_is_decoded_before_extracting() {
    let blob = format!("{}\n", keychain_blob("sk-ant-test-HEX"));
    let runner = keychain_runner(FakeReply::ok(&format!("{}\n", hex_of(&blob))));
    assert_eq!(
        read_token_keychain_for(&runner, &mac()).as_deref(),
        Some("sk-ant-test-HEX")
    );
}

#[test]
fn keychain_failures_read_as_absent() {
    for reply in [
        FakeReply::exit(44, "The specified item could not be found in the keychain."),
        FakeReply::not_found(),
        FakeReply::timed_out(),
        FakeReply::ok(""),
        FakeReply::ok("{\"mcpOAuth\": {}}"),
    ] {
        let runner = keychain_runner(reply.clone());
        assert_eq!(read_token_keychain_for(&runner, &mac()), None, "{reply:?}");
    }
    // no account name: nothing to ask for
    let runner = FakeRunner::new();
    assert_eq!(read_token_keychain_for(&runner, &Host::new(true, "")), None);
    assert!(runner.calls().is_empty());
}

#[test]
fn host_user_comes_from_getpass_variables_in_order() {
    let env = |pairs: &[(&str, &str)]| {
        Env::from_map(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<_, _>>(),
        )
    };
    let user = |pairs: &[(&str, &str)]| Host::from_env(true, &env(pairs)).user;
    assert_eq!(user(&[("USER", "u"), ("LOGNAME", "l")]), "l");
    assert_eq!(user(&[("USERNAME", "w"), ("USER", "u")]), "u");
    assert_eq!(user(&[]), "");
    assert!(!Host::from_env(false, &env(&[])).macos);
}

// ---- has_claude_token -----------------------------------------------------------------------

#[test]
fn has_claude_token_checks_every_configured_dir() {
    let t = TempConfig::new();
    let runner = FakeRunner::new();
    assert!(!has_claude_token_on(t.paths(), &runner, &linux()));
    t.write_config("config", "config_dirs = ~/.claude, ~/.claude-work\n");
    t.write_home(
        ".claude-work/.credentials.json",
        r#"{"accessToken":"sk-ant-test-W"}"#,
    );
    assert!(has_claude_token_on(t.paths(), &runner, &linux()));
}

#[test]
fn system_host_entry_points_only_ask_the_fake_runner() {
    // the public wrappers use the real OS/user; the Keychain is only ever the fake one
    let t = TempConfig::new();
    let runner = FakeRunner::new();
    assert_eq!(
        read_token_for(t.paths(), &runner, &t.home().join("x")),
        None
    );
    assert!(!has_claude_token(t.paths(), &runner));
}
