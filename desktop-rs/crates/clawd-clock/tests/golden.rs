//! Golden fixtures of `tools/golden/export_clock.py`: what the real Python modules did, replayed
//! against the Rust crate (`desktop-rs/fixtures/clock/*.json`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use clawd_clock::api::ClockApiOptions;
use clawd_clock::{
    config_diff, manual_clock, stored_clock, valid_host, BootWatch, ClockApi, ClockFinder,
    ClockStore, MdnsSource, OnRestart,
};
use clawd_contracts::{ClockError, ClockSource, ClockTarget, TargetsFn};
use clawd_testkit::clock::{FakeClockDevice, FakeMdns};
use clawd_testkit::{load_fixture_value, FakeClock, FakeResponse, TempConfig};
use serde_json::{json, Value};

fn clock_dict(c: &Option<ClockTarget>) -> Value {
    match c {
        None => Value::Null,
        Some(c) => json!({
            "host": c.host, "port": c.port, "name": c.name, "version": c.version,
            "source": c.source.as_str(), "base": c.base(), "usage_url": c.usage_url(),
            "label": c.label(),
        }),
    }
}

fn target_of(d: &Value) -> ClockTarget {
    let source = match d["source"].as_str().unwrap_or("mdns") {
        "saved" => ClockSource::Saved,
        "manual" => ClockSource::Manual,
        "scan" => ClockSource::Scan,
        _ => ClockSource::Mdns,
    };
    let mut c = ClockTarget::new(d["host"].as_str().unwrap(), source);
    c.port = d["port"].as_u64().unwrap_or(80) as u16;
    c.name = d["name"].as_str().unwrap_or("").to_owned();
    c.version = d["version"].as_str().unwrap_or("").to_owned();
    c
}

#[test]
fn config_diff_matches_python() {
    let fx = load_fixture_value("clock", "config_diff");
    let cases = fx["input"]["cases"].as_array().unwrap();
    let want = fx["output"]["results"].as_array().unwrap();
    for (case, want) in cases.iter().zip(want) {
        let got = config_diff(&case["old"], &case["new"]);
        // to_string: key order is part of the contract (what is POSTed)
        assert_eq!(got.to_string(), want.to_string(), "{}", case["name"]);
    }
}

#[test]
fn manual_clock_and_valid_host_match_python() {
    let fx = load_fixture_value("clock", "manual_clock");
    let raws = fx["input"]["raws"].as_array().unwrap();
    let want = fx["output"]["clocks"].as_array().unwrap();
    assert!(raws.len() > 50);
    for (raw, want) in raws.iter().zip(want) {
        let raw = raw.as_str().unwrap();
        assert_eq!(&clock_dict(&manual_clock(raw)), want, "{raw:?}");
    }
    let hosts = fx["input"]["hosts"].as_array().unwrap();
    let valid = fx["output"]["valid_hosts"].as_array().unwrap();
    for (host, want) in hosts.iter().zip(valid) {
        let host = host.as_str().unwrap();
        assert_eq!(valid_host(host), want.as_bool().unwrap(), "{host:?}");
    }
}

#[test]
fn stored_clock_matches_python() {
    let fx = load_fixture_value("clock", "stored_clock");
    let cases = fx["input"]["cases"].as_array().unwrap();
    let want = fx["output"]["clocks"].as_array().unwrap();
    for (case, want) in cases.iter().zip(want) {
        assert_eq!(&clock_dict(&stored_clock(case)), want, "{case}");
    }
}

#[test]
fn clock_store_text_and_unpin_match_python() {
    let fx = load_fixture_value("clock", "clock_store");
    let tmp = TempConfig::new();
    let saves = fx["input"]["saves"].as_array().unwrap();
    let texts = fx["output"]["save_texts"].as_array().unwrap();
    for (i, (data, text)) in saves.iter().zip(texts).enumerate() {
        let path = tmp.home().join(format!("save{i}")).join("clock.json");
        ClockStore::new(path.clone()).save(data);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            text.as_str().unwrap(),
            "save {i}"
        );
    }
    let unpins = fx["input"]["unpins"].as_array().unwrap();
    let wants = fx["output"]["unpins"].as_array().unwrap();
    for (i, (data, want)) in unpins.iter().zip(wants).enumerate() {
        let path = tmp.home().join(format!("unpin{i}")).join("clock.json");
        let store = ClockStore::new(path.clone());
        if !data.is_null() {
            store.save(data);
        }
        let clock = store.unpin();
        assert_eq!(clock_dict(&clock), want["clock"], "unpin {i}");
        let file = path
            .exists()
            .then(|| std::fs::read_to_string(&path).unwrap());
        assert_eq!(
            file.map_or(Value::Null, Value::String),
            want["file"],
            "unpin {i} file"
        );
    }
}

