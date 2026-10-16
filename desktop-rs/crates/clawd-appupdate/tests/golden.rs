//! Golden fixtures (`desktop-rs/fixtures/appupdate/*.json`, written by
//! `tools/golden/export_appupdate.py` from the real Python `appupdate.py`): the Rust port must
//! answer exactly what Python answered. Network is an in-memory fake, programs are fakes.
//!
//! The release site is rebuilt from the fixture input by the rule in the exporter's docstring;
//! the port additionally serves (and verifies) a `SHA256SUMS.minisig` signed with a key made for
//! the test, which Python never had.

mod common;

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clawd_appupdate::codesign::{check_bundle, mac_swap_script, NO_TEAM};
use clawd_appupdate::launch::WINDOWS_DETACHED_FLAGS;
use clawd_appupdate::source::{auto_enabled, from_settings};
use clawd_appupdate::{
    asset_name, can_apply, is_newer, parse_sums, parse_version, team_id, LaunchKind, Source,
    Updater,
};
use clawd_config::CommandOutput;
use clawd_contracts::{AppRelease, Env, SUMS_NAME, SUMS_SIG_NAME};
use clawd_testkit::appupdate::{output_ok, FakeGitHub, FakeReply, FakeRunner};
use clawd_testkit::{load_fixture_value, TempConfig};
use common::*;
use serde_json::{json, Value};

fn fixture(name: &str) -> (Value, Value) {
    let v = load_fixture_value("appupdate", name);
    (v["input"].clone(), v["output"].clone())
}

/// `txt()` of the exporter: a string, or `{repeat, count, prefix?, suffix?}`.
fn txt(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => format!(
            "{}{}{}",
            other["prefix"].as_str().unwrap_or(""),
            other["repeat"]
                .as_str()
                .unwrap()
                .repeat(other["count"].as_u64().unwrap() as usize),
            other["suffix"].as_str().unwrap_or("")
        ),
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn same(name: &str, actual: &[Value], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "{name}: number of results");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(a, e, "{name} #{i}");
    }
}

// ---- pure helpers ------------------------------------------------------------------------------
#[test]
fn golden_versions() {
    let (input, output) = fixture("versions");
    let parsed: Vec<Value> = input["parse"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| match v.as_str().and_then(parse_version) {
            Some((a, b, c)) => json!([a, b, c]),
            None => Value::Null,
        })
        .collect();
    same("parse_version", &parsed, &output["parse"]);
    let newer: Vec<Value> = input["newer"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| json!(is_newer(p[0].as_str().unwrap(), p[1].as_str().unwrap())))
        .collect();
    same("is_newer", &newer, &output["newer"]);
}

#[test]
fn golden_asset_names() {
    let (input, output) = fixture("asset_names");
    let names: Vec<Value> = input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            json!(asset_name(
                c[0].as_str().unwrap(),
                c[1].as_str().unwrap(),
                c[2].as_str().unwrap()
            ))
        })
        .collect();
    same("asset_name", &names, &output["names"]);
}

#[test]
fn golden_parse_sums() {
    let (input, output) = fixture("parse_sums");
    let tables: Vec<Value> = strings(&input["texts"])
        .iter()
        .map(|t| {
            json!(parse_sums(t)
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>())
        })
        .collect();
    same("parse_sums", &tables, &output["tables"]);
}

#[test]
fn golden_team_id() {
    let (input, output) = fixture("team_id");
    let teams: Vec<Value> = strings(&input["texts"])
        .iter()
        .map(|t| json!(team_id(t)))
        .collect();
    same("team_id", &teams, &output["teams"]);
}

