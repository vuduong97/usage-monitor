//! Shared test scaffolding: the Python `GitHub` class (tests/test_appupdate.py) on top of the
//! testkit's in-memory transport, with a throwaway minisign key pair generated per test.
#![allow(dead_code)]

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clawd_appupdate::{Source, Updater, UpdaterOptions};
use clawd_contracts::{AppPaths, SUMS_NAME, SUMS_SIG_NAME};
use clawd_testkit::appupdate::{output_ok, FakeGitHub, FakeLauncher, FakeReply, FakeRunner};
use clawd_testkit::{FakeClock, TempConfig};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const EXE_NAME: &str = "Clawd-0.2.0-windows-setup.exe";
pub const ZIP_NAME: &str = "Clawd-0.2.0-mac-arm64.zip";
pub const BASE: &str = "https://github.com/o/r/releases/download/v0.2.0/";
pub const CDN: &str = "https://release-assets.githubusercontent.com/o/";
pub const API_LATEST: &str = "https://api.github.com/repos/o/r/releases/latest";

pub fn sha(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// A key pair made for one test; the secret never leaves the process.
pub struct TestKey {
    pair: minisign::KeyPair,
}

impl TestKey {
    pub fn new() -> Self {
        Self {
            pair: minisign::KeyPair::generate_unencrypted_keypair().expect("keypair"),
        }
    }

    /// The base64 line of the `.pub` file.
    pub fn public(&self) -> String {
        self.pair.pk.to_base64()
    }

    /// The text of the `.minisig` file for `data`.
    pub fn sign(&self, data: &[u8]) -> String {
        minisign::sign(
            Some(&self.pair.pk),
            &self.pair.sk,
            Cursor::new(data),
            Some("trusted comment"),
            Some("untrusted comment"),
        )
        .expect("sign")
        .to_string()
    }
}

/// A fake release `v<version>` of `o/r` holding `files`, `SHA256SUMS` and its signature.
pub struct GitHub {
    pub gh: Arc<FakeGitHub>,
    pub key: TestKey,
    pub files: Vec<(String, Vec<u8>)>,
    pub release: Value,
    pub base: String,
}

impl GitHub {
    pub fn new(files: Vec<(&str, Vec<u8>)>) -> Self {
        Self::with_version(files, "0.2.0")
    }

    pub fn with_version(files: Vec<(&str, Vec<u8>)>, version: &str) -> Self {
        let base = format!("https://github.com/o/r/releases/download/v{version}/");
        let files: Vec<(String, Vec<u8>)> = files
            .into_iter()
            .map(|(n, d)| (n.to_owned(), d))
            .collect();
        let mut assets: Vec<Value> = files
            .iter()
            .map(|(n, d)| json!({"name": n, "size": d.len(), "browser_download_url": format!("{base}{n}")}))
            .collect();
        assets.push(json!({"name": SUMS_NAME, "size": 100, "browser_download_url": format!("{base}{SUMS_NAME}")}));
        assets.push(json!({"name": SUMS_SIG_NAME, "size": 100, "browser_download_url": format!("{base}{SUMS_SIG_NAME}")}));
        let release = json!({
            "tag_name": format!("v{version}"), "draft": false, "prerelease": false,
            "body": "Sửa lỗi xoay màn hình",
            "html_url": format!("https://github.com/o/r/releases/tag/v{version}"),
            "assets": assets,
        });
        let this = Self {
            gh: FakeGitHub::new(),
            key: TestKey::new(),
            files,
            release,
            base,
        };
        this.publish();
        this
    }

    pub fn sums_text(&self) -> String {
        self.files
            .iter()
            .map(|(n, d)| format!("{}  {n}\n", sha(d)))
            .collect()
    }

    /// (Re)register every route from the current `release`, `files` and `sums`.
    pub fn publish(&self) {
        self.publish_with(&self.sums_text(), None);
    }

    /// Serve `sums` signed by the key (or by `signature` when given).
    pub fn publish_with(&self, sums: &str, signature: Option<String>) {
        let g = &self.gh;
        g.route(API_LATEST, FakeReply::json(&self.release));
        let signature = signature.unwrap_or_else(|| self.key.sign(sums.as_bytes()));
        g.route(&format!("{}{SUMS_NAME}", self.base), FakeReply::redirect(&format!("{CDN}sums")));
        g.route(&format!("{CDN}sums"), FakeReply::ok(sums.as_bytes()));
        g.route(&format!("{}{SUMS_SIG_NAME}", self.base), FakeReply::redirect(&format!("{CDN}sums.sig")));
        g.route(&format!("{CDN}sums.sig"), FakeReply::ok(signature.as_bytes()));
        for (name, data) in &self.files {
            g.route(&format!("{}{name}", self.base), FakeReply::redirect(&format!("{CDN}{name}")));
            g.route(&format!("{CDN}{name}"), FakeReply::ok(data.clone()));
        }
    }

    /// Same size, last byte flipped (a swapped download).
    pub fn tamper(&self, name: &str) {
        let data = &self.files.iter().find(|(n, _)| n == name).expect("file").1;
        let mut bad = data.clone();
        *bad.last_mut().expect("non-empty") = b'X';
        self.gh.route(&format!("{CDN}{name}"), FakeReply::ok(bad));
    }

    pub fn source(&self) -> Source {
        Source::with_transport("o/r", "https://api.github.com", self.gh.clone())
            .expect("source")
            .with_public_key(&self.key.public())
    }

    /// Drop the asset called `name` from the release JSON and republish.
    pub fn remove_asset(&mut self, name: &str) {
        let assets = self.release["assets"].as_array_mut().expect("assets");
        assets.retain(|a| a["name"] != name);
        self.publish();
    }
}

/// What a test needs around one `Updater`.
pub struct Rig {
    pub cfg: TempConfig,
    pub clock: FakeClock,
    pub launcher: Arc<FakeLauncher>,
    pub runner: Arc<FakeRunner>,
}

impl Rig {
    pub fn new() -> Self {
        Self::with_runner(FakeRunner::new(|_, _| Ok(output_ok())))
    }

    pub fn with_runner(runner: Arc<FakeRunner>) -> Self {
        Self {
            cfg: TempConfig::new(),
            clock: FakeClock::new(1000.0),
            launcher: FakeLauncher::new(),
            runner,
        }
    }

    pub fn paths(&self) -> &AppPaths {
        self.cfg.paths()
    }

    pub fn stage(&self) -> PathBuf {
        self.paths().app_update_stage_dir()
    }

    pub fn options(&self, current: &str, system: &str, arch: &str, key: &TestKey) -> UpdaterOptions {
        let mut opts = UpdaterOptions::new(self.paths().clone(), self.runner.clone());
        opts.current = current.to_owned();
        opts.now = self.clock.arc();
        opts.system = Some(system.to_owned());
        opts.arch = Some(arch.to_owned());
        opts.packaged = Some(true);
        opts.launcher = self.launcher.clone();
        opts.public_key = key.public();
        opts
    }

    pub fn windows_updater(&self, gh: &GitHub, current: &str) -> Updater {
        Updater::new(
            Some(gh.source()),
            self.options(current, "windows", "x86_64", &gh.key),
        )
    }

    /// A second process: same stage dir, no source.
    pub fn bystander(&self, gh: &GitHub, current: &str, system: &str) -> Updater {
        Updater::new(None, self.options(current, system, "x86_64", &gh.key))
    }
}

/// Make `ota.busy` exist now (a firmware install in progress).
pub fn set_ota_busy(paths: &AppPaths) {
    std::fs::create_dir_all(paths.config_dir()).expect("mkdir");
    std::fs::write(paths.ota_busy_file(), b"").expect("ota.busy");
}

/// The list of file names in `dir` (sorted).
pub fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}
