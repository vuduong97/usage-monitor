//! `tests/test_settings_api.py` over the real `ClockApi`, a fake clock on loopback and the real
//! feed client against `FakeFeed`: the settings commands (`Bridge`, the commands minus the Tauri
//! context). Test names are the Python names without `test_`.

mod settings_support;

use std::time::{Duration, Instant};

use clawd_app::commands::clock::{factory_reset_on, login_on, save_on, scan_on, set_clock_on};
use clawd_app::commands::firmware::{check_update_on, update_on};
use clawd_app::commands::jobs::job_on;
use clawd_app::commands::session::{diff, Bridge};
use clawd_app::commands::PY_API_NAMES;
use clawd_app::state::ShellCmd;
use clawd_clock::ClockStore;
use clawd_firmware::crc32::crc32_hex;
use clawd_firmware::feed::Feed;
use clawd_store::prefs::clock_screen;
use clawd_testkit::fw::{desk_pak, release_files, FakeFeed, TOKEN};
use clawd_testkit::{FakeHttpServer, FakeResponse, TempConfig};
use serde_json::{json, Value};
use settings_support::*;

fn saved_clock(rig: &Rig, value: Value) {
    ClockStore::new(rig.tc.paths().clock_file()).save(&value);
}

async fn wait_job(b: &Bridge) -> Value {
    assert!(
        wait_for(40, || !b.job_running()).await,
        "the update job did not end"
    );
    job_on(b, &[]).await.unwrap()
}

/// The page asks `fw` again while `feedBusy`: wait for the background feed check.
async fn feed_settled(b: &Bridge) -> Value {
    assert!(
        wait_for(20, || b.fw_info()["feedBusy"] == json!(false)).await,
        "the feed check did not end"
    );
    b.fw_info()
}

fn bundle_in(tc: &TempConfig, seq: u64) -> clawd_firmware::firmware::Bundle {
    make_bundle(&tc.root().join("bundle"), seq, 1, &["synth", "paper"])
}