#[test]
fn golden_trusted() {
    let (input, output) = fixture("trusted");
    let mut sources: HashMap<String, Source> = HashMap::new();
    let mut actual = Vec::new();
    for case in input["cases"].as_array().unwrap() {
        let api = case["api"].as_str().unwrap();
        let source = sources.entry(api.to_owned()).or_insert_with(|| {
            Source::with_transport("o/r", api, FakeGitHub::new()).expect("source")
        });
        actual.push(json!(source.trusted(case["url"].as_str().unwrap())));
    }
    same("trusted", &actual, &output["trusted"]);
}

// ---- the fake release site ------------------------------------------------------------------------
struct Site {
    gh: Arc<FakeGitHub>,
    key: TestKey,
}

/// Follow redirects through the route table to the body a client would finally read.
fn final_body(routes: &HashMap<String, FakeReply>, start: &str) -> Vec<u8> {
    let mut url = start.to_owned();
    for _ in 0..6 {
        let Some(reply) = routes.get(&url) else {
            return Vec::new();
        };
        match &reply.location {
            Some(location) if (300..400).contains(&reply.status) => {
                url = reqwest::Url::parse(&url)
                    .unwrap()
                    .join(location)
                    .unwrap()
                    .to_string();
            }
            _ => return reply.body.clone(),
        }
    }
    Vec::new()
}

fn route_spec(spec: &Value) -> FakeReply {
    FakeReply {
        status: spec["status"].as_u64().unwrap() as u16,
        location: spec["location"].as_str().map(str::to_owned),
        body: txt(&spec["text"]).into_bytes(),
    }
}

/// The exporter's `Site`, plus the minisign signature the port requires.
fn build_site(case: &Value) -> Site {
    let key = TestKey::new();
    let gh = FakeGitHub::new();
    let mut routes: HashMap<String, FakeReply> = HashMap::new();
    let empty = serde_json::Map::new();
    let files = case["files"].as_object().unwrap_or(&empty);
    let mut release = case.get("release").cloned();
    if let Some(release) = release.as_mut() {
        let has_sums = release["assets"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x["name"] == SUMS_NAME));
        if has_sums {
            release["assets"].as_array_mut().unwrap().push(
                json!({"name": SUMS_SIG_NAME, "size": 100, "browser_download_url": format!("{BASE}{SUMS_SIG_NAME}")}),
            );
        }
        routes.insert(API_LATEST.to_owned(), FakeReply::json(release));
    }
    let sums = match case.get("sums_override").filter(|v| !v.is_null()) {
        Some(text) => txt(text),
        None => files
            .iter()
            .map(|(n, t)| format!("{}  {n}\n", sha(txt(t).as_bytes())))
            .collect(),
    };
    routes.insert(
        format!("{BASE}{SUMS_NAME}"),
        FakeReply::redirect(&format!("{CDN}sums")),
    );
    routes.insert(format!("{CDN}sums"), FakeReply::ok(sums.into_bytes()));
    let tamper = case.get("tamper").map(strings).unwrap_or_default();
    for (name, text) in files {
        let mut data = txt(text).into_bytes();
        if tamper.contains(name) {
            *data.last_mut().unwrap() = b'X';
        }
        routes.insert(
            format!("{BASE}{name}"),
            FakeReply::redirect(&format!("{CDN}{name}")),
        );
        routes.insert(format!("{CDN}{name}"), FakeReply::ok(data));
    }
    for (url, spec) in case
        .get("routes")
        .and_then(Value::as_object)
        .unwrap_or(&empty)
    {
        routes.insert(url.clone(), route_spec(spec));
    }
    let signed = final_body(&routes, &format!("{BASE}{SUMS_NAME}"));
    routes.insert(
        format!("{BASE}{SUMS_SIG_NAME}"),
        FakeReply::redirect(&format!("{CDN}sums.sig")),
    );
    routes.insert(
        format!("{CDN}sums.sig"),
        FakeReply::ok(key.sign(&signed).into_bytes()),
    );
    for (url, reply) in routes {
        gh.route(&url, reply);
    }
    if case.get("offline").and_then(Value::as_bool) == Some(true) {
        gh.go_offline("no network");
    }
    Site { gh, key }
}

