//! `appupdate.Source` against the in-memory GitHub (Python: tests/test_appupdate.py "the release
//! source") plus the minisign checks that only exist in the port.

mod common;

use std::sync::{Arc, Mutex};

use clawd_appupdate::minisig::{self, verify_sums};
use clawd_appupdate::Source;
use clawd_contracts::{AppRelease, UpdateError, SUMS_NAME, SUMS_SIG_NAME};
use clawd_testkit::appupdate::FakeReply;
use common::*;
use serde_json::json;

fn err_text(r: Result<Option<AppRelease>, UpdateError>) -> String {
    r.expect_err("an error").0
}

#[tokio::test]
async fn test_latest_finds_the_asset_and_its_checksum() {
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    let rel = gh
        .source()
        .latest("0.1.0", Some("windows"), Some("x86_64"))
        .await
        .unwrap();
    assert_eq!(
        rel,
        Some(AppRelease {
            version: "0.2.0".into(),
            tag: "v0.2.0".into(),
            notes: "Sửa lỗi xoay màn hình".into(),
            page: gh.release["html_url"].as_str().unwrap().into(),
            name: EXE_NAME.into(),
            url: format!("{BASE}{EXE_NAME}"),
            size: 9,
            sha256: sha(b"installer"),
        })
    );
}

#[tokio::test]
async fn test_a_draft_a_prerelease_or_a_odd_tag_is_not_offered() {
    for change in [
        json!({"draft": true}),
        json!({"prerelease": true}),
        json!({"tag_name": "nightly"}),
        json!({"tag_name": "v0.2.0-rc1"}),
    ] {
        let mut gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
        for (k, v) in change.as_object().unwrap() {
            gh.release[k] = v.clone();
        }
        gh.publish();
        let got = gh
            .source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await;
        assert_eq!(got, Ok(None), "{change}");
    }
}

#[tokio::test]
async fn test_nothing_newer_nothing_for_this_platform_or_no_checksum_yet() {
    let mut gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    let ask = |current: &'static str, system: &'static str, arch: &'static str, gh: &GitHub| {
        let source = gh.source();
        async move { source.latest(current, Some(system), Some(arch)).await }
    };
    assert_eq!(ask("0.2.0", "windows", "x86_64", &gh).await, Ok(None)); // already current
    assert_eq!(ask("0.3.0", "windows", "x86_64", &gh).await, Ok(None)); // running a newer build
    assert_eq!(ask("0.1.0", "mac", "arm64", &gh).await, Ok(None)); // no mac file in this release
    assert_eq!(ask("0.1.0", "linux", "x86_64", &gh).await, Ok(None));
    gh.remove_asset(SUMS_NAME);
    assert_eq!(ask("0.1.0", "windows", "x86_64", &gh).await, Ok(None)); // CI is still uploading
}

