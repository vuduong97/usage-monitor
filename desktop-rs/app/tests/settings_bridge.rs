//! The settings commands that hand work to the tray, through `Bridge` (the commands minus the
//! Tauri context), over the fake clock of `settings_support`. Test names are the Python names
//! without `test_` where there is one.

mod settings_support;

use clawd_app::commands::clock::set_clock_on;
use clawd_app::state::ShellCmd;
use serde_json::json;
use settings_support::*;

#[tokio::test(flavor = "multi_thread")]
async fn state_reads_the_clock_and_brings_the_opening_tab_once() {
    let mut rig = Build::new().tab("update").build();
    let first = rig.bridge.state().await;
    assert_eq!(first["ok"], true);
    assert_eq!(first["config"]["hostname"], "clawd");
    assert!(first["net"]
        .as_object()
        .is_some_and(|n| n["ssid"] == "Home"));
    assert_eq!(first["tab"], "update");
    assert!(
        rig.bridge.state().await.get("tab").is_none(),
        "the tab rides on the first answer only"
    );
    assert!(rig.told().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_opening_tab_is_not_handed_to_the_page() {
    let rig = Build::new().tab("nowhere").build();
    assert!(rig.bridge.state().await.get("tab").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn set_clock_by_hand_reconnects_and_tells_the_tray() {
    let mut rig = Build::new().base("", "").build();
    let typed = FakeClock::start(new_heap(), None);
    let r = set_clock_on(&rig.bridge, &[json!(typed.host())])
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(rig.bridge.address().0, typed.url());
    assert_eq!(rig.told(), vec![ShellCmd::SetClock { host: typed.host() }]);
    let bad = set_clock_on(&rig.bridge, &[json!("bad host:99999")])
        .await
        .unwrap();
    assert_eq!(bad["code"], "bad");
    // no UnicodeError from the resolver
    let dots = set_clock_on(&rig.bridge, &[json!("10..97.40.207")])
        .await
        .unwrap();
    assert_eq!(dots["code"], "bad");
    assert!(rig.told().is_empty(), "a refused address is not passed on");
    let not_text = set_clock_on(&rig.bridge, &[json!(5)]).await.unwrap();
    assert_eq!(not_text["code"], "bad");
}