impl Site {
    fn source(&self) -> Source {
        Source::with_transport("o/r", "https://api.github.com", self.gh.clone())
            .unwrap()
            .with_public_key(&self.key.public())
    }
}

#[tokio::test]
async fn golden_latest() {
    let (input, output) = fixture("latest");
    let mut actual = Vec::new();
    for case in input["cases"].as_array().unwrap() {
        let site = build_site(case);
        let result = site
            .source()
            .latest(
                case["current"].as_str().unwrap(),
                case["system"].as_str(),
                case["arch"].as_str(),
            )
            .await;
        actual.push(match result {
            Ok(rel) => json!({"release": rel.map(|r| serde_json::to_value(r).unwrap())}),
            Err(e) => json!({"error": e.0}),
        });
    }
    let names: Vec<&str> = input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    let expected = output["results"].as_array().unwrap();
    assert_eq!(actual.len(), expected.len());
    for ((a, e), name) in actual.iter().zip(expected).zip(names) {
        assert_eq!(a, e, "latest case {name}");
    }
}

#[tokio::test]
async fn golden_download() {
    let (input, output) = fixture("download");
    let mut actual = Vec::new();
    for case in input["cases"].as_array().unwrap() {
        let site = build_site(&json!({"routes": case["routes"]}));
        let rel: AppRelease = serde_json::from_value(case["rel"].clone()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join(EXE_NAME);
        let seen = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&seen);
        let progress =
            move |got: u64, total: u64| *sink.lock().unwrap() = Some(json!([got, total]));
        let result = site.source().download(&rel, &dest, Some(&progress)).await;
        actual.push(json!({
            "error": result.err().map(|e| e.0),
            "dest_sha256": dest.exists().then(|| sha(&std::fs::read(&dest).unwrap())),
            "dir": names(dir.path()),
            "last_progress": seen.lock().unwrap().clone(),
            "asked": site.gh.calls(),
        }));
    }
    let names_: Vec<&str> = input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    for ((a, e), name) in actual
        .iter()
        .zip(output["results"].as_array().unwrap())
        .zip(names_)
    {
        // the Python call list includes only the API-free download hops, as does ours
        assert_eq!(a, e, "download case {name}");
    }
}

// ---- check_bundle ----------------------------------------------------------------------------------
fn plist_xml(bundle_id: &str, version: Option<&str>) -> String {
    let version = version.map_or(String::new(), |v| {
        format!("<key>CFBundleShortVersionString</key><string>{v}</string>")
    });
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\
<key>CFBundleIdentifier</key><string>{bundle_id}</string>{version}</dict></plist>\n"
    )
}

fn slashes(text: &str) -> String {
    text.replace('\\', "/")
}

