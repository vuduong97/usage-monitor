//! `usage_preview_*` and `clawd_pak_*` of `tests/test_preview.py`, and `usage_preview` of
//! `tests/test_renderer_feed.py`: the settings commands over the real renderer runner (the
//! fake `clawd_render` program of `clawd-firmware`, so it runs on every OS), a fake clock for
//! `/clawd.pak` and the real feed client. Test names are the Python names without `test_`.

mod settings_support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clawd_app::commands::session::Bridge;
use clawd_app::commands::usage::{clawd_pak_on, usage_preview_on};
use clawd_contracts::{Env, UsagePayload};
use clawd_firmware::crc32::crc32;
use clawd_firmware::feed::{renderer_platform, Feed};
use clawd_firmware::preview::{load_renderer, PakCache, PAK_MAX};
use clawd_testkit::fw::{make_style_pack, release_files, FakeFeed, TOKEN};
use clawd_testkit::{FakeClock as FakeNow, TempConfig};
use serde_json::{json, Value};
use settings_support::*;

const NOW: f64 = 1_800_000_000.0;

fn payload() -> UsagePayload {
    serde_json::from_value(
        json!({"s": 42, "sr": 134, "w": 67, "wr": 3960, "st": "allowed",
        "t": 1_760_191_500_i64, "tf": 24}),
    )
    .unwrap()
}

/// A window over a fake renderer folder (`styles` it can draw), the bundled firmware or not,
/// the last usage payload the engine sent (`stored`) and a clock that stands at `NOW`.
struct Preview {
    rig: Rig,
    folder: PathBuf,
}

fn preview(styles: &[&str], bundle: bool, stored: bool, mode: &str) -> Preview {
    let tc = TempConfig::new();
    let folder = fake_renderer_folder(tc.root(), mode, "", styles);
    write_clip(&folder);
    let mut build = Build::new();
    if bundle {
        build = build.bundle(make_bundle(
            &tc.root().join("bundle"),
            2,
            1,
            &["synth", "paper"],
        ));
    }
    let rig = build.temp(tc).build();
    if stored {
        clawd_store::paths::set_usage_payload(&rig.paths(), &payload(), Some(NOW - 30.0)).unwrap();
    }
    let renderer = load_renderer(&Env::empty(), Some(&folder), "").expect("renderer");
    let now = FakeNow::new(NOW);
    {
        let mut d = rig.bridge.data();
        d.renderer = Some(Arc::new(renderer));
        d.renderer_loaded = true;
        d.clock = now.arc();
    }
    Preview { rig, folder }
}

impl Preview {
    fn set_mode(&self, mode: &str) {
        std::fs::write(self.folder.join("mode"), mode).unwrap();
    }

    fn log(&self, name: &str) -> String {
        std::fs::read_to_string(self.folder.join(name)).unwrap_or_default()
    }

    /// The styles the renderer was run for, in order (`classic`: no `--style`).
    fn calls(&self) -> Vec<String> {
        self.log("args.log")
            .lines()
            .map(|line| {
                let words: Vec<&str> = line.split_whitespace().collect();
                match words.iter().position(|w| *w == "--style") {
                    Some(i) => Path::new(words[i + 1])
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    None => "classic".to_owned(),
                }
            })
            .collect()
    }

    /// The payload each run was given on stdin (`null`: none).
    fn payloads(&self) -> Vec<Value> {
        self.log("stdin.log")
            .lines()
            .map(|l| serde_json::from_str(l).unwrap_or(Value::Null))
            .collect()
    }

    async fn ask(&self, args: Value) -> Value {
        usage_preview_on(&self.rig.bridge, args.as_array().unwrap())
            .await
            .unwrap()
    }
}

