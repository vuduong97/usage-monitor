//! Port of `tests/test_discovery.py`: mDNS, the remembered clock, a typed address; never a
//! network scan. mDNS is the testkit's `FakeMdns`; time is a hand-driven `FakeClock`.

use std::sync::Arc;
use std::time::Duration;

use clawd_clock::finder::{BAD_HOLD, FAIL_LIMIT};
use clawd_clock::{ClockFinder, ClockStore, MdnsSource};
use clawd_contracts::{new_stop, ClockSource, ClockTarget, TargetsFn};
use clawd_testkit::clock::FakeMdns;
use clawd_testkit::{FakeClock, TempConfig};
use serde_json::{json, Value};

struct Rig {
    _tmp: TempConfig,
    finder: ClockFinder,
    mdns: Arc<FakeMdns>,
    now: FakeClock,
}

impl Rig {
    fn store(&self) -> &ClockStore {
        self.finder.store()
    }

    fn load(&self) -> Value {
        self.store().load()
    }
}

fn a() -> ClockTarget {
    let mut c = ClockTarget::new("10.0.0.7", ClockSource::Mdns);
    c.name = "clawd".into();
    c.version = "2.16.0".into();
    c
}

fn named(host: &str, name: &str) -> ClockTarget {
    let mut c = ClockTarget::new(host, ClockSource::Mdns);
    c.name = name.into();
    c
}

fn make(clocks: &[ClockTarget], manual: &[&str], saved: Option<Value>) -> Rig {
    let tmp = TempConfig::new();
    let store = ClockStore::new(tmp.home().join("clock.json"));
    if let Some(saved) = saved {
        store.save(&saved);
    }
    let now = FakeClock::new(1000.0);
    let mdns = Arc::new(FakeMdns::new(clocks));
    let manual: Vec<String> = manual.iter().map(|s| (*s).to_owned()).collect();
    let manual_fn: TargetsFn = Arc::new(move || manual.clone());
    let browser: Arc<dyn MdnsSource> = mdns.clone();
    let finder = ClockFinder::new(store, manual_fn, browser, now.arc())
        .with_wall_clock(FakeClock::new(1_700_000_000.0).arc());
    Rig {
        _tmp: tmp,
        finder,
        mdns,
        now,
    }
}

fn drop_clock(f: &ClockFinder, url: &str) {
    for _ in 0..FAIL_LIMIT {
        f.report(url, false);
    }
}