#[test]
fn golden_check_bundle() {
    let (input, output) = fixture("check_bundle");
    let version = input["version"].as_str().unwrap();
    let mut actual = Vec::new();
    for case in input["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let new = dir.path().join("x").join("Clawd.app");
        let running = dir.path().join("Applications").join("Clawd.app");
        std::fs::create_dir_all(new.join("Contents")).unwrap();
        std::fs::create_dir_all(&running).unwrap();
        let plist = new.join("Contents").join("Info.plist");
        match case.get("plist").and_then(Value::as_str).unwrap_or("ok") {
            "ok" => {
                let bundle_version = case
                    .get("bundle_version")
                    .map_or(Some("0.2.0"), Value::as_str);
                let id = case
                    .get("bundle_id")
                    .and_then(Value::as_str)
                    .unwrap_or("vn.clawd.desktop");
                std::fs::write(&plist, plist_xml(id, bundle_version)).unwrap();
            }
            "garbage" => std::fs::write(&plist, b"garbage").unwrap(),
            _ => {}
        }
        let team = |key: &str| -> Option<String> {
            match case.get(key) {
                None => Some("ABCDE12345".to_owned()),
                Some(v) => v.as_str().map(str::to_owned),
            }
        };
        let (new_team, running_team) = (team("new_team"), team("running_team"));
        let verify_rc = case.get("verify_rc").and_then(Value::as_i64).unwrap_or(0) as i32;
        let oserror = case.get("codesign").is_some();
        let running_text = running.to_string_lossy().into_owned();
        let runner = FakeRunner::new(move |_, args| {
            if oserror {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "No such file or directory",
                ));
            }
            if args[0] == "--verify" {
                return Ok(CommandOutput {
                    status: verify_rc,
                    ..output_ok()
                });
            }
            let team = if *args.last().unwrap() == running_text {
                &running_team
            } else {
                &new_team
            };
            Ok(CommandOutput {
                stderr: format!(
                    "Identifier=vn.clawd.desktop\nTeamIdentifier={}\n",
                    team.as_deref().unwrap_or("not set")
                ),
                ..output_ok()
            })
        });
        let running_arg = (case.get("running").and_then(Value::as_str) != Some("absent"))
            .then_some(running.as_path());
        let result = check_bundle(&new, version, running_arg, runner.as_ref());
        let error = result.err().map(|e| {
            if e.0.starts_with("Info.plist:") || e.0.starts_with("codesign:") {
                format!("{}: *", e.0.split(':').next().unwrap())
            } else {
                e.0
            }
        });
        let calls: Vec<Vec<String>> = runner
            .calls()
            .into_iter()
            .map(|(program, args)| {
                std::iter::once(program)
                    .chain(args)
                    .map(|a| {
                        slashes(
                            &a.replace(&*new.to_string_lossy(), "<new>")
                                .replace(&*running.to_string_lossy(), "<running>"),
                        )
                    })
                    .collect()
            })
            .collect();
        actual.push(json!({"error": error, "calls": calls}));
    }
    let names_: Vec<&str> = input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    for ((a, e), name) in actual
        .iter()
        .zip(output["results"].as_array().unwrap())
        .zip(names_)
    {
        assert_eq!(a, e, "check_bundle case {name}");
    }
}

#[test]
fn golden_can_apply() {
    let (input, output) = fixture("can_apply");
    let dir = tempfile::tempdir().unwrap();
    let apps = dir.path().join("Applications");
    std::fs::create_dir_all(&apps).unwrap();
    let mut actual = Vec::new();
    for case in input["cases"].as_array().unwrap() {
        let exe: Option<PathBuf> = case["executable"]
            .as_str()
            .map(|t| PathBuf::from(t.replace("<apps>", &apps.to_string_lossy())));
        let (ok, why) = can_apply(
            case["frozen"].as_bool(),
            case["system"].as_str(),
            exe.as_deref(),
        );
        actual.push(json!([ok, why]));
    }
    same("can_apply", &actual, &output["results"]);
}

#[test]
fn golden_swap_script_is_byte_identical_to_pythons() {
    let (_, output) = fixture("swap_script");
    let text = output["text"].as_str().unwrap();
    assert_eq!(mac_swap_script(), text);
    assert!(!mac_swap_script().contains('\r'), "LF only");
    assert_eq!(
        sha(mac_swap_script().as_bytes()),
        output["sha256"].as_str().unwrap()
    );
    assert_eq!(NO_TEAM, output["no_team"].as_str().unwrap());
}

