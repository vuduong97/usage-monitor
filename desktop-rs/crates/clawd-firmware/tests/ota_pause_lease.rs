//! The real `pause.json` lease through `clawd_store::pause` while OTA runs (`ota.py:28-37`).
//!
//! `#[ignore]`d until phase 03 is merged: `clawd_store::pause::{hold, release}` are `todo!()` on
//! this branch. Run with `cargo test -p clawd-firmware --test ota_pause_lease -- --ignored`
//! after the rebase onto phase 03.
mod common;

use std::sync::{Arc, Mutex};

use clawd_clock::ClockApi;
use clawd_contracts::{Clock, ClockError};
use clawd_firmware::firmware::{sha256_hex, Bundle};
use clawd_firmware::ota::{install, ClockOps, OtaOptions};
use clawd_testkit::TempConfig;
use serde_json::{json, Value};

struct Clk {
    pause_file: std::path::PathBuf,
    seen: Arc<Mutex<Vec<Value>>>,
    fail: bool,
}

impl Clk {
    fn snap(&self) {
        if let Ok(t) = std::fs::read_to_string(&self.pause_file) {
            self.seen
                .lock()
                .unwrap()
                .push(serde_json::from_str(&t).unwrap());
        }
    }
}

impl ClockOps for Clk {
    async fn heap(&self) -> Result<Value, ClockError> {
        Ok(json!({"version": "2.16.0", "pak": 17, "desk": 31}))
    }
    async fn upload_firmware(&self, _image: &[u8]) -> Result<(), ClockError> {
        self.snap();
        if self.fail {
            return Err(ClockError::Other("no answer".into()));
        }
        Ok(())
    }
    async fn upload_pack(&self, _k: &str, _d: &[u8]) -> Result<Value, ClockError> {
        unreachable!()
    }
    async fn save(&self, _c: &Value) -> Result<Value, ClockError> {
        unreachable!()
    }
    async fn wait_back(
        &self,
        _started: f64,
        _clock: &dyn Clock,
        tick: &(dyn Fn() + Send + Sync),
    ) -> Result<Value, ClockError> {
        tick();
        self.snap();
        Ok(json!({"variant": "esp8266-clawd", "fwSeq": 1}))
    }
}

fn bundle(dir: &std::path::Path) -> Bundle {
    let image = vec![0xe9u8; 100];
    std::fs::write(dir.join("fw.bin"), &image).unwrap();
    Bundle {
        path: dir.join("fw.bin"),
        size: 100,
        sha256: sha256_hex(&image),
        variant: "esp8266-clawd".into(),
        seq: 1,
        layouts: 1,
        commit: String::new(),
        style_api: 0,
        styles: Vec::new(),
    }
}

#[tokio::test(start_paused = true)]
async fn the_lease_is_held_renewed_and_removed_even_on_error() {
    let cfg = TempConfig::new();
    let b = bundle(cfg.root());
    let mut opts = OtaOptions::new(cfg.paths().clone());
    opts.settle = Some(std::time::Duration::from_millis(1));
    for fail in [false, true] {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let api = Clk {
            pause_file: cfg.paths().pause_file(),
            seen: seen.clone(),
            fail,
        };
        let r = install(&api, &b, &opts).await;
        assert_eq!(r.is_err(), fail);
        assert!(!cfg.paths().pause_file().exists(), "released (fail={fail})");
        let seen = seen.lock().unwrap();
        assert!(!seen.is_empty() && seen.iter().all(|s| s["reason"] == "firmware update"));
    }
    // the type check: the real client satisfies the same trait
    fn _is_clock_ops<T: ClockOps>() {}
    _is_clock_ops::<ClockApi>();
}
