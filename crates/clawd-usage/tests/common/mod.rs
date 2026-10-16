//! Shared test setup: an isolated home, a hand-driven clock and fake servers. Nothing here
//! touches the real home, keychain or network.
#![allow(dead_code)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clawd_config::{CommandOutput, CommandRunner};
use clawd_contracts::{Env, UsagePayload};
use clawd_testkit::usage::FakeAnthropic;
use clawd_testkit::{FakeClock, TempConfig};
use clawd_usage::UsageEnv;
use serde_json::Value;

/// 2026-10-11 09:40 UTC (the Python tests' `NOW`).
pub const NOW: f64 = 1_791_711_600.0;
pub const KEY: &str = "mgmt-key-for-tests";
pub const IDX: &str = "2864549a21dbb562";

/// No external program exists in tests.
pub struct NoRunner;

impl CommandRunner for NoRunner {
    fn run(&self, _: &str, _: &[&str], _: Duration) -> io::Result<CommandOutput> {
        Err(io::ErrorKind::NotFound.into())
    }
}

pub fn key_present(_: &dyn CommandRunner) -> Option<String> {
    Some(KEY.to_owned())
}

pub fn key_wrong(_: &dyn CommandRunner) -> Option<String> {
    Some("wrong".to_owned())
}

pub fn key_absent(_: &dyn CommandRunner) -> Option<String> {
    None
}

pub struct Fx {
    pub cfg: TempConfig,
    pub clock: FakeClock,
    pub env: UsageEnv,
    pub anthropic: FakeAnthropic,
}

impl Fx {
    /// Time starts at [`NOW`]; Anthropic URLs point at the fake server; no proxy settings.
    pub fn new() -> Self {
        let cfg = TempConfig::new();
        let clock = FakeClock::new(NOW);
        let anthropic = FakeAnthropic::start();
        let mut env = UsageEnv::new(cfg.paths().clone(), Arc::new(NoRunner));
        env.api_url = anthropic.api_url();
        env.usage_url = anthropic.usage_url();
        env.clock = clock.arc();
        env.env = Env::empty();
        env.client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("client");
        env.cliproxy_key_reader = key_present;
        Self {
            cfg,
            clock,
            env,
            anthropic,
        }
    }

    /// Replace the config file's text.
    pub fn config(&self, text: &str) {
        self.cfg.write_config("config", text);
    }

    /// `desktop.json` with `usage_source`.
    pub fn usage_source(&self, source: &str) {
        let prefs = self.cfg.paths().prefs_file();
        std::fs::create_dir_all(prefs.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            prefs,
            serde_json::json!({"usage_source": source}).to_string(),
        )
        .expect("write prefs");
    }

    /// A Claude config dir holding a credentials file with `token`.
    pub fn claude_dir(&self, name: &str, token: &str) -> PathBuf {
        let dir = self.cfg.root().join(name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(
            dir.join(".credentials.json"),
            serde_json::json!({"claudeAiOauth": {"accessToken": token}}).to_string(),
        )
        .expect("write credentials");
        dir
    }

    /// A Claude config dir without any token.
    pub fn empty_dir(&self, name: &str) -> PathBuf {
        let dir = self.cfg.root().join(name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }
}

pub fn set_mtime(path: &Path, epoch: f64) {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open for mtime");
    file.set_modified(UNIX_EPOCH + Duration::from_secs_f64(epoch))
        .expect("set mtime");
}

pub fn real_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after epoch")
        .as_secs_f64()
}

pub fn payload(json: Value) -> UsagePayload {
    serde_json::from_value(json).expect("payload")
}

/// Compare as compact text, so key order counts like it does on the wire.
pub fn assert_same_json(got: &Value, want: &Value, what: &str) {
    assert_eq!(
        serde_json::to_string(got).expect("json"),
        serde_json::to_string(want).expect("json"),
        "{what}"
    );
}

/// `usage_url` config text for the dirs plus anything else.
pub fn dirs_config(dirs: &[&Path], extra: &str) -> String {
    let list: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
    format!("config_dirs = {}\nclock = off\n{extra}", list.join(", "))
}