// ---- Updater flows (Windows) -------------------------------------------------------------------------
#[tokio::test]
async fn golden_updater_flows() {
    let (input, output) = fixture("updater_flows");
    for (case, expected) in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(output["results"].as_array().unwrap())
    {
        let name = case["name"].as_str().unwrap();
        let site = build_site(case);
        let rig = Rig::new();
        let system = case["system"].as_str().unwrap();
        let arch = case["arch"].as_str().unwrap();
        let make = |current: &str, source: Option<Source>| {
            let mut opts = rig.options(current, system, arch, &site.key);
            opts.packaged = Some(case.get("frozen").and_then(Value::as_bool).unwrap_or(true));
            Updater::new(source, opts)
        };
        let mut up = make(case["current"].as_str().unwrap(), Some(site.source()));
        let mut actual = Vec::new();
        for step in case["steps"].as_array().unwrap() {
            if let Some(force) = step.get("check").and_then(Value::as_bool) {
                let before = site.gh.calls().len();
                let state = up.check(force).await;
                let asks = site.gh.calls()[before..]
                    .iter()
                    .filter(|u| u.as_str() == API_LATEST)
                    .count();
                actual.push(json!({"state": state, "api_asks": asks}));
            } else if step.get("state").is_some() {
                actual.push(json!({"state": up.state()}));
            } else if let Some(secs) = step.get("advance").and_then(Value::as_f64) {
                rig.clock.advance(secs);
            } else if let Some(version) = step.get("fail_marker").and_then(Value::as_str) {
                std::fs::write(rig.stage().join("failed"), version).unwrap();
            } else if step.get("restart").is_some() {
                let current = step
                    .get("current")
                    .and_then(Value::as_str)
                    .unwrap_or(case["current"].as_str().unwrap());
                up = make(current, Some(site.source()));
                actual.push(json!({"restart_state": up.state()}));
            }
        }
        let expected = expected["states"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "{name}");
        for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(a, e, "flow {name} step #{i}");
        }
    }
}

// ---- apply(): commands recorded, nothing executed ------------------------------------------------------
fn windows_site() -> Site {
    let payload = "installer".repeat(100);
    build_site(&json!({
        "files": {EXE_NAME: payload},
        "release": {
            "tag_name": "v0.2.0", "draft": false, "prerelease": false, "body": "Sửa lỗi xoay màn hình",
            "html_url": "https://github.com/o/r/releases/tag/v0.2.0",
            "assets": [
                {"name": EXE_NAME, "size": 900, "browser_download_url": format!("{BASE}{EXE_NAME}")},
                {"name": SUMS_NAME, "size": 100, "browser_download_url": format!("{BASE}{SUMS_NAME}")},
            ],
        },
    }))
}