#[tokio::test]
async fn finder_sequences_match_python() {
    let fx = load_fixture_value("clock", "finder_sequences");
    let scenarios = fx["input"]["scenarios"].as_array().unwrap();
    let runs = fx["output"]["runs"].as_array().unwrap();
    assert!(scenarios.len() >= 20);
    for (sc, run) in scenarios.iter().zip(runs) {
        let name = sc["name"].as_str().unwrap();
        let tmp = TempConfig::new();
        let store = ClockStore::new(tmp.home().join("clock.json"));
        if !sc["saved"].is_null() {
            store.save(&sc["saved"]);
        }
        let now = FakeClock::new(1000.0);
        let clocks = |v: &Value| -> Vec<ClockTarget> {
            v.as_array()
                .map(|a| a.iter().map(target_of).collect())
                .unwrap_or_default()
        };
        let mdns = Arc::new(FakeMdns::new(&clocks(&sc["mdns"])));
        let manual: Vec<String> = sc["manual"]
            .as_array()
            .map(|a| a.iter().map(|u| u.as_str().unwrap().to_owned()).collect())
            .unwrap_or_default();
        let manual_fn: TargetsFn = Arc::new(move || manual.clone());
        let browser: Arc<dyn MdnsSource> = mdns.clone();
        let f = ClockFinder::new(store, manual_fn, browser, now.arc())
            .with_wall_clock(FakeClock::new(1_700_000_000.0).arc());
        let snapshot = |f: &ClockFinder| -> Value {
            let d = f.describe();
            json!({
                "targets": f.targets(),
                "current": clock_dict(&f.current()),
                "describe": {
                    "manual_config": d.manual_config, "clock": clock_dict(&d.clock),
                    "config_targets": d.config_targets, "last_ok_set": d.last_ok.is_some(),
                    "failing": d.failing,
                },
                "searching": d.searching,
                "clock_json": f.store().load(),
            })
        };
        let mut seen = vec![snapshot(&f)];
        for op in sc["steps"].as_array().unwrap() {
            let mut extra = None;
            match op["op"].as_str().unwrap() {
                "step" => f.step().await,
                "report" => f.report(op["url"].as_str().unwrap(), op["ok"].as_bool().unwrap()),
                "set_manual" => {
                    extra = Some(clock_dict(&f.set_manual(op["raw"].as_str().unwrap())));
                }
                "use" => f.use_clock(&target_of(&op["clock"])),
                "retry" => f.retry(),
                "advance" => now.advance(op["secs"].as_f64().unwrap()),
                "mdns" => mdns.set(&clocks(&op["clocks"])),
                other => panic!("unknown op {other}"),
            }
            let mut snap = snapshot(&f);
            if let Some(returned) = extra {
                snap["returned"] = returned;
            }
            seen.push(snap);
        }
        let want = run.as_array().unwrap();
        assert_eq!(seen.len(), want.len(), "{name}");
        for (i, (got, want)) in seen.iter().zip(want).enumerate() {
            assert_eq!(
                got,
                want,
                "{name}: after op {i}: {}",
                sc["steps"].get(i.wrapping_sub(1)).unwrap_or(&Value::Null)
            );
        }
    }
}