fn feed_for(tc: &TempConfig, ff: &FakeFeed) -> Feed {
    Feed::new(
        &ff.url(),
        TOKEN,
        "stable",
        Some(tc.paths().feed_cache_dir()),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn state_and_a_one_post_save() {
    let rig = rig();
    let b = &rig.bridge;
    let s = b.state().await;
    assert_eq!(s["ok"], true);
    assert_eq!(
        s["fw"]["styles"],
        json!({"state": "unavailable", "active": "classic", "packs": []})
    );
    assert_eq!(s["app"]["clock"], rig.clock.url());
    assert_eq!(diff(b, &rig.edited(json!({}))).unwrap(), json!({}));
    let r = save_on(
        b,
        &[rig.edited(json!({"brightness": 60, "auth": {"enabled": true}}))],
    )
    .await
    .unwrap();
    assert_eq!(r["ok"], true);
    assert_eq!(r["sent"], json!({"brightness": 60}));
    assert_eq!(
        (r["reboot"].clone(), r["styleSent"].clone()),
        (json!(false), json!(false))
    );
    assert_eq!(rig.clock.saved(), vec![json!({"brightness": 60})]);
    let again = save_on(b, &[rig.edited(json!({"brightness": 60}))])
        .await
        .unwrap();
    assert_eq!(again["sent"], json!({}), "nothing left to send");
}

#[tokio::test(flavor = "multi_thread")]
async fn wifi_change_sends_the_whole_list_and_reboots() {
    let rig = rig();
    rig.bridge.state().await;
    let wifi = json!([{"ssid": "Phone"}, {"ssid": "Home"}, {"ssid": "Cafe", "pass": "12345678"}]);
    let r = save_on(&rig.bridge, &[rig.edited(json!({"wifi": wifi.clone()}))])
        .await
        .unwrap();
    assert_eq!(r["reboot"], true);
    assert_eq!(rig.clock.saved().last().unwrap(), &json!({"wifi": wifi}));
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_input_is_refused_before_the_clock() {
    let rig = rig();
    rig.bridge.state().await;
    let r = save_on(&rig.bridge, &[rig.edited(json!({"brightness": 500}))])
        .await
        .unwrap();
    assert_eq!(r["code"], "bad");
    assert!(rig.clock.saved().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_style_the_clock_cannot_draw_is_not_sent() {
    let tc = TempConfig::new();
    let bundle = bundle_in(&tc, 2);
    let rig = Build::new()
        .temp(tc)
        .heap(new_heap())
        .bundle(bundle)
        .build();
    let s = rig.bridge.state().await;
    assert_eq!(s["fw"]["styles"]["state"], "update");
    let ids: Vec<&str> = s["fw"]["styles"]["packs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["synth", "paper"]);
    let r = save_on(&rig.bridge, &[rig.edited(json!({})), json!("synth")])
        .await
        .unwrap();
    assert_eq!(r["code"], "bad");
    assert!(rig.clock.styles().is_empty());
    let classic = save_on(&rig.bridge, &[rig.edited(json!({})), json!("classic")])
        .await
        .unwrap();
    assert_eq!(classic["ok"], true);
    assert!(
        rig.clock.styles().is_empty(),
        "already classic: nothing to send"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn classic_is_restored_without_a_bundle_or_feed() {
    // the clock runs a pack, but this build carries none (no bundle, no feed): going back to
    // the classic screen only deletes the clock's pack, so it must not be refused
    let rig = Build::new()
        .heap(with(styled_heap(), json!({"style": "synth"})))
        .build();
    let s = rig.bridge.state().await;
    assert_eq!(
        s["fw"]["styles"],
        json!({"state": "unavailable", "active": "synth", "packs": []})
    );
    let nope = save_on(&rig.bridge, &[rig.edited(json!({})), json!("nope")])
        .await
        .unwrap();
    assert_eq!(nope["code"], "bad");
    assert!(rig.clock.styles().is_empty());
    let r = save_on(&rig.bridge, &[rig.edited(json!({})), json!("classic")])
        .await
        .unwrap();
    assert_eq!(
        (r["ok"].clone(), r["styleSent"].clone()),
        (json!(true), json!(true))
    );
    assert_eq!(rig.clock.styles(), vec![Vec::<u8>::new()]);
    assert_eq!(r["fw"]["styles"]["active"], "classic");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_style_is_sent_first_then_the_settings() {
    let tc = TempConfig::new();
    let bundle = bundle_in(&tc, 2);
    let synth = bundle.styles[0].read().unwrap();
    let rig = Build::new()
        .temp(tc)
        .heap(styled_heap())
        .bundle(bundle)
        .build();
    assert_eq!(rig.bridge.state().await["fw"]["styles"]["state"], "ready");
    // reboots: the pack is kept on the clock
    let r = save_on(
        &rig.bridge,
        &[
            rig.edited(json!({"wifi": [{"ssid": "Home"}]})),
            json!("synth"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(r["ok"], true);
    assert_eq!(
        (r["styleSent"].clone(), r["reboot"].clone()),
        (json!(true), json!(true))
    );
    assert_eq!(r["fw"]["styles"]["active"], "synth");
    assert_eq!(rig.clock.styles(), vec![synth]);
    let same = save_on(&rig.bridge, &[rig.edited(json!({})), json!("synth")])
        .await
        .unwrap();
    assert_eq!(same["styleSent"], false, "it shows that one already");
    let back = save_on(&rig.bridge, &[rig.edited(json!({})), json!("classic")])
        .await
        .unwrap();
    assert_eq!(back["ok"], true);
    assert!(rig.clock.styles().last().unwrap().is_empty());
    for bad in [json!("nope"), json!(3), json!(["synth"])] {
        let r = save_on(&rig.bridge, &[rig.edited(json!({})), bad])
            .await
            .unwrap();
        assert_eq!(r["code"], "bad");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_damaged_bundled_pack_is_never_sent() {
    let tc = TempConfig::new();
    let bundle = bundle_in(&tc, 2);
    let paper = bundle.styles[1].path.clone();
    let rig = Build::new()
        .temp(tc)
        .heap(styled_heap())
        .bundle(bundle)
        .build();
    rig.bridge.state().await;
    let mut bytes = std::fs::read(&paper).unwrap();
    *bytes.last_mut().unwrap() = b'!';
    std::fs::write(&paper, bytes).unwrap();
    let r = save_on(&rig.bridge, &[rig.edited(json!({})), json!("paper")])
        .await
        .unwrap();
    assert_eq!(r["code"], "error");
    assert!(rig.clock.styles().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn login_keeps_the_password_in_the_keychain() {
    let rig = Build::new().password("s3cret").build();
    assert_eq!(rig.bridge.state().await["code"], "auth");
    let wrong = login_on(&rig.bridge, &[json!("wrong")]).await.unwrap();
    assert_eq!(wrong["code"], "auth");
    assert!(rig.keys.is_empty());
    let r = login_on(&rig.bridge, &[json!("s3cret")]).await.unwrap();
    assert_eq!(
        (r["ok"].clone(), r["saved"].clone()),
        (json!(true), json!(true))
    );
    let service = rig.tc.os_names().keyring_service.clone();
    assert_eq!(
        rig.keys.secret(&service, "clawd").as_deref(),
        Some("admin\ns3cret")
    );
    assert!(
        !r.to_string().contains("s3cret"),
        "never handed back to the page"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_and_noclock() {
    // a clock that does not answer at all: the connection is refused
    let gone = format!("http://127.0.0.1:{}", dead_port());
    let rig = Build::new().base(&gone, "clawd").build();
    assert_eq!(rig.bridge.state().await["code"], "offline");
    let none = Build::new().base("", "").build();
    let s = none.bridge.state().await;
    assert_eq!(s["code"], "noclock");
    assert!(s["error"].as_str().unwrap().contains("Nhập IP"));
    assert_eq!(none.clock.requests(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn state_names_the_network_in_use_and_none_when_it_cannot() {
    let rig = rig();
    assert_eq!(
        rig.bridge.state().await["net"],
        json!({"ssid": "Home", "rssi": -58})
    );
    rig.clock
        .set_status(json!({"mode": "ap", "connected": false, "ssid": "", "rssi": 0}));
    assert!(rig.bridge.state().await["net"].is_null());
    rig.clock.status_down(true);
    let s = rig.bridge.state().await;
    assert_eq!(s["ok"], true, "decoration only: the page still loads");
    assert!(s["net"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_keeps_the_strongest_of_each_name() {
    let rig = rig();
    let r = scan_on(&rig.bridge, &[]).await.unwrap();
    assert_eq!(
        r["networks"],
        json!([{"ssid": "A", "rssi": -50, "enc": true}, {"ssid": "B", "rssi": -60, "enc": false}])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn factory_reset_needs_the_word() {
    let rig = rig();
    let r = factory_reset_on(&rig.bridge, &[json!("yes")])
        .await
        .unwrap();
    assert_eq!(r["code"], "bad");
    assert_eq!(rig.clock.requests(), 0, "the clock was not asked");
    assert_eq!(
        factory_reset_on(&rig.bridge, &[json!("XOA")])
            .await
            .unwrap()["ok"],
        true
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn update_job_installs_then_saves_and_shows_the_style() {
    let tc = TempConfig::new();
    let bundle = bundle_in(&tc, 2);
    let paper = bundle.styles[1].read().unwrap();
    let rig = Build::new()
        .temp(tc)
        .heap(old_heap())
        .bundle(bundle)
        .build();
    rig.clock
        .set_after(with(styled_heap(), json!({"uptime": 0})));
    let s = rig.bridge.state().await;
    assert_eq!(s["fw"]["update"], "available");
    assert_eq!(s["fw"]["styles"]["state"], "update");
    let bad = update_on(
        &rig.bridge,
        &[rig.edited(json!({"brightness": 30})), json!(3)],
    )
    .await
    .unwrap();
    assert_eq!(bad["code"], "bad");
    let started = update_on(
        &rig.bridge,
        &[rig.edited(json!({"brightness": 30})), json!("paper")],
    )
    .await
    .unwrap();
    assert_eq!(started["ok"], true);
    let j = wait_job(&rig.bridge).await;
    assert!(
        !rig.tc.paths().pause_file().exists(),
        "the lease was let go"
    );
    assert!(!clawd_store::paths::ota_busy(rig.tc.paths(), None));
    assert_eq!(
        (j["done"].clone(), j["error"].clone()),
        (json!(true), json!("")),
        "{j}"
    );
    assert_eq!(rig.clock.uploads(), 1);
    assert_eq!(rig.clock.saved(), vec![json!({"brightness": 30})]);
    assert_eq!(rig.clock.styles(), vec![paper]);
    let st = rig.bridge.state().await;
    assert_eq!(st["fw"]["update"], "current");
    assert_eq!(
        st["fw"]["styles"],
        json!({"state": "ready", "active": "paper", "packs": [
            {"id": "synth", "name": "Synth", "light": false},
            {"id": "paper", "name": "Paper", "light": true}]})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn update_job_fails_when_the_new_image_draws_no_styles() {
    let tc = TempConfig::new();
    let bundle = bundle_in(&tc, 2);
    let rig = Build::new()
        .temp(tc)
        .heap(old_heap())
        .bundle(bundle)
        .build();
    rig.clock
        .set_after(with(new_heap(), json!({"fwSeq": 2, "uptime": 0})));
    rig.bridge.state().await;
    assert_eq!(
        update_on(&rig.bridge, &[Value::Null, json!("synth")])
            .await
            .unwrap()["ok"],
        true
    );
    let j = wait_job(&rig.bridge).await;
    assert!(!j["error"].as_str().unwrap_or_default().is_empty(), "{j}");
    assert!(rig.clock.styles().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_asks_only_the_saved_address_then_says_to_type_the_ip() {
    let mut rig = rig();
    // the saved clock is a host that does not answer: it says 500 on /api/heap
    let saved = FakeHttpServer::start().unwrap();
    saved.route("GET", "/api/heap", FakeResponse::status(500));
    saved_clock(
        &rig,
        json!({"name": "clawd", "host": "127.0.0.1", "port": saved.addr().port()}),
    );
    let r = set_clock_on(&rig.bridge, &[json!("")]).await.unwrap();
    assert_eq!(
        rig.told(),
        vec![ShellCmd::SetClock {
            host: String::new()
        }]
    );
    let asked = saved.requests();
    assert!(
        !asked.is_empty() && asked.iter().all(|q| q.path() == "/api/heap"),
        "one host, nothing else"
    );
    assert_eq!(r["code"], "offline");
    let error = r["error"].as_str().unwrap();
    assert!(
        error.contains("127.0.0.1") && error.contains("Nhập IP"),
        "{error}"
    );
    assert_eq!(
        rig.bridge.address().0,
        rig.clock.url(),
        "no reconnect to a dead address"
    );
    assert_eq!(rig.clock.requests(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_connects_when_the_saved_clock_answers() {
    let rig = rig();
    let other = FakeClock::start(new_heap(), None);
    saved_clock(
        &rig,
        json!({"name": "clawd", "host": "127.0.0.1", "port": other.port()}),
    );
    let r = set_clock_on(&rig.bridge, &[json!("")]).await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(rig.bridge.address().0, other.url());
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_with_no_address_says_to_type_one() {
    let rig = rig();
    let r = set_clock_on(&rig.bridge, &[json!("")]).await.unwrap();
    assert_eq!(r["code"], "offline");
    let error = r["error"].as_str().unwrap();
    assert!(
        error.contains("Nhập IP") && error.contains("Chưa có địa chỉ"),
        "{error}"
    );
    assert_eq!(rig.clock.requests(), 0, "nothing to ask");
}

#[tokio::test(flavor = "multi_thread")]
async fn private_state_is_not_exposed_to_the_page() {
    // only the 32 commands of the page exist; the Python object's `preview`, `busy`,
    // `set_hooks` and `close_mdns` are not reachable from it
    let mut names: Vec<&str> = PY_API_NAMES.to_vec();
    names.sort_unstable();
    let mut want = vec![
        "state",
        "login",
        "save",
        "scan",
        "reboot",
        "factory_reset",
        "export_backup",
        "import_backup",
        "update",
        "job",
        "set_autostart",
        "open_log",
        "open_web",
        "set_clock",
        "usage_preview",
        "clawd_pak",
        "check_update",
        "fw",
        "app_update",
        "app_update_check",
        "app_update_apply",
        "app_update_page",
        "agents",
        "set_agent",
        "set_agents",
        "disconnect_all",
        "usage_sources",
        "set_usage_source",
        "setup_state",
        "finish_setup",
        "setup_clocks",
        "setup_use_clock",
    ];
    want.sort_unstable();
    assert_eq!(names, want);
    // and the password never reaches the page, even when the clock is logged in
    let rig = Build::new().password("s3cret").build();
    let service = rig.tc.os_names().keyring_service.clone();
    rig.keys.insert(&service, "clawd", "admin\ns3cret");
    let s = rig.bridge.state().await;
    assert_eq!(s["ok"], true, "{s}");
    assert!(!s.to_string().contains("s3cret"));
}

#[tokio::test(flavor = "multi_thread")]
async fn main_screen_is_handed_to_the_engine() {
    let rig = rig();
    rig.bridge.state().await;
    assert_eq!(
        clock_screen(rig.tc.paths()),
        Some(0),
        "what the clock has now"
    );
    save_on(&rig.bridge, &[json!({"usage": {"screen": 1}})])
        .await
        .unwrap();
    assert_eq!(clock_screen(rig.tc.paths()), Some(1));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clock_on_the_removed_clawd_screen_is_moved_to_auto_on_save() {
    // The page no longer offers the Clawd main screen (2): it shows Auto for such a clock, and
    // that is one real change, so Save writes 0 (and only then; opening writes nothing).
    let rig = rig();
    let mut config = cfg();
    config["usage"]["screen"] = json!(2);
    rig.clock.set_config(config);
    rig.bridge.state().await;
    assert!(
        rig.clock.saved().is_empty(),
        "opening the window changed nothing"
    );
    let view = clawd_app::commands::session::clock_view(&rig.bridge.data().cfg);
    assert_eq!(
        view["usage"]["screen"], 2,
        "the page compares against the real value"
    );
    let r = save_on(&rig.bridge, &[rig.edited(json!({"usage": {"screen": 0}}))])
        .await
        .unwrap();
    assert_eq!(r["ok"], true);
    assert_eq!(r["sent"], json!({"usage": {"screen": 0}}));
    assert_eq!(rig.clock.saved(), vec![json!({"usage": {"screen": 0}})]);
}

#[tokio::test(flavor = "multi_thread")]
async fn window_follows_the_clock_the_tray_found_at_a_new_address() {
    // The window opened on an address that is gone; the tray's scan then saved the clock's new
    // address in clock.json. "Thử lại" must reach it there, with the same login.
    let gone = format!("http://127.0.0.1:{}", dead_port());
    let rig = Build::new().password("pw").base(&gone, "clawd").build();
    let service = rig.tc.os_names().keyring_service.clone();
    rig.keys.insert(&service, "clawd", "admin\npw");
    assert_eq!(
        rig.bridge.state().await["code"],
        "offline",
        "nothing better known yet"
    );
    saved_clock(
        &rig,
        json!({"name": "", "host": "127.0.0.1", "port": rig.clock.port()}),
    );
    let s = rig.bridge.state().await;
    assert_eq!(s["ok"], true, "{s}");
    assert_eq!(s["app"]["clock"], rig.clock.url());
    assert_eq!(
        s["app"]["key"], "clawd",
        "the same clock keeps its saved login"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn window_follows_a_typed_address_and_ignores_the_same_one() {
    let port = dead_port();
    let rig = Build::new()
        .base(&format!("http://127.0.0.1:{port}"), "clawd")
        .build();
    let before = rig.bridge.api();
    saved_clock(&rig, json!({"host": "127.0.0.1", "port": port}));
    assert_eq!(rig.bridge.state().await["code"], "offline");
    assert!(
        std::sync::Arc::ptr_eq(&before, &rig.bridge.api()),
        "the store says the same: no reconnect"
    );
    let typed = FakeClock::start(new_heap(), None);
    saved_clock(&rig, json!({"manual_host": typed.host()}));
    assert_eq!(rig.bridge.state().await["app"]["clock"], typed.url());
}

#[tokio::test(flavor = "multi_thread")]
async fn window_asks_the_trays_newer_address_first() {
    // Waiting out the old address costs about a minute on a real network: when the tray already
    // saved another address, go there without asking the old one at all.
    let old = FakeHttpServer::start().unwrap();
    let rig = Build::new().base(&old.url(), "clawd").build();
    saved_clock(
        &rig,
        json!({"name": "clawd", "host": "127.0.0.1", "port": rig.clock.port()}),
    );
    let started = Instant::now();
    let s = rig.bridge.state().await;
    assert_eq!(s["ok"], true, "{s}");
    assert_eq!(s["app"]["clock"], rig.clock.url());
    assert!(old.requests().is_empty(), "the old address was not asked");
    assert!(started.elapsed() < Duration::from_secs(5));
}

fn feed_heap() -> Value {
    with(styled_heap(), json!({"fwSeq": 5, "style": "synth"}))
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_release_is_offered_installed_with_its_packs_and_style() {
    let tc = TempConfig::new();
    let ff = FakeFeed::start();
    ff.publish(
        6,
        release_files(&["synth", "paper"], 1),
        "stable",
        "Màn chờ có IP",
        1,
        &[],
    );
    let feed = feed_for(&tc, &ff);
    let rig = Build::new().temp(tc).heap(feed_heap()).feed(feed).build();
    // the clock runs release 6 afterwards, with no pack of ours; it checks none (`pak` 0), so
    // there is no checksum to wait for (the wait is `ota`'s own test)
    rig.clock.set_after(with(
        feed_heap(),
        json!({"fwSeq": 6, "pakCrc": "", "deskCrc": "", "pak": 0, "desk": 0, "uptime": 0}),
    ));
    rig.bridge.state().await;
    let fw = feed_settled(&rig.bridge).await;
    assert_eq!(fw["update"], "available");
    assert_eq!(
        (fw["bundle"]["source"].clone(), fw["bundle"]["seq"].clone()),
        (json!("feed"), json!(6))
    );
    assert_eq!(fw["bundle"]["notes"], "Màn chờ có IP");
    assert_eq!(fw["plan"]["fw"], "need");
    let ids: Vec<&str> = fw["styles"]["packs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["synth", "paper"]);
    assert_eq!(fw["styles"]["state"], "ready");
    assert!(
        !ff.file_calls().iter().any(|c| c.contains("firmware.bin")),
        "only the style packs so far"
    );
    assert_eq!(
        update_on(&rig.bridge, &[Value::Null, Value::Null])
            .await
            .unwrap()["ok"],
        true
    );
    let j = wait_job(&rig.bridge).await;
    assert_eq!(
        (j["done"].clone(), j["error"].clone()),
        (json!(true), json!("")),
        "{j}"
    );
    assert_eq!(rig.clock.uploads(), 1);
    assert_eq!(rig.clock.packs(), ["clawdpak", "deskpak"]);
    assert!(
        !rig.clock.styles().is_empty(),
        "synth re-sent (its CRC differs)"
    );
    assert_eq!(rig.clock.heap()["deskCrc"], json!(crc32_hex(&desk_pak())));
    assert_eq!(rig.bridge.state().await["fw"]["update"], "current");
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_older_than_the_bundle_is_ignored_and_errors_show() {
    let tc = TempConfig::new();
    let bundle = bundle_in(&tc, 2);
    let ff = FakeFeed::start();
    ff.publish_default(1);
    let feed = feed_for(&tc, &ff);
    let rig = Build::new()
        .temp(tc)
        .heap(feed_heap())
        .bundle(bundle)
        .feed(feed)
        .build();
    rig.bridge.state().await;
    assert_eq!(feed_settled(&rig.bridge).await["bundle"]["source"], "app");

    // a Worker that refuses the token: the error shows, there is nothing to install
    let tc2 = TempConfig::new();
    let broken = Feed::new(
        &FakeFeed::start().url(),
        "nope",
        "stable",
        Some(tc2.paths().feed_cache_dir()),
    );
    let rig2 = Build::new()
        .temp(tc2)
        .heap(feed_heap())
        .feed(broken)
        .build();
    rig2.bridge.state().await;
    let fw = feed_settled(&rig2.bridge).await;
    assert_eq!(fw["update"], "none");
    assert!(!fw["feedError"].as_str().unwrap().is_empty());
    assert_eq!(
        update_on(&rig2.bridge, &[Value::Null, Value::Null])
            .await
            .unwrap()["code"],
        "bad"
    );
    let checked = check_update_on(&rig2.bridge, &[]).await.unwrap();
    assert!(!checked["fw"]["feedError"].as_str().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn state_does_not_wait_for_a_slow_feed() {
    // The window shows the clock and the bundled packs at once; the feed's release arrives
    // later, and the page picks it up through fw() while `feedBusy` is true.
    let tc = TempConfig::new();
    let bundle = bundle_in(&tc, 2);
    let ff = FakeFeed::start();
    let manifest = ff.publish(
        6,
        release_files(&["synth", "paper"], 1),
        "stable",
        "mới",
        1,
        &[],
    );
    // the same manifest from a Worker that takes its time (its files are not served: the
    // release is kept without its packs, as when a pack download fails)
    let slow = FakeHttpServer::start().unwrap();
    slow.route(
        "GET",
        "/v1/latest",
        FakeResponse::json(200, &manifest).with_delay(Duration::from_millis(1500)),
    );
    let feed = Feed::new(
        &slow.url(),
        TOKEN,
        "stable",
        Some(tc.paths().feed_cache_dir()),
    );
    let rig = Build::new()
        .temp(tc)
        .heap(feed_heap())
        .bundle(bundle)
        .feed(feed)
        .build();
    let started = Instant::now();
    let s = rig.bridge.state().await;
    assert!(
        started.elapsed() < Duration::from_millis(1400),
        "state waited for the feed"
    );
    assert_eq!(s["ok"], true);
    assert_eq!(s["fw"]["feedBusy"], true);
    assert_eq!(s["fw"]["bundle"]["source"], "app");
    assert_eq!(rig.bridge.fw_info()["feedBusy"], true);
    let fw = feed_settled(&rig.bridge).await;
    assert_eq!(fw["bundle"]["source"], "feed");
    assert_eq!(fw["bundle"]["notes"], "mới");
    // asked again within the recheck time: no new check
    let again = rig.bridge.state().await;
    assert_eq!(again["fw"]["feedBusy"], false);
    assert_eq!(slow.requests_to("/v1/latest").len(), 1);
}