#[tokio::test]
async fn test_bad_checksum_file_untrusted_url_and_rate_limit_are_errors() {
    let gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    gh.publish_with(&format!("{}  something-else.exe\n", "0".repeat(64)), None);
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert!(e.contains("SHA256SUMS"), "{e}");

    let mut gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    gh.release["assets"][0]["browser_download_url"] = json!("https://evil.example/Clawd.exe");
    gh.publish();
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(
        e,
        format!("{EXE_NAME}: địa chỉ hoặc kích thước không hợp lệ")
    );

    let limited = clawd_testkit::appupdate::FakeGitHub::new();
    limited.route(API_LATEST, FakeReply::status(403));
    let source = Source::with_transport("o/r", "https://api.github.com", limited).unwrap();
    let e = err_text(
        source
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert!(e.contains("giới hạn"), "{e}");

    assert_eq!(
        Source::new("not a repo", "https://api.github.com")
            .err()
            .unwrap()
            .0,
        "bad repository name: 'not a repo'"
    );
}

#[tokio::test]
async fn http_errors_odd_json_and_bad_sizes_are_worded_like_python() {
    let ask = |reply: FakeReply| async move {
        let fake = clawd_testkit::appupdate::FakeGitHub::new();
        fake.route(API_LATEST, reply);
        let source = Source::with_transport("o/r", "https://api.github.com", fake).unwrap();
        source
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await
    };
    assert_eq!(ask(FakeReply::status(404)).await, Ok(None));
    assert_eq!(
        err_text(ask(FakeReply::status(429)).await),
        "GitHub đang giới hạn số lần hỏi, thử lại sau"
    );
    assert_eq!(
        err_text(ask(FakeReply::status(500)).await),
        "GitHub trả HTTP 500"
    );
    assert_eq!(
        err_text(ask(FakeReply::ok("<html>")).await),
        "phản hồi không phải JSON"
    );
    assert_eq!(ask(FakeReply::json(&json!([1, 2]))).await, Ok(None));
    assert_eq!(
        err_text(ask(FakeReply::ok(vec![b' '; 2 * 1024 * 1024 + 1])).await),
        "phản hồi quá lớn"
    );
    for size in [
        json!(0),
        json!(true),
        json!(9.0),
        json!("9"),
        json!(500 * 1024 * 1024 + 1),
        json!(-1),
    ] {
        let mut gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
        gh.release["assets"][0]["size"] = size.clone();
        gh.publish();
        let e = err_text(
            gh.source()
                .latest("0.1.0", Some("windows"), Some("x86_64"))
                .await,
        );
        assert!(e.contains("không hợp lệ"), "{size}: {e}");
    }
}

#[tokio::test]
async fn the_sums_hop_loop_checks_every_hop_and_stops_after_four() {
    let gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    let sums_url = format!("{}{SUMS_NAME}", gh.base);
    gh.gh
        .route(&sums_url, FakeReply::redirect("https://evil.example/sums"));
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(e, "địa chỉ tải không đáng tin");
    assert!(
        !gh.gh.calls().iter().any(|c| c.contains("evil.example")),
        "no request to the foreign host"
    );

    gh.gh.route(&sums_url, FakeReply::redirect(&sums_url));
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(e, "quá nhiều lần chuyển hướng");

    gh.gh.route(&sums_url, FakeReply::status(404));
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(e, "không đọc được SHA256SUMS (HTTP 404)");

    gh.gh
        .route(&sums_url, FakeReply::ok(vec![b'a'; 64 * 1024 + 1]));
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(e, "không đọc được SHA256SUMS (HTTP 200)");
}

#[tokio::test]
async fn the_check_without_a_signature_or_with_a_wrong_one_is_refused() {
    // 1. no .minisig asset in the release
    let mut gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    gh.remove_asset(SUMS_SIG_NAME);
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(e, minisig::SIGNATURE_MISSING);

    // 2. the signature belongs to other bytes (SHA256SUMS swapped after signing)
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    let signed_for_other = gh.key.sign(b"something else\n");
    gh.publish_with(&gh.sums_text(), Some(signed_for_other));
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(e, minisig::SIGNATURE_MISMATCH);

    // 3. signed with someone else's key
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    let stranger = TestKey::new();
    gh.publish_with(
        &gh.sums_text(),
        Some(stranger.sign(gh.sums_text().as_bytes())),
    );
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(e, minisig::SIGNATURE_MISMATCH);

    // 4. garbage instead of a signature file
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    gh.publish_with(&gh.sums_text(), Some("not a signature".to_owned()));
    let e = err_text(
        gh.source()
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(e, minisig::BAD_SIGNATURE_FILE);

    // 5. the build carries the placeholder key (no production key yet): fails closed
    let gh = GitHub::new(vec![(EXE_NAME, b"installer".to_vec())]);
    let source = Source::with_transport("o/r", "https://api.github.com", gh.gh.clone()).unwrap();
    let e = err_text(
        source
            .latest("0.1.0", Some("windows"), Some("x86_64"))
            .await,
    );
    assert_eq!(e, minisig::BAD_KEY);
}

#[test]
fn verify_sums_accepts_only_the_exact_bytes_under_the_key() {
    let key = TestKey::new();
    let sums = b"abc  file\n";
    let sig = key.sign(sums);
    assert_eq!(verify_sums(sums, &sig, &key.public()), Ok(()));
    assert_eq!(
        verify_sums(b"abc  file\r\n", &sig, &key.public())
            .unwrap_err()
            .0,
        minisig::SIGNATURE_MISMATCH
    );
    assert_eq!(
        verify_sums(sums, "", &key.public()).unwrap_err().0,
        minisig::BAD_SIGNATURE_FILE
    );
    assert_eq!(
        verify_sums(sums, &sig, clawd_contracts::MINISIGN_PUBKEY_PLACEHOLDER)
            .unwrap_err()
            .0,
        minisig::BAD_KEY
    );
}

#[tokio::test]
async fn test_download_checks_size_and_hash_and_only_trusted_hosts() {
    let payload: Vec<u8> = b"installer-bytes".repeat(1000);
    let gh = GitHub::new(vec![(EXE_NAME, payload.clone())]);
    gh.gh.chunk_size(4096);
    let source = gh.source();
    let rel = source
        .latest("0.1.0", Some("windows"), Some("x86_64"))
        .await
        .unwrap()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join(EXE_NAME);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let progress = move |got: u64, total: u64| sink.lock().unwrap().push((got, total));
    source.download(&rel, &dest, Some(&progress)).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), payload);
    let n = payload.len() as u64;
    assert_eq!(seen.lock().unwrap().last(), Some(&(n, n)));
    assert_eq!(
        names(dir.path()),
        vec![EXE_NAME.to_owned()],
        "no .part left over"
    );

    gh.tamper(EXE_NAME); // same size, one byte flipped
    let bad = dir.path().join("bad.exe");
    let e = source.download(&rel, &bad, None).await.unwrap_err().0;
    assert!(e.contains("SHA-256"), "{e}");
    assert!(!bad.exists());
    assert_eq!(names(dir.path()), vec![EXE_NAME.to_owned()]);

    let short = AppRelease {
        size: 10,
        ..rel.clone()
    }; // more bytes than announced
    let e = source
        .download(&short, &dir.path().join("x.exe"), None)
        .await
        .unwrap_err()
        .0;
    assert!(e.contains("lớn hơn"), "{e}");
    let long = AppRelease {
        size: n + 5,
        ..rel.clone()
    }; // fewer bytes than announced
    gh.publish();
    let e = source
        .download(&long, &dir.path().join("w.exe"), None)
        .await
        .unwrap_err()
        .0;
    assert_eq!(
        e,
        format!("{EXE_NAME}: nhận {n} byte, đã công bố {}", n + 5)
    );
    let evil = AppRelease {
        url: "https://evil.example/x".into(),
        ..rel.clone()
    };
    let e = source
        .download(&evil, &dir.path().join("y.exe"), None)
        .await
        .unwrap_err()
        .0;
    assert!(e.contains("đáng tin"), "{e}");
    assert_eq!(names(dir.path()), vec![EXE_NAME.to_owned()]);
}