#[tokio::test]
async fn golden_apply() {
    let (_, output) = fixture("apply");
    let results = output["results"].as_array().unwrap();
    let by_name = |n: &str| {
        results
            .iter()
            .find(|r| r["name"] == n)
            .unwrap_or_else(|| panic!("scenario {n}"))
    };

    // Windows: the installer, silent, detached
    let site = windows_site();
    let rig = Rig::new();
    let up = Updater::new(
        Some(site.source()),
        rig.options("0.1.0", "windows", "x86_64", &site.key),
    );
    up.check(false).await;
    let ok = by_name("windows_ok");
    assert_eq!(json!(up.apply(Some(1234))), ok["result"]);
    let started = rig.launcher.started();
    assert_eq!(started.len(), 1);
    assert_eq!(
        started[0].program.file_name().unwrap().to_string_lossy(),
        ok["popen"]["exe"].as_str().unwrap()
    );
    assert_eq!(json!(started[0].args), ok["popen"]["args"]);
    assert_eq!(started[0].kind, LaunchKind::Installer);
    assert_eq!(
        u64::from(WINDOWS_DETACHED_FLAGS),
        ok["popen"]["flags"].as_u64().unwrap()
    );

    set_ota_busy(rig.paths());
    assert_eq!(json!(up.apply(None)), by_name("windows_ota_busy")["result"]);
    assert_eq!(started.len(), rig.launcher.started().len());
    std::fs::remove_file(rig.paths().ota_busy_file()).unwrap();
    std::fs::write(up.staged().unwrap().path, b"swapped after the download").unwrap();
    let changed = by_name("windows_changed_file");
    assert_eq!(json!(up.apply(None)), changed["result"]);
    assert_eq!(
        up.staged().is_some(),
        changed["staged_after"].as_bool().unwrap()
    );
    assert_eq!(
        rig.launcher.started().len(),
        1,
        "a changed installer is never started"
    );

    let rig_empty = Rig::new();
    let empty = Updater::new(
        None,
        rig_empty.options("0.1.0", "windows", "x86_64", &site.key),
    );
    assert_eq!(
        json!(empty.apply(None)),
        by_name("windows_nothing_staged")["result"]
    );
    let rig2 = Rig::new();
    let mut opts = rig2.options("0.1.0", "windows", "x86_64", &site.key);
    opts.packaged = Some(false);
    assert_eq!(
        json!(Updater::new(None, opts).apply(None)),
        by_name("windows_from_source")["result"]
    );

    // macOS: swap.sh
    let m = mac_setup(Mac::default());
    let mac = by_name("mac_ok");
    let state = m.up.check(false).await;
    assert_eq!(state.status, mac["state_status"].as_str().unwrap());
    let root = m.rig.cfg.root().to_path_buf();
    let stage = root.join("stage");
    std::fs::write(stage.join("failed"), "0.1.9").unwrap();
    assert_eq!(json!(m.up.apply(Some(4321))), mac["result"]);
    let shorten = |text: &str| -> String {
        slashes(
            &text
                .replace(&*stage.to_string_lossy(), "<stage>")
                .replace(&*m.running.to_string_lossy(), "<running>")
                .replace(&*root.to_string_lossy(), "<tmp>"),
        )
    };
    let started = m.rig.launcher.started();
    let spec = &started[0];
    let argv: Vec<String> = std::iter::once(spec.program.to_string_lossy().into_owned())
        .chain(spec.args.iter().cloned())
        .map(|a| shorten(&a))
        .collect();
    assert_eq!(json!(argv), mac["popen"]["argv"]);
    assert_eq!(
        spec.kind == LaunchKind::SwapScript,
        mac["popen"]["start_new_session"].as_bool().unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(stage.join("swap.sh")).unwrap() == mac_swap_script(),
        mac["popen"]["script_is_swap"].as_bool().unwrap()
    );
    assert_eq!(
        !stage.join("failed").exists(),
        mac["popen"]["failed_marker_removed"].as_bool().unwrap()
    );
    let calls: Vec<Vec<String>> = m
        .rig
        .runner
        .calls()
        .into_iter()
        .map(|(p, a)| std::iter::once(p).chain(a).map(|x| shorten(&x)).collect())
        .collect();
    assert_eq!(json!(calls), mac["runner_calls"]);
}

// ---- from_settings / auto_enabled -----------------------------------------------------------------------
#[test]
fn golden_settings() {
    let (input, output) = fixture("settings");
    let mut sources = Vec::new();
    for case in input["cases"].as_array().unwrap() {
        let cfg = TempConfig::new();
        if let Some(text) = case["config"].as_str() {
            cfg.write_config("config", text);
        }
        let vars: HashMap<String, String> = case["env"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        sources.push(match from_settings(cfg.paths(), &Env::from_map(vars)) {
            Ok(s) => json!({"repo": s.repo(), "api": s.api()}),
            Err(e) => json!({"error": e.0}),
        });
    }
    let names_: Vec<&str> = input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    for ((a, e), name) in sources
        .iter()
        .zip(output["sources"].as_array().unwrap())
        .zip(names_)
    {
        assert_eq!(a, e, "from_settings case {name}");
    }
    let mut auto = Vec::new();
    for text in strings(&input["auto_configs"]) {
        let cfg = TempConfig::new();
        if !text.is_empty() {
            cfg.write_config("config", &text);
        }
        auto.push(json!(auto_enabled(cfg.paths())));
    }
    same("auto_enabled", &auto, &output["auto"]);
}
