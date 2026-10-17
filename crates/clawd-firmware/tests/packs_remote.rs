//! Remote usage style packs: a signed `packs-api{N}` release (here [`FakePacks`] on loopback)
//! is checked end to end, cached by SHA-256, and only ever replaces bundled packs it may.

use clawd_contracts::{AppPaths, Env};
use clawd_firmware::firmware::{load_bundle, sha256_hex};
use clawd_firmware::packs::{self, StyleSource};
use clawd_testkit::fw::{make_style_pack, style_row, write_bundle, FakePacks, TestKey};
use serde_json::json;

fn source(fp: &FakePacks, cache: Option<std::path::PathBuf>) -> StyleSource {
    StyleSource::new(&fp.url(), &fp.public_key(), cache)
}

#[tokio::test]
async fn a_signed_release_arrives_checked_and_is_cached() {
    let t = tempfile::tempdir().unwrap();
    let fp = FakePacks::start();
    fp.publish(2, 1_800_000_000, &["synth", "tetris", "paper"]);
    let src = source(&fp, Some(t.path().join("cache")));
    let r = src.latest(2).await.unwrap().expect("published");
    assert_eq!((r.api, r.rev), (2, 1_800_000_000));
    let ids: Vec<&str> = r.styles.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["synth", "tetris", "paper"], "the index's order");
    assert!(r.styles[2].light);
    for p in &r.styles {
        assert_eq!(
            std::fs::read(&p.path).unwrap(),
            make_style_pack(&p.id),
            "cached file"
        );
        assert_eq!(p.read().unwrap(), make_style_pack(&p.id));
    }
    // again: the index is fetched, the packs come from the cache
    let again = src.latest(2).await.unwrap().unwrap();
    assert_eq!(again, r);
    assert_eq!(fp.got(2, "index.json"), 2);
    assert_eq!(fp.got(2, "tetris.pak"), 1);
    // a new rev without tetris: its cached copy goes
    fp.publish(2, 1_800_000_100, &["synth"]);
    let newer = src.latest(2).await.unwrap().unwrap();
    assert_eq!(newer.styles.len(), 1);
    let left: Vec<_> = std::fs::read_dir(t.path().join("cache")).unwrap().collect();
    assert_eq!(left.len(), 1);
}

#[tokio::test]
async fn nothing_published_is_not_an_error() {
    let fp = FakePacks::start();
    assert_eq!(source(&fp, None).latest(2).await.unwrap(), None);
    assert_eq!(
        source(&fp, None).latest(0).await.unwrap(),
        None,
        "firmware without styles"
    );
}

#[tokio::test]
async fn anything_not_from_the_author_is_refused() {
    let fp = FakePacks::start();
    fp.publish(2, 5, &["synth"]);
    // signed by someone else
    let body =
        json!({"api": 2, "rev": 6, "styles": [style_row("synth", &make_style_pack("synth"))]})
            .to_string()
            .into_bytes();
    fp.publish_index(2, body.clone(), Some(TestKey::default().sign(&body)));
    let e = source(&fp, None).latest(2).await.unwrap_err();
    assert!(e.to_string().contains("not from the author"), "{e}");
    // no signature at all
    fp.publish_index(2, body.clone(), Some(String::new()));
    assert!(source(&fp, None).latest(2).await.is_err());
    // a key that is still the placeholder
    fp.publish_index(2, body, None);
    let unsigned = StyleSource::new(
        &fp.url(),
        clawd_contracts::MINISIGN_PUBKEY_PLACEHOLDER,
        None,
    );
    assert!(unsigned.latest(2).await.is_err());
    // signed, but a pack does not match its SHA-256
    fp.publish(2, 7, &["synth"]);
    fp.tamper(2, "synth.pak");
    let e = source(&fp, None).latest(2).await.unwrap_err();
    assert!(
        e.to_string().contains("does not match the signed index"),
        "{e}"
    );
}

#[tokio::test]
async fn a_signed_pack_with_a_bad_header_or_another_api_is_refused() {
    let fp = FakePacks::start();
    // the right hash of a wrong pack: the header CRC check still catches it
    let mut bad = make_style_pack("synth");
    *bad.last_mut().unwrap() ^= 1;
    let mut row = style_row("synth", &make_style_pack("synth"));
    row["sha256"] = json!(sha256_hex(&bad));
    fp.server.route(
        "GET",
        "/packs-api2/synth.pak",
        clawd_testkit::FakeResponse::bytes(200, bad),
    );
    let body = json!({"api": 2, "rev": 1, "styles": [row]})
        .to_string()
        .into_bytes();
    fp.publish_index(2, body, None);
    assert!(source(&fp, None).latest(2).await.is_err());
    // an index that says api 1 under packs-api2
    let body =
        json!({"api": 1, "rev": 1, "styles": [style_row("synth", &make_style_pack("synth"))]})
            .to_string()
            .into_bytes();
    fp.publish_index(2, body, None);
    let e = source(&fp, None).latest(2).await.unwrap_err();
    assert!(e.to_string().contains("style API"), "{e}");
}

#[tokio::test]
async fn the_bundle_keeps_its_packs_unless_the_remote_ones_are_newer() {
    let t = tempfile::tempdir().unwrap();
    let dir = write_bundle(&t.path().join("fw"), 8, 2, &["synth", "paper"], 1_000);
    let b = load_bundle(&Env::empty(), Some(&dir)).unwrap();
    assert_eq!((b.style_rev, b.styles.len(), b.packs.len()), (1_000, 2, 2));
    let fp = FakePacks::start();
    fp.publish(2, 999, &["tetris"]);
    let old = source(&fp, None).latest(2).await.unwrap().unwrap();
    assert_eq!(
        packs::choose(&b, Some(&old)).styles.len(),
        2,
        "older: bundled"
    );
    fp.publish(2, 1_001, &["tetris", "synth", "paper"]);
    let new = source(&fp, None).latest(2).await.unwrap().unwrap();
    let chosen = packs::choose(&b, Some(&new));
    assert_eq!(chosen.styles.len(), 3);
    assert_eq!(chosen.style_rev, 1_001);
    assert_eq!(
        chosen.packs, b.packs,
        "the image and its animation packs never change"
    );
}

#[test]
fn settings_pick_the_url_or_switch_it_off() {
    let t = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(t.path());
    let gh = packs::from_settings(&paths, &Env::empty()).unwrap();
    assert_eq!(
        gh.base(),
        format!(
            "https://github.com/{}/releases/download",
            clawd_contracts::UPDATE_REPO
        )
    );
    assert_eq!(
        gh.url(2, "index.json"),
        format!(
            "https://github.com/{}/releases/download/packs-api2/index.json",
            clawd_contracts::UPDATE_REPO
        )
    );
    let env =
        |v: &'static str| Env::from_fn(move |k| (k == "CLAWD_PACKS_URL").then(|| v.to_owned()));
    assert!(packs::from_settings(&paths, &env("off")).is_none());
    assert!(packs::from_settings(&paths, &env("http://example.com")).is_none());
    assert_eq!(
        packs::from_settings(&paths, &env("http://127.0.0.1:9/"))
            .unwrap()
            .base(),
        "http://127.0.0.1:9"
    );
}