async fn call(api: &ClockApi, name: &str, args: &[Value]) -> Result<Value, ClockError> {
    let n = |i: usize| args[i].as_u64().unwrap() as usize;
    match name {
        "heap" => api.heap().await,
        "config" => api.config().await,
        "status" => api.status().await,
        "export" => api.export().await,
        "style" => api.style().await,
        "clear_style" => api.clear_style().await,
        "scan" => api.scan().await.map(Value::Array),
        "save" => api.save(&args[0]).await,
        "reboot" => api.reboot().await.map(|()| Value::Null),
        "upload_firmware" => api
            .upload_firmware(&vec![b'x'; n(0)])
            .await
            .map(|()| Value::Null),
        "upload_pack" => {
            api.upload_pack(args[0].as_str().unwrap(), &vec![b'x'; n(1)])
                .await
        }
        "install_style" => api.install_style(&vec![b'x'; n(0)]).await,
        "get_file" => api
            .get_file(args[0].as_str().unwrap(), n(1))
            .await
            .map(|b| json!({"bytes": b.len()})),
        other => panic!("unknown call {other}"),
    }
}

#[tokio::test]
async fn api_results_and_messages_match_python() {
    let fx = load_fixture_value("clock", "api_messages");
    let cases = fx["input"]["cases"].as_array().unwrap();
    let results = fx["output"]["results"].as_array().unwrap();
    assert!(cases.len() >= 50);
    for (case, want) in cases.iter().zip(results) {
        let name = case["name"].as_str().unwrap();
        let device = FakeClockDevice::start(None);
        if let Some(path) = case["path"].as_str() {
            let r = &case["response"];
            let status = r["status"].as_u64().unwrap_or(200) as u16;
            let response = match r.get("json") {
                Some(body) => FakeResponse::json(status, body),
                None => FakeResponse::text(status, r["text"].as_str().unwrap()),
            };
            device.override_path(path, response);
        }
        let api = ClockApi::with_options(&device.url(), None, ClockApiOptions::default());
        // The testkit server drops a connection when the machine stalls it past its 5 s read
        // timeout; that is the only transient error, and a real failure repeats every time.
        let mut got = Value::Null;
        for _ in 0..3 {
            got = match call(
                &api,
                case["call"].as_str().unwrap(),
                case["args"].as_array().unwrap(),
            )
            .await
            {
                Ok(v) => json!({"ok": v}),
                Err(ClockError::AuthNeeded) => {
                    json!({"error": "AuthNeeded", "text": ClockError::AuthNeeded.to_string()})
                }
                Err(e) => json!({"error": "ClockError", "text": e.to_string()}),
            };
            let transient = got["text"]
                .as_str()
                .is_some_and(|t| t.starts_with("RequestError:") && t.contains("connection closed"));
            if !transient {
                break;
            }
        }
        assert_eq!(&got, want, "{name}");
    }
}

#[tokio::test]
async fn bootwatch_steps_match_python() {
    let fx = load_fixture_value("clock", "bootwatch_steps");
    let scenarios = fx["input"]["scenarios"].as_array().unwrap();
    let runs = fx["output"]["runs"].as_array().unwrap();
    for (sc, run) in scenarios.iter().zip(runs) {
        let name = sc["name"].as_str().unwrap();
        let device = FakeClockDevice::start(None);
        let url = device.usage_url();
        let now = FakeClock::new(1000.0);
        let hits: Arc<Mutex<Vec<u64>>> = Arc::default();
        let delivered = Arc::new(AtomicBool::new(true));
        let on_restart: OnRestart = {
            let hits = Arc::clone(&hits);
            let delivered = Arc::clone(&delivered);
            Arc::new(move |_, up| {
                hits.lock().unwrap().push(up);
                let ok = delivered.load(Ordering::SeqCst);
                Box::pin(async move { ok })
            })
        };
        let listed = vec![url.clone()];
        let targets: TargetsFn = Arc::new(move || listed.clone());
        let mut watch = BootWatch::new(targets, on_restart, now.arc());
        let want = run.as_array().unwrap();
        for (i, (step, want)) in sc["steps"].as_array().unwrap().iter().zip(want).enumerate() {
            now.advance(step["dt"].as_f64().unwrap());
            device.set_uptime(step["uptime"].as_u64());
            delivered.store(step["delivered"].as_bool().unwrap(), Ordering::SeqCst);
            let restarted = watch.step().await == [url.clone()];
            let got = json!({"restarted": restarted, "hits": hits.lock().unwrap().clone()});
            assert_eq!(&got, want, "{name}: step {i}");
        }
    }
}