#[tokio::test]
async fn test_a_redirect_to_a_foreign_host_is_refused() {
    let gh = GitHub::new(vec![(EXE_NAME, b"abc".to_vec())]);
    let source = gh.source();
    let rel = source
        .latest("0.1.0", Some("windows"), Some("x86_64"))
        .await
        .unwrap()
        .unwrap();
    gh.gh.route(
        &format!("{BASE}{EXE_NAME}"),
        FakeReply::redirect("https://evil.example/Clawd.exe"),
    );
    gh.gh.clear_calls();
    let dir = tempfile::tempdir().unwrap();
    let e = source
        .download(&rel, &dir.path().join("z.exe"), None)
        .await
        .unwrap_err()
        .0;
    assert!(e.contains("đáng tin"), "{e}");
    assert_eq!(
        gh.gh.calls(),
        vec![format!("{BASE}{EXE_NAME}")],
        "the foreign host is never asked"
    );

    // a hop that is trusted but loops forever
    gh.gh.route(
        &format!("{BASE}{EXE_NAME}"),
        FakeReply::redirect(&format!("{BASE}{EXE_NAME}")),
    );
    let e = source
        .download(&rel, &dir.path().join("z.exe"), None)
        .await
        .unwrap_err()
        .0;
    assert_eq!(e, "quá nhiều lần chuyển hướng");
    gh.gh
        .route(&format!("{BASE}{EXE_NAME}"), FakeReply::status(404));
    let e = source
        .download(&rel, &dir.path().join("z.exe"), None)
        .await
        .unwrap_err()
        .0;
    assert_eq!(e, format!("tải {EXE_NAME}: HTTP 404"));
}

#[test]
fn only_https_or_loopback_http_apis_are_accepted() {
    assert!(Source::new("o/r", "https://api.github.com/").is_ok());
    assert!(Source::new("o/r", "http://127.0.0.1:9").is_ok());
    assert!(Source::new("o/r", "http://localhost:9").is_ok());
    for api in [
        "http://api.github.com",
        "http://evil.example",
        "ftp://127.0.0.1",
        "api.github.com",
    ] {
        assert!(Source::new("o/r", api).is_err(), "{api}");
    }
    assert_eq!(
        Source::new("o/r", "https://api.github.com///")
            .unwrap()
            .api(),
        "https://api.github.com"
    );
}

#[tokio::test]
async fn a_rust_prefixed_tag_is_not_a_release_of_this_updater() {
    // `rust-v*` tags are the Rust pipeline's drafts/prereleases: only `vX.Y.Z` is a version
    let mut gh = GitHub::new(vec![(EXE_NAME, b"x".to_vec())]);
    gh.release["tag_name"] = json!("rust-v0.2.0");
    gh.publish();
    let got = gh
        .source()
        .latest("0.1.0", Some("windows"), Some("x86_64"))
        .await;
    assert_eq!(got, Ok(None));
}