#[test]
fn there_is_no_port_scan_anywhere() {
    for file in ["finder.rs", "store.rs", "clock.rs", "mdns.rs"] {
        let path = format!("{}/src/{file}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(&path).unwrap();
        for gone in ["scan_lan", "TcpStream", "ip_network", "\"subnets\"", "reqwest"] {
            assert!(!text.contains(gone), "{file} mentions {gone}");
        }
    }
}

#[tokio::test]
async fn first_mdns_clock_is_used_and_remembered() {
    let r = make(&[a()], &[], None);
    assert!(r.finder.targets().is_empty());
    assert!(r.finder.describe().searching);
    r.finder.step().await;
    assert_eq!(r.finder.targets(), ["http://10.0.0.7/api/usage"]);
    assert_eq!(
        r.load(),
        json!({"name": "clawd", "host": "10.0.0.7", "port": 80, "version": "2.16.0"})
    );
    assert!(!r.finder.describe().searching);
}

#[tokio::test]
async fn saved_clock_is_tried_at_once_then_follows_its_new_address() {
    let r = make(&[], &[], Some(json!({"name": "clawd", "host": "192.168.1.48", "port": 80})));
    assert_eq!(r.finder.targets(), ["http://192.168.1.48/api/usage"]); // before mDNS answers
    r.mdns.set(&[named("192.168.1.60", "clawd")]); // the router gave it a new IP
    r.finder.step().await;
    assert_eq!(r.finder.targets(), ["http://192.168.1.60/api/usage"]);
    assert_eq!(r.load()["host"], "192.168.1.60");
}

#[tokio::test]
async fn remembered_clock_wins_over_another_one() {
    let other = named("10.0.0.9", "aaa-neighbour");
    let r = make(&[other, a()], &[], Some(json!({"name": "clawd", "host": "1.2.3.4"})));
    r.finder.step().await;
    assert_eq!(r.finder.targets(), ["http://10.0.0.7/api/usage"]);
}

#[tokio::test]
async fn config_http_targets_bypass_discovery() {
    let r = make(&[a()], &["http://192.168.0.5/api/usage"], None);
    r.finder.step().await;
    assert_eq!(r.finder.targets(), ["http://192.168.0.5/api/usage"]);
    assert_eq!(r.load(), json!({}));
    let d = r.finder.describe();
    assert!(d.manual_config && d.clock.is_none() && !d.searching);
    assert_eq!(d.config_targets, ["http://192.168.0.5/api/usage"]);
    assert!(r.mdns.resolve_calls().is_empty(), "mDNS is not even asked");
}

#[tokio::test]
async fn a_silent_clock_is_only_ever_retried_at_its_own_address() {
    let r = make(&[], &[], Some(json!({"host": "10.97.40.207"})));
    let url = "http://10.97.40.207/api/usage";
    for _ in 0..FAIL_LIMIT - 1 {
        r.finder.report(url, false);
    }
    assert_eq!(r.finder.targets(), [url]);
    r.finder.report(url, false);
    assert!(r.finder.targets().is_empty(), "stop pushing to a screen that is not there");
    for _ in 0..10 {
        // a long outage: only that one address, now and then
        r.now.advance(BAD_HOLD / 2.0);
        r.finder.step().await;
        let t = r.finder.targets();
        assert!(t.is_empty() || t == [url]);
        if !t.is_empty() {
            drop_clock(&r.finder, url);
        }
    }
    assert_eq!(r.load()["host"], "10.97.40.207");
    r.now.advance(BAD_HOLD + 1.0);
    r.finder.step().await;
    assert_eq!(r.finder.targets(), [url]);
}

#[tokio::test]
async fn scanless_status_has_no_scan_fields() {
    let r = make(&[a()], &[], None);
    let d = r.finder.describe();
    assert!(d.clock.is_none() && d.failing);
    r.finder.step().await;
    r.finder.report(&a().usage_url(), true);
    let d = r.finder.describe();
    assert_eq!(d.clock, Some(a()));
    assert!(!d.failing);
    assert_eq!(d.last_ok, Some(1_700_000_000.0));
}

#[tokio::test]
async fn typed_address_wins_over_mdns_and_failures_and_restarts() {
    let r = make(&[a()], &[], None);
    assert!(r.finder.set_manual("not a host:99999").is_none());
    let c = r.finder.set_manual(" 192.168.1.80 ").unwrap();
    assert_eq!(c.source, ClockSource::Manual);
    assert_eq!(r.finder.targets(), ["http://192.168.1.80/api/usage"]);
    r.finder.step().await;
    assert_eq!(r.finder.targets(), ["http://192.168.1.80/api/usage"]); // mDNS does not override it
    drop_clock(&r.finder, "http://192.168.1.80/api/usage");
    drop_clock(&r.finder, "http://192.168.1.80/api/usage");
    assert_eq!(r.finder.targets(), ["http://192.168.1.80/api/usage"]); // nor do failures
    assert_eq!(r.load()["manual_host"], "192.168.1.80");
    let again = ClockStore::new(r.store().path.clone());
    let browser: Arc<dyn MdnsSource> = Arc::new(FakeMdns::new(&[a()]));
    let f2 = ClockFinder::new(again, Arc::new(Vec::new), browser, r.now.arc());
    assert_eq!(f2.targets(), ["http://192.168.1.80/api/usage"]); // a new app start keeps it
}

#[tokio::test]
async fn retry_keeps_the_address_and_tries_it_at_once() {
    let r = make(&[], &[], Some(json!({"name": "clawd", "host": "10.97.40.207", "port": 80})));
    let url = "http://10.97.40.207/api/usage";
    drop_clock(&r.finder, url);
    r.finder.step().await;
    assert!(r.finder.targets().is_empty(), "on hold for BAD_HOLD");
    r.finder.retry(); // "Tìm lại"
    assert_eq!(r.finder.targets(), [url], "no wait, and nothing forgotten");
    assert_eq!(r.load()["host"], "10.97.40.207");
    assert_eq!(r.load()["name"], "clawd");
    r.now.advance(1.0);
    r.finder.step().await;
    assert_eq!(r.finder.targets(), [url]);
}

#[tokio::test]
async fn retry_unpins_a_typed_address_so_mdns_may_replace_it_later() {
    let r = make(&[a()], &[], None);
    r.finder.set_manual("10.97.40.207");
    r.finder.retry();
    assert_eq!(
        r.load(),
        json!({"name": "", "host": "10.97.40.207", "port": 80, "version": ""})
    );
    assert_eq!(r.finder.current().unwrap().source, ClockSource::Saved);
    r.finder.step().await;
    assert_eq!(r.finder.targets(), ["http://10.97.40.207/api/usage"]); // still tried first
    drop_clock(&r.finder, "http://10.97.40.207/api/usage"); // but it is gone (another network)
    r.finder.step().await;
    assert_eq!(r.finder.targets(), ["http://10.0.0.7/api/usage"]); // mDNS takes over
    assert_eq!(r.load()["host"], "10.0.0.7");
}

#[tokio::test]
async fn retry_with_nothing_saved_only_listens_to_mdns() {
    let r = make(&[], &[], None);
    r.finder.retry();
    assert!(r.finder.targets().is_empty());
    assert_eq!(r.load(), json!({}));
    r.mdns.set(&[a()]);
    r.finder.step().await;
    assert_eq!(r.finder.targets(), ["http://10.0.0.7/api/usage"]);
}

#[tokio::test]
async fn old_scan_keys_are_dropped_from_clock_json() {
    let saved = json!({"name": "", "host": "10.97.40.207", "port": 80, "version": "2.16.0",
                       "subnets": ["10.97.40.0/24"]});
    let r = make(&[], &[], Some(saved));
    r.finder.report("http://10.97.40.207/api/usage", true);
    assert_eq!(
        r.load(),
        json!({"name": "", "host": "10.97.40.207", "port": 80, "version": "2.16.0"})
    );
}

// ---- beyond the Python tests ----------------------------------------------------------

#[tokio::test]
async fn use_clock_remembers_by_name_without_pinning() {
    let r = make(&[], &[], Some(json!({"manual_host": "10.0.0.1"})));
    r.finder.use_clock(&a());
    assert_eq!(
        r.load(),
        json!({"name": "clawd", "host": "10.0.0.7", "port": 80, "version": "2.16.0"})
    );
    assert_eq!(r.finder.targets(), ["http://10.0.0.7/api/usage"]);
}

#[tokio::test]
async fn an_unnamed_saved_address_takes_its_mdns_name() {
    let r = make(&[a()], &[], Some(json!({"host": "10.0.0.7", "port": 80})));
    r.finder.step().await;
    assert_eq!(r.finder.current(), Some(a()));
    assert_eq!(r.load()["name"], "clawd");
}

#[tokio::test]
async fn mdns_is_resolved_again_only_when_something_changed() {
    let r = make(&[a()], &[], None);
    r.finder.step().await;
    r.finder.step().await;
    assert_eq!(r.mdns.resolve_calls().len(), 1);
    r.mdns.set(&[a(), named("10.0.0.9", "zzz")]);
    r.finder.step().await;
    assert_eq!(r.mdns.resolve_calls().len(), 3);
}

#[tokio::test]
async fn a_successful_report_for_another_url_only_stamps_last_ok() {
    let r = make(&[a()], &[], None);
    r.finder.step().await;
    r.finder.report("http://10.9.9.9/api/usage", false);
    assert!(r.finder.describe().last_ok.is_none());
    r.finder.report("http://10.9.9.9/api/usage", true);
    assert!(r.finder.describe().last_ok.is_some());
    assert!(!r.finder.describe().failing);
}

#[tokio::test]
async fn run_starts_the_browser_steps_and_stops_it() {
    let r = make(&[a()], &[], None);
    let mdns = Arc::clone(&r.mdns);
    let finder = Arc::new(r.finder);
    let (stop_tx, stop_rx) = new_stop();
    let task = tokio::spawn({
        let finder = Arc::clone(&finder);
        async move { finder.run(stop_rx).await }
    });
    for _ in 0..100 {
        if !finder.targets().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(finder.targets(), ["http://10.0.0.7/api/usage"]);
    stop_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap();
    assert_eq!((mdns.start_count(), mdns.stop_count()), (1, 1));
}

#[tokio::test]
async fn run_without_mdns_still_serves_the_remembered_address() {
    let r = make(&[], &[], Some(json!({"host": "10.0.0.6"})));
    r.mdns.refuse_start();
    let finder = Arc::new(r.finder);
    let (stop_tx, stop_rx) = new_stop();
    let task = tokio::spawn({
        let finder = Arc::clone(&finder);
        async move { finder.run(stop_rx).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(finder.targets(), ["http://10.0.0.6/api/usage"]);
    drop(stop_tx); // a dropped sender also ends the loop
    tokio::time::timeout(Duration::from_secs(5), task).await.unwrap().unwrap();
}

#[test]
fn clock_store_survives_garbage_and_writes_python_text() {
    let tmp = TempConfig::new();
    let path = tmp.home().join("deep").join("clock.json");
    let store = ClockStore::new(path.clone());
    assert_eq!(store.load(), json!({}));
    assert!(store.unpin().is_none());
    store.save(&json!({"name": "Đ", "host": "10.0.0.6", "port": 80, "version": ""}));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{\n  \"name\": \"\\u0110\",\n  \"host\": \"10.0.0.6\",\n  \"port\": 80,\n  \"version\": \"\"\n}"
    );
    assert!(!path.with_extension("tmp").exists(), "the temp file was renamed away");
    std::fs::write(&path, "[1, 2]").unwrap();
    assert_eq!(store.load(), json!({}), "not an object");
    std::fs::write(&path, "{nope").unwrap();
    assert_eq!(store.load(), json!({}), "not JSON");
}