fn keys(out: &Value) -> Vec<String> {
    let mut k: Vec<String> = out["frames"].as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_draws_every_style_with_the_real_usage() {
    let p = preview(&["synth", "paper"], true, true, "clip");
    let out = p.ask(json!([])).await;
    assert!(out["ok"] == true && out["exact"] == true, "{out}");
    assert_eq!(keys(&out), ["classic", "paper", "synth"]);
    assert_eq!(
        (out["sentAt"].clone(), out["now"].clone()),
        (json!(NOW - 30.0), json!(NOW))
    );
    let live = out["payload"].clone();
    assert!(
        live["s"] == 42 && live["w"] == 67 && live["t"] != json!(1_760_191_500_i64),
        "real numbers, wall clock now"
    );
    assert_eq!(p.calls(), ["classic", "synth", "paper"]);
    assert!(p.payloads().iter().all(|sent| *sent == live));
    let url = out["frames"]["synth"].as_str().unwrap();
    assert!(url.starts_with("data:image/png;base64,"));
    let png = b64_decode(url.split_once(',').unwrap().1);
    assert!(png.windows(4).any(|w| w == b"acTL"), "an animated loop");
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_without_a_bundle_draws_only_the_classic_screen() {
    let p = preview(&["synth", "paper"], false, true, "clip");
    let out = p.ask(json!([])).await;
    assert_eq!(keys(&out), ["classic"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_without_motion_draws_still_frames() {
    let p = preview(&["synth", "paper"], true, true, "frame");
    let still = p.ask(json!([["synth"], false])).await;
    assert!(still["frames"]["synth"]
        .as_str()
        .unwrap()
        .starts_with("data:image/bmp;base64,"));
    // not a bool: animate
    p.set_mode("clip");
    let moving = p.ask(json!([["synth"], "yes"])).await;
    assert!(moving["frames"]["synth"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));
    p.set_mode("frame");
    assert_eq!(p.ask(json!([["synth"], false])).await, still);
    assert_eq!(p.calls().len(), 2, "each kind cached");
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_reuses_frames_within_the_minute() {
    let p = preview(&["synth", "paper"], true, true, "clip");
    p.ask(json!([["paper"]])).await;
    p.ask(json!([["paper"]])).await;
    assert_eq!(p.calls().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_ignores_bad_style_lists_and_skips_what_the_renderer_lacks() {
    let p = preview(&["synth"], true, true, "clip");
    // not a style list: all of them; the renderer has no paper
    let out = p.ask(json!([["x", 99, true]])).await;
    assert_eq!(keys(&out), ["classic", "synth"]);
    let p = preview(&["synth", "paper"], true, true, "clip");
    let out = p.ask(json!([["paper", "paper", "classic"]])).await;
    assert_eq!(keys(&out), ["classic", "paper"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_without_a_renderer_hands_the_page_the_real_payload() {
    let p = preview(&["synth"], false, true, "clip");
    p.rig.bridge.data().renderer = None;
    let out = p.ask(json!([])).await;
    assert!(out["exact"] == false && out.get("frames").is_none());
    assert_eq!(out["payload"]["s"], 42);
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_falls_back_when_the_renderer_fails() {
    let p = preview(&["synth"], false, true, "exit1");
    let out = p.ask(json!([])).await;
    assert!(out["ok"] == true && out["exact"] == false, "{out}");
    assert_eq!(out["payload"]["s"], 42);
    assert_eq!(p.calls().len(), 1, "it was asked, and failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_before_any_usage_draws_the_waiting_screen() {
    let p = preview(&["synth"], false, false, "clip");
    let out = p.ask(json!([["classic"]])).await;
    assert!(
        out["exact"] == true && out["payload"].is_null() && out["sentAt"].is_null(),
        "{out}"
    );
    assert_eq!(p.calls(), ["classic"]);
    assert_eq!(
        p.payloads(),
        [Value::Null],
        "no payload: the waiting screen"
    );
}

fn feed_heap() -> Value {
    with(styled_heap(), json!({"fwSeq": 5, "style": "synth"}))
}

/// `renderer_zip`: a renderer as `tools/build_preview.py --zip` packs it, around the fake
/// program.
fn renderer_zip(commit: &str) -> Vec<u8> {
    let exe_name = if cfg!(windows) {
        "clawd_render.exe"
    } else {
        "clawd_render"
    };
    let exe = std::fs::read(fake_renderer_exe()).unwrap();
    let packs: Vec<(String, Vec<u8>)> = ["synth", "paper"]
        .iter()
        .map(|id| ((*id).to_owned(), make_style_pack(id)))
        .collect();
    let rows: Vec<Value> = packs
        .iter()
        .map(|(id, d)| {
            json!({"id": id, "name": id, "file": format!("{id}.pak"), "size": d.len(),
                "crc": format!("{:08x}", crc32(&d[16..])), "light": id == "paper"})
        })
        .collect();
    let index = json!({"api": 1, "styles": rows}).to_string();
    let preview = json!({"exe": exe_name, "pak": "clawd.pak", "commit": commit, "layouts": 1,
        "styleApi": 1, "fwSeq": 6})
    .to_string();
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("preview.json".into(), preview.into_bytes()),
        (exe_name.into(), exe),
        ("clawd.pak".into(), b"pak".to_vec()),
        ("styles/index.json".into(), index.into_bytes()),
    ];
    files.extend(
        packs
            .into_iter()
            .map(|(id, d)| (format!("styles/{id}.pak"), d)),
    );
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_slice()))
        .collect();
    zip_stored(&refs)
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_draws_with_the_feed_renderer() {
    let tc = TempConfig::new();
    let ff = FakeFeed::start();
    let platform = renderer_platform();
    ff.publish(
        6,
        release_files(&["synth", "paper"], 1),
        "stable",
        "",
        1,
        &[(platform, renderer_zip("c000006"))],
    );
    let feed = Feed::new(
        &ff.url(),
        TOKEN,
        "stable",
        Some(tc.paths().feed_cache_dir()),
    );
    let rig = Build::new().temp(tc).heap(feed_heap()).feed(feed).build();
    rig.bridge.state().await;
    assert!(wait_for(30, || rig.bridge.fw_info()["feedBusy"] == json!(false)).await);
    assert_eq!(rig.bridge.fw_info()["bundle"]["source"], "feed");
    let out = usage_preview_on(&rig.bridge, &[json!(["classic", "synth"]), json!(false)])
        .await
        .unwrap();
    assert_eq!(out["exact"], true, "{out}");
    assert_eq!(keys(&out), ["classic", "synth"]);
}

// ---- the clock's own clawd.pak ---------------------------------------------------------------

fn pak_rig(pak: Option<Vec<u8>>, heap_pak: u64) -> Rig {
    let rig = Build::new()
        .heap(json!({"pak": heap_pak, "uptime": 5}))
        .build();
    rig.clock.set_pak(pak);
    rig
}

async fn state_of(rig: &Rig) {
    rig.bridge.state().await;
}

fn pak_of(r: &Value) -> Vec<u8> {
    b64_decode(r["pak"].as_str().expect("a pak"))
}

#[tokio::test(flavor = "multi_thread")]
async fn clawd_pak_reads_the_clock_once_and_keeps_it() {
    let pak = make_pak(2, 4, 3, 2);
    let rig = pak_rig(Some(pak.clone()), 17);
    state_of(&rig).await;
    let first = clawd_pak_on(&rig.bridge, &[]).await.unwrap();
    assert!(first["ok"] == true && pak_of(&first) == pak, "{first}");
    let file = rig.paths().pak_file();
    assert_eq!(std::fs::read(&file).unwrap(), pak);
    let meta: Value = serde_json::from_str(
        &std::fs::read_to_string(file.with_file_name("clawd.pak.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(meta, json!({"count": 17}));
    let mut names: Vec<String> = std::fs::read_dir(file.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("clawd"))
        .collect();
    names.sort();
    assert_eq!(names, ["clawd.pak", "clawd.pak.json"], "no .tmp left");
    assert_eq!(PakCache::new(file).load(), Some((pak, 17)));
    assert_eq!(clawd_pak_on(&rig.bridge, &[]).await.unwrap(), first);
    assert_eq!(rig.clock.pak_reads(), 1, "the cache answers now");
    // the clock got a new pack
    let rig2 = pak_rig_same_home(&rig, Some(make_pak(3, 4, 3, 2)), 3);
    state_of(&rig2).await;
    assert_eq!(
        pak_of(&clawd_pak_on(&rig2.bridge, &[]).await.unwrap()),
        make_pak(3, 4, 3, 2)
    );
    assert_eq!(rig2.clock.pak_reads(), 1);
}

fn pak_rig_same_home(first: &Rig, pak: Option<Vec<u8>>, heap_pak: u64) -> Rig {
    let rig = Build::new()
        .heap(json!({"pak": heap_pak, "uptime": 5}))
        .same_home_as(first)
        .build();
    rig.clock.set_pak(pak);
    rig
}

#[tokio::test(flavor = "multi_thread")]
async fn clawd_pak_without_one_on_the_clock_says_so_and_does_not_hammer_it() {
    let rig = pak_rig(None, 17);
    let now = FakeNow::new(NOW);
    rig.bridge.data().clock = now.arc();
    state_of(&rig).await;
    assert_eq!(clawd_pak_on(&rig.bridge, &[]).await.unwrap()["ok"], false);
    assert_eq!(rig.clock.pak_reads(), 1);
    assert_eq!(clawd_pak_on(&rig.bridge, &[]).await.unwrap()["ok"], false);
    assert_eq!(rig.clock.pak_reads(), 1, "waits before asking again");
    now.advance(30.0 + 1.0);
    clawd_pak_on(&rig.bridge, &[]).await.unwrap();
    assert_eq!(rig.clock.pak_reads(), 2);
    // heap: no pack on the clock
    let none = pak_rig(Some(make_pak(2, 4, 3, 2)), 0);
    state_of(&none).await;
    assert_eq!(
        clawd_pak_on(&none.bridge, &[]).await.unwrap()["code"],
        "nopak"
    );
    assert_eq!(none.clock.pak_reads(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn clawd_pak_refuses_a_damaged_pack_and_keeps_the_old_one() {
    let good = make_pak(2, 4, 3, 2);
    let first = pak_rig(Some(good.clone()), 17);
    state_of(&first).await;
    clawd_pak_on(&first.bridge, &[]).await.unwrap();
    // a different, broken pack
    let rig = pak_rig_same_home(&first, Some(b"CLWD junk".to_vec()), 5);
    state_of(&rig).await;
    assert_eq!(pak_of(&clawd_pak_on(&rig.bridge, &[]).await.unwrap()), good);
    assert_eq!(std::fs::read(rig.paths().pak_file()).unwrap(), good);
    assert!(PAK_MAX > good.len());
    let _: &Bridge = &rig.bridge;
}
