//! `usage_preview_*` and `clawd_pak_*` of `tests/test_preview.py`: the settings commands over
//! the real renderer runner (the fake `clawd_render` program of `clawd-firmware`, so it runs on
//! every OS), a fake clock for `/clawd.pak` and the real remote style packs client. Test names
//! are the Python names without `test_`.

mod settings_support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clawd_app::commands::session::Bridge;
use clawd_app::commands::usage::{clawd_pak_on, usage_preview_on};
use clawd_contracts::{Env, UsagePayload};
use clawd_firmware::packs::StyleSource;
use clawd_firmware::preview::{load_renderer, PakCache, PAK_MAX};
use clawd_testkit::fw::FakePacks;
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

/// The services of `b`'s window with a remote packs source.
fn with_packs(b: &Bridge, src: StyleSource) -> clawd_app::state::Services {
    let s = &b.services;
    clawd_app::state::Services::new(
        s.paths.clone(),
        s.names.clone(),
        Arc::clone(&s.updater),
        Arc::clone(&s.registry),
        Some(src),
        s.bundle.clone(),
        Arc::clone(&s.runner),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_preview_draws_remote_packs_with_the_bundled_renderer() {
    let p = preview(&["synth", "paper"], true, true, "frame");
    let fp = FakePacks::start();
    fp.publish(1, 500, &["tetris", "synth"]);
    let src = StyleSource::new(
        &fp.url(),
        &fp.public_key(),
        Some(p.rig.paths().style_packs_dir()),
    );
    // the same window, now with a packs source; the bundled renderer is found through the env
    let folder = p.folder.clone();
    p.rig.bridge.data().env = Some(Env::from_fn(move |k| {
        (k == "CLAWD_PREVIEW_DIR").then(|| folder.display().to_string())
    }));
    let b = Bridge {
        services: Arc::new(with_packs(&p.rig.bridge, src)),
        ..p.rig.bridge.clone()
    };
    b.check_styles(true).await;
    assert_eq!(b.fw_info()["bundle"]["stylesSource"], "remote");
    let out = usage_preview_on(&b, &[json!(["classic", "tetris", "synth"]), json!(false)])
        .await
        .unwrap();
    assert_eq!(out["exact"], true, "{out}");
    assert_eq!(keys(&out), ["classic", "synth", "tetris"]);
    let cache = p.rig.paths().style_packs_dir();
    let drawn = p.log("args.log");
    assert!(
        drawn
            .lines()
            .filter(|l| l.contains(&*cache.to_string_lossy()))
            .count()
            == 2,
        "tetris and the new synth come from the packs cache: {drawn}"
    );
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
