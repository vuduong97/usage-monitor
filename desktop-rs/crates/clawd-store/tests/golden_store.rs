//! Golden tests: the fixtures in `desktop-rs/fixtures/store/` were written by
//! `tools/golden/export_store.py` from the real Python `paths`, `pause`, `firstrun`, `autostart`
//! and `creds`. Files are compared byte for byte (`file_text`, plist text, sha256).

use clawd_contracts::{AppPaths, Prefs, UsagePayload, APP_VERSION, LAUNCH_AGENT_LABEL};
use clawd_store::autostart::{launch_agent, plist_xml, run_value};
use clawd_store::creds::{forget_with, get_with, put_with};
use clawd_store::paths::{ota_busy, set_usage_payload, usage_payload};
use clawd_store::pause::{hold_at, release, PauseReader};
use clawd_store::prefs::{clock_screen, load_prefs, save_prefs, set_clock_screen, update_prefs};
use clawd_store::{firstrun, pyjson};
use clawd_testkit::store::{
    put_config_file, read_config_file, reset_config_dir, set_mtime, MemKeyring, SandboxAutostart,
};
use clawd_testkit::{load_fixture_value, FakeClock, TempConfig};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const VERSION_TOKEN: &str = "@VERSION@";

fn output(name: &str) -> Value {
    load_fixture_value("store", name)["output"].clone()
}

fn input(name: &str) -> Value {
    load_fixture_value("store", name)["input"].clone()
}

fn prefs_of(value: &Value) -> Prefs {
    serde_json::from_value(value.clone()).expect("prefs object")
}

fn opt_str(value: &Value) -> Option<&str> {
    value.as_str()
}

fn array(value: &Value) -> &Vec<Value> {
    value.as_array().expect("array")
}

fn strings(value: &Value) -> Vec<String> {
    array(value)
        .iter()
        .map(|v| v.as_str().expect("string").to_owned())
        .collect()
}

fn swap_version(text: &str) -> String {
    text.replace(APP_VERSION, VERSION_TOKEN)
}

fn unswap_version(text: &str) -> String {
    text.replace(VERSION_TOKEN, APP_VERSION)
}

