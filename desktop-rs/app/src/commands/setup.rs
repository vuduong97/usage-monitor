//! Settings commands: the setup wizard (`settings_api.py`).
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use std::sync::Arc;

use clawd_clock::{stored_clock, MdnsBrowser, MdnsSource};
use clawd_contracts::ClockTarget;
use clawd_store::firstrun;
use serde_json::{json, Value};

use super::session::{arg, fail, merged, text_of, Bridge};
use super::CommandCtx;
use crate::state::ShellCmd;

/// `_clock_dict(c)`.
fn clock_dict(c: &ClockTarget) -> Value {
    json!({"name": c.name, "host": c.host, "port": c.port, "version": c.version, "base": c.base()})
}

/// `SettingsApi.setup_state`.
pub async fn setup_state(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    Ok(setup_state_of(&Bridge::of(ctx)))
}

fn setup_state_of(b: &Bridge) -> Value {
    let prefs = firstrun::load(b.paths()).ok();
    let left = firstrun::pending(b.paths(), prefs.as_ref());
    json!({
        "ok": true, "steps": firstrun::STEPS, "fresh": left.iter().any(|s| s == "clock"),
        "pending": left, "app": b.app_info(),
    })
}

/// `SettingsApi.finish_setup(steps)`: steps done or skipped (each step saves its own choice
/// first).
pub async fn finish_setup(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    if let Some(steps) = arg(&args, 0).as_array() {
        let steps: Vec<String> = steps.iter().map(text_of).collect();
        let steps: Vec<&str> = steps.iter().map(String::as_str).collect();
        let _ = firstrun::mark_done(b.paths(), &steps);
    }
    b.notify(ShellCmd::Setup);
    Ok(setup_state_of(&b))
}

/// `SettingsApi.setup_clocks`: the wizard's clock step, asked every ~2 s: clocks mDNS sees, and
/// whether the one the app uses now answers. mDNS only: nothing on the network is scanned.
pub async fn setup_clocks(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let mdns = {
        let mut d = b.data();
        match d.mdns.clone() {
            Some(m) => m,
            None => {
                let make = d.mdns_factory.clone();
                let m: Arc<dyn MdnsSource> = match make {
                    Some(make) => make(),
                    None => Arc::new(MdnsBrowser::new()),
                };
                if !m.start() {
                    tracing::warn!("[settings] mDNS unavailable in the settings window");
                }
                d.mdns = Some(Arc::clone(&m));
                m
            }
        }
    };
    let names = mdns.names();
    for name in &names {
        if b.data().mdns_seen.contains_key(name) {
            continue;
        }
        let (m, n) = (Arc::clone(&mdns), name.clone());
        // Resolving blocks for up to a few seconds.
        let found = tokio::task::spawn_blocking(move || m.resolve(&n))
            .await
            .ok()
            .flatten();
        if let Some(c) = found {
            b.data().mdns_seen.insert(name.clone(), c);
        }
    }
    b.data().mdns_seen.retain(|n, _| names.contains(n));
    b.blocking(|b| b.follow_store()).await;
    let current = match stored_clock(&b.store().load()) {
        Some(c) => Some(merged(
            clock_dict(&c),
            json!({"answers": Bridge::answers(&c.base()).await}),
        )),
        None => None,
    };
    let clocks: Vec<Value> = b.data().mdns_seen.values().map(clock_dict).collect();
    Ok(json!({"ok": true, "clocks": clocks, "current": current}))
}

/// `SettingsApi.setup_use_clock(name)`: use the clock mDNS found under this name (remembered
/// by name).
pub async fn setup_use_clock(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    use_clock(&Bridge::of(ctx), &args).await
}

/// `setup_use_clock` on a bridge (the command without the Tauri context).
pub async fn use_clock(b: &Bridge, args: &[Value]) -> Result<Value, String> {
    let name = text_of(arg(args, 0));
    let found = b
        .data()
        .mdns_seen
        .values()
        .find(|c| c.name == name)
        .cloned();
    let Some(c) = found else {
        return Ok(fail(
            "gone",
            "Không còn thấy đồng hồ này. Thử lại hoặc nhập IP.",
        ));
    };
    b.notify(ShellCmd::UseClock {
        name: c.name.clone(),
        host: c.host.clone(),
        port: c.port,
        version: c.version.clone(),
    });
    let (base, key) = (c.base(), c.name.clone());
    b.blocking(move |b| b.connect(&base, &key)).await;
    let answers = Bridge::answers(&c.base()).await;
    Ok(json!({"ok": true, "current": merged(clock_dict(&c), json!({"answers": answers}))}))
}
