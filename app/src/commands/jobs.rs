//! Settings commands: the progress of the firmware install (`settings_api.py`).
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use serde_json::{json, Value};

use super::session::{merged, Bridge};
use super::CommandCtx;
use crate::state::lock;

/// The job as the page sees it: the progress dict without `started`.
pub fn public_job(job: &Value) -> Value {
    let mut shown = job.clone();
    if let Some(map) = shown.as_object_mut() {
        map.remove("started");
    }
    shown
}

/// `SettingsApi.job`: `{"ok": true, "running": ..., "step", "error", "done"}`; just
/// `running: false` when idle.
pub async fn job(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    job_on(&Bridge::of(ctx), &args).await
}

/// `job` on a bridge (the command without the Tauri context).
pub async fn job_on(b: &Bridge, _args: &[Value]) -> Result<Value, String> {
    Ok(merged(json!({"ok": true}), public_job(&lock(&b.jobs.job))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_job_hides_its_start_time() {
        let running =
            json!({"running": true, "step": "check", "error": "", "done": false, "started": 5.0});
        assert_eq!(
            public_job(&running),
            json!({"running": true, "step": "check", "error": "", "done": false})
        );
        assert_eq!(
            public_job(&json!({"running": false})),
            json!({"running": false})
        );
    }
}