fn swap_value(value: &Value) -> Value {
    match value {
        Value::String(s) => Value::String(swap_version(s)),
        Value::Array(items) => Value::Array(items.iter().map(swap_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), swap_value(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn desktop_json(paths: &AppPaths) -> Option<String> {
    read_config_file(paths, "desktop.json")
}

// ---- prefs ------------------------------------------------------------------------

#[test]
fn prefs_update_matches_python_byte_for_byte() {
    let t = TempConfig::new();
    let paths = t.paths();
    let out = output("prefs_update");
    for case in array(&out["update"]) {
        let name = case["name"].as_str().expect("name");
        reset_config_dir(paths);
        put_config_file(paths, "desktop.json", opt_str(&case["input"]["file"]));
        let ops = array(&case["input"]["ops"]);
        let result = update_prefs(paths, |prefs| {
            for op in ops {
                match op[0].as_str().expect("op") {
                    "set" => {
                        prefs.insert(op[1].as_str().expect("key").to_owned(), op[2].clone());
                    }
                    "del" => {
                        prefs.shift_remove(op[1].as_str().expect("key"));
                    }
                    other => panic!("unknown op {other}"),
                }
            }
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            desktop_json(paths).as_deref(),
            opt_str(&case["output"]["file_text"]),
            "{name}: file text"
        );
        assert_eq!(
            serde_json::to_string(&result).expect("json"),
            serde_json::to_string(&prefs_of(&case["output"]["prefs"])).expect("json"),
            "{name}: returned prefs (order included)"
        );
    }
}

#[test]
fn clock_screen_and_load_prefs_read_like_python() {
    let t = TempConfig::new();
    let paths = t.paths();
    for case in array(&output("prefs_update")["reads"]) {
        let text = case["file"].as_str().expect("file");
        reset_config_dir(paths);
        put_config_file(paths, "desktop.json", Some(text));
        assert_eq!(
            clock_screen(paths),
            case["clock_screen"].as_i64(),
            "clock_screen of {text:?}"
        );
        assert_eq!(
            load_prefs(paths),
            prefs_of(&case["load_prefs"]),
            "load_prefs of {text:?}"
        );
    }
}

#[test]
fn set_clock_screen_and_save_prefs_match_python() {
    let t = TempConfig::new();
    let paths = t.paths();
    let out = output("prefs_update");
    reset_config_dir(paths);
    put_config_file(paths, "desktop.json", Some("{\"a\": 1}"));
    set_clock_screen(paths, 2).expect("set");
    assert_eq!(
        desktop_json(paths).as_deref(),
        opt_str(&out["set_clock_screen_2_on_a1"]["file_text"])
    );
    for case in array(&out["save"]) {
        let name = case["name"].as_str().expect("name");
        reset_config_dir(paths);
        put_config_file(paths, "desktop.json", opt_str(&case["input"]["file"]));
        save_prefs(paths, &prefs_of(&case["input"]["data"])).expect("save");
        assert_eq!(
            desktop_json(paths).as_deref(),
            opt_str(&case["output"]["file_text"]),
            "{name}"
        );
    }
}

#[test]
fn the_sample_roundtrip_fixture_still_holds() {
    let t = TempConfig::new();
    let paths = t.paths();
    let sample = load_fixture_value("store", "prefs_roundtrip");
    reset_config_dir(paths);
    put_config_file(
        paths,
        "desktop.json",
        Some(&pyjson::dumps(&sample["input"]["initial"], pyjson::Style::Indent2).expect("dumps")),
    );
    set_clock_screen(paths, 1).expect("set");
    assert_eq!(
        desktop_json(paths).as_deref(),
        opt_str(&sample["output"]["file_text"])
    );
    assert_eq!(clock_screen(paths), Some(1));
}

// ---- firstrun ---------------------------------------------------------------------

#[test]
fn firstrun_matches_python() {
    let t = TempConfig::new();
    let paths = t.paths();
    let fixture = output("firstrun");
    assert_eq!(
        strings(&input("firstrun")["steps"]),
        firstrun::STEPS.to_vec()
    );
    for case in array(&fixture["cases"]) {
        let which = case["prefs"].as_str().expect("prefs");
        let func = case["fn"].as_str().expect("fn");
        let args = array(&case["args"]);
        let label = format!("{which}/{func}{args:?}");
        reset_config_dir(paths);
        let initial = case["file"].as_str().map(unswap_version);
        put_config_file(paths, "desktop.json", initial.as_deref());

        let result = match func {
            "migrate" => serde_json::to_value(firstrun::migrate(&load_prefs(paths))).expect("v"),
            "load" => serde_json::to_value(firstrun::load(paths).expect("load")).expect("v"),
            "pending" => {
                let from_file = firstrun::pending(paths, None);
                assert_eq!(
                    from_file,
                    firstrun::pending(paths, Some(&load_prefs(paths))),
                    "{label}"
                );
                json!(from_file)
            }
            "should_open" => {
                let from_file = firstrun::should_open(paths, None);
                assert_eq!(
                    from_file,
                    firstrun::should_open(paths, Some(&load_prefs(paths))),
                    "{label}"
                );
                json!(from_file)
            }
            "mark_done" => {
                let steps = strings(&args[0]);
                let steps: Vec<&str> = steps.iter().map(String::as_str).collect();
                json!(firstrun::mark_done(paths, &steps).expect("mark_done"))
            }
            "mark_shown" => {
                firstrun::mark_shown(paths).expect("mark_shown");
                Value::Null
            }
            "seen" => json!(firstrun::seen(paths).into_iter().collect::<Vec<_>>()),
            "mark_seen" => {
                let ids = strings(&args[0]);
                let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
                firstrun::mark_seen(paths, &ids).expect("mark_seen");
                Value::Null
            }
            "new_agents" => json!(firstrun::new_agents(
                paths,
                &strings(&args[0]),
                &strings(&args[1])
            )),
            other => panic!("unknown call {other}"),
        };
        assert_eq!(swap_value(&result), case["result"], "{label}: result");
        assert_eq!(
            desktop_json(paths).as_deref().map(swap_version),
            case["file_text"].as_str().map(str::to_owned),
            "{label}: file text"
        );
    }
}

// ---- pause and ota.busy -------------------------------------------------------------

#[test]
fn pause_hold_writes_the_python_text() {
    let t = TempConfig::new();
    let paths = t.paths();
    let fixture = output("pause");
    let pid_token = input("pause")["pid_token"]
        .as_str()
        .expect("pid token")
        .to_owned();
    assert_eq!(input("pause")["max_lease"], 300.0);
    for case in array(&fixture["hold"]) {
        let i = &case["input"];
        reset_config_dir(paths);
        hold_at(
            paths,
            i["seconds"].as_f64().expect("seconds"),
            i["reason"].as_str().expect("reason"),
            i["now"].as_f64().expect("now"),
        )
        .expect("hold");
        let text = read_config_file(paths, "pause.json")
            .expect("pause.json")
            .replace(&format!("\"pid\": {}", std::process::id()), &pid_token);
        assert_eq!(
            Some(text.as_str()),
            opt_str(&case["output"]["file_text"]),
            "{i}"
        );
    }
    reset_config_dir(paths);
    release(paths).expect("release of a missing file");
    hold_at(paths, 10.0, "x", 1.0).expect("hold");
    release(paths).expect("release");
    assert_eq!(
        paths.pause_file().exists(),
        fixture["release"]["exists_after_release"]
            .as_bool()
            .expect("bool")
    );
}

#[test]
fn pause_reader_matches_python() {
    let t = TempConfig::new();
    let paths = t.paths();
    let fixture = output("pause");
    for case in array(&fixture["reads"]) {
        let name = case["name"].as_str().expect("name");
        reset_config_dir(paths);
        put_config_file(paths, "pause.json", opt_str(&case["input"]["file"]));
        let clock = FakeClock::new(case["input"]["now"].as_f64().expect("now"));
        let reader = PauseReader::with_clock(paths, clock.arc());
        assert_eq!(
            reader.reason().as_deref(),
            opt_str(&case["output"]["reason"]),
            "{name}"
        );
    }
    reset_config_dir(paths);
    let reader = PauseReader::with_clock(paths, FakeClock::new(1.0).arc());
    assert_eq!(
        reader.reason().as_deref(),
        opt_str(&fixture["missing_file"])
    );
}

#[test]
fn ota_busy_matches_python() {
    let t = TempConfig::new();
    let paths = t.paths();
    let fixture = output("ota_busy");
    let mtime = input("ota_busy")["mtime"].as_u64().expect("mtime");
    for case in array(&fixture["cases"]) {
        reset_config_dir(paths);
        put_config_file(paths, "ota.busy", Some("1"));
        set_mtime(&paths.ota_busy_file(), mtime);
        let now = case["now"].as_f64().expect("now");
        assert_eq!(
            ota_busy(paths, Some(now)),
            case["busy"].as_bool().expect("busy"),
            "age {}",
            case["age"]
        );
    }
    reset_config_dir(paths);
    assert_eq!(
        ota_busy(paths, Some(mtime as f64)),
        fixture["missing_file"].as_bool().expect("bool")
    );
}

// ---- usage payload ------------------------------------------------------------------

#[test]
fn usage_payload_file_matches_python() {
    let t = TempConfig::new();
    let paths = t.paths();
    let fixture = output("usage_payload");
    for case in array(&fixture["writes"]) {
        reset_config_dir(paths);
        let payload: UsagePayload =
            serde_json::from_value(case["input"]["payload"].clone()).expect("payload");
        let now = case["input"]["now"].as_f64().expect("now");
        set_usage_payload(paths, &payload, Some(now)).expect("write");
        assert_eq!(
            read_config_file(paths, "usage_payload.json").as_deref(),
            opt_str(&case["output"]["file_text"]),
            "{}",
            case["input"]
        );
        let read = usage_payload(paths).map(|(p, at)| json!([p, at]));
        assert_eq!(
            read.unwrap_or(Value::Null),
            case["output"]["read"],
            "{}",
            case["input"]
        );
    }
}

#[test]
fn usage_payload_reads_odd_files_like_python() {
    let t = TempConfig::new();
    let paths = t.paths();
    let fixture = output("usage_payload");
    for case in array(&fixture["reads"]) {
        let text = case["file"].as_str().expect("file");
        reset_config_dir(paths);
        put_config_file(paths, "usage_payload.json", Some(text));
        let got = usage_payload(paths)
            .map(|(payload, at)| json!({"payload": payload, "at": at}))
            .unwrap_or(Value::Null);
        assert_eq!(got, case["result"], "file {text:?}");
    }
    reset_config_dir(paths);
    assert!(usage_payload(paths).is_none());
    assert!(fixture["missing_file"].is_null());
}

// ---- autostart ----------------------------------------------------------------------

fn plist_to_json(value: &plist::Value) -> Value {
    match value {
        plist::Value::Boolean(b) => json!(b),
        plist::Value::String(s) => json!(s),
        plist::Value::Integer(i) => json!(i.as_signed().expect("small int")),
        plist::Value::Array(items) => Value::Array(items.iter().map(plist_to_json).collect()),
        plist::Value::Dictionary(d) => Value::Object(
            d.iter()
                .map(|(k, v)| (k.clone(), plist_to_json(v)))
                .collect(),
        ),
        other => panic!("unexpected plist value {other:?}"),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn launch_agent_plist_is_a_byte_copy_of_python() {
    let fixture = output("autostart");
    assert_eq!(input("autostart")["label"], LAUNCH_AGENT_LABEL);
    for case in array(&fixture["mac"]) {
        let cmd = strings(&case["input"]["cmd"]);
        let out = &case["output"];
        let agent = launch_agent(LAUNCH_AGENT_LABEL, &cmd);
        assert_eq!(
            plist_to_json(&plist::Value::Dictionary(agent.clone())),
            out["dict"],
            "{cmd:?}: dict"
        );
        let bytes = plist_xml(&agent).expect("plist");
        assert_eq!(
            String::from_utf8(bytes.clone()).expect("utf8"),
            out["plist_text"].as_str().expect("text"),
            "{cmd:?}: text"
        );
        assert_eq!(
            sha256_hex(&bytes),
            out["plist_sha256"].as_str().expect("sha"),
            "{cmd:?}: sha256"
        );
    }
}

#[test]
fn mac_autostart_flow_writes_the_python_bytes_under_a_temp_home() {
    let t = TempConfig::new();
    let fixture = output("autostart");
    for case in array(&fixture["mac"]) {
        let cmd = strings(&case["input"]["cmd"]);
        let out = &case["output"];
        let sandbox = SandboxAutostart::new(
            t.paths(),
            t.os_names(),
            clawd_contracts::Platform::Darwin,
            cmd.clone(),
        );
        let auto = &sandbox.autostart;
        assert!(auto.supported());
        assert_eq!(
            auto.is_enabled(),
            out["is_enabled_before"].as_bool().expect("b")
        );
        assert_eq!(
            auto.enable().expect("enable"),
            out["enabled_returns"].as_bool().expect("b")
        );
        assert_eq!(
            auto.is_enabled(),
            out["is_enabled_after"].as_bool().expect("b")
        );
        // the same bytes as Python, except the label of the throwaway LaunchAgent
        let written = std::fs::read(auto.plist_path()).expect("plist file");
        assert!(auto.plist_path().starts_with(t.home()));
        let want = out["plist_text"].as_str().expect("text").replace(
            &format!("<string>{LAUNCH_AGENT_LABEL}</string>"),
            &format!("<string>{}</string>", t.os_names().launch_label),
        );
        assert_eq!(String::from_utf8(written).expect("utf8"), want, "{cmd:?}");
        assert_eq!(
            auto.disable().expect("disable"),
            out["disable_returns"].as_bool().expect("b")
        );
        assert_eq!(
            !auto.is_enabled(),
            out["gone_after_disable"].as_bool().expect("b")
        );
        assert_eq!(
            auto.disable().expect("disable again"),
            out["disable_again_returns"].as_bool().expect("b")
        );
    }
}

#[test]
fn run_value_matches_python() {
    let fixture = output("autostart");
    assert_eq!(input("autostart")["run_value"], "Clawd");
    for case in array(&fixture["windows"]) {
        let cmd = strings(&case["input"]["cmd"]);
        assert_eq!(
            Some(run_value(&cmd).as_str()),
            opt_str(&case["output"]["run_value"]),
            "{cmd:?}"
        );
        assert_eq!(case["output"]["run_type"], "REG_SZ");
    }
}

#[cfg(windows)]
#[test]
fn windows_autostart_flow_uses_a_sandbox_registry_key() {
    let t = TempConfig::new();
    let fixture = output("autostart");
    for case in array(&fixture["windows"]) {
        let cmd = strings(&case["input"]["cmd"]);
        let out = &case["output"];
        let sandbox = SandboxAutostart::new(
            t.paths(),
            t.os_names(),
            clawd_contracts::Platform::Win32,
            cmd,
        );
        let auto: &clawd_store::autostart::Autostart = &sandbox.autostart;
        assert_ne!(auto.run_key(), clawd_contracts::RUN_KEY);
        assert_eq!(
            auto.is_enabled(),
            out["is_enabled_before"].as_bool().expect("b")
        );
        assert_eq!(
            auto.enable().expect("enable"),
            out["enabled_returns"].as_bool().expect("b")
        );
        assert_eq!(
            auto.is_enabled(),
            out["is_enabled_after"].as_bool().expect("b")
        );
        assert_eq!(
            auto.disable().expect("disable"),
            out["disable_returns"].as_bool().expect("b")
        );
        assert_eq!(
            !auto.is_enabled(),
            out["gone_after_disable"].as_bool().expect("b")
        );
        assert_eq!(
            auto.disable().expect("again"),
            out["disable_again_returns"].as_bool().expect("b")
        );
    }
}

#[test]
fn unsupported_platform_answers_like_python() {
    let t = TempConfig::new();
    let u = &output("autostart")["unsupported"];
    let probe =
        |platform| SandboxAutostart::new(t.paths(), t.os_names(), platform, vec!["/x".to_owned()]);
    for (name, platform) in [
        ("darwin", clawd_contracts::Platform::Darwin),
        ("win32", clawd_contracts::Platform::Win32),
        ("linux", clawd_contracts::Platform::Linux),
    ] {
        assert_eq!(
            probe(platform).autostart.supported(),
            u["supported"][name].as_bool().expect("b"),
            "{name}"
        );
    }
    let linux = probe(clawd_contracts::Platform::Linux);
    let a = &linux.autostart;
    assert_eq!(
        a.enable().expect("enable"),
        u["linux_enable"].as_bool().expect("b")
    );
    assert_eq!(
        a.disable().expect("disable"),
        u["linux_disable"].as_bool().expect("b")
    );
    assert_eq!(a.is_enabled(), u["linux_is_enabled"].as_bool().expect("b"));
    assert_eq!(
        a.set_enabled(true).expect("set"),
        u["linux_set_enabled_true"].as_bool().expect("b")
    );
}

// ---- creds --------------------------------------------------------------------------

#[test]
fn creds_secret_format_matches_python() {
    let t = TempConfig::new();
    let names = t.os_names();
    assert!(!names.is_production());
    let service = &names.keyring_service;
    let fixture = output("creds");
    assert_eq!(
        input("creds")["default_user"],
        clawd_store::creds::DEFAULT_USER
    );
    let pair = |v: &Value| -> Option<(String, String)> {
        v.as_array().map(|a| {
            (
                a[0].as_str().expect("user").to_owned(),
                a[1].as_str().expect("pw").to_owned(),
            )
        })
    };
    for case in array(&fixture["puts"]) {
        let i = &case["input"];
        let key = i["key"].as_str().expect("key");
        let kr = MemKeyring::new();
        let ok = put_with(
            &kr,
            names,
            key,
            i["user"].as_str().expect("user"),
            i["password"].as_str().expect("pw"),
        );
        assert_eq!(ok, case["output"]["ok"].as_bool().expect("ok"), "{i}");
        assert_eq!(
            kr.secret(service, key).as_deref(),
            opt_str(&case["output"]["secret"]),
            "{i}"
        );
        assert_eq!(
            get_with(&kr, names, key),
            pair(&case["output"]["get"]),
            "{i}"
        );
    }
    for case in array(&fixture["gets"]) {
        let kr = MemKeyring::new();
        kr.insert(service, "k", case["secret"].as_str().expect("secret"));
        assert_eq!(
            get_with(&kr, names, "k"),
            pair(&case["get"]),
            "secret {}",
            case["secret"]
        );
    }
    let kr = MemKeyring::new();
    assert_eq!(get_with(&kr, names, "nope"), None);
    assert!(fixture["missing"].is_null());
    kr.insert(service, "k", "u\np");
    forget_with(&kr, names, "k");
    forget_with(&kr, names, "k");
    assert_eq!(
        kr.secret(service, "k").as_deref(),
        opt_str(&fixture["after_forget"])
    );
    assert!(kr.is_empty());
}
