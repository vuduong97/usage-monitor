//! Settings commands: coding agents (`settings_api.py`).
//! Each takes the positional JS `args` and returns the JSON the Python method returned
//! (`{"ok": true, ...}` or `{"ok": false, "code", "error"}`); `Err` rejects the promise.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use super::session::{arg, fail, merged, text_of, truthy, Bridge};
use super::CommandCtx;
use crate::state::ShellCmd;

/// `agents.infos(reg)` as JSON.
fn infos(b: &Bridge) -> Value {
    serde_json::to_value(clawd_agents::infos(b.registry())).unwrap_or_else(|_| json!([]))
}

/// `_has_wsl()`: `wsl.exe` on PATH, Windows only.
fn has_wsl() -> bool {
    cfg!(windows)
        && std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|dir| dir.join("wsl.exe").is_file())
        })
}

/// `SettingsApi.agents`: every agent Clawd knows, whether it is on this machine and connected.
/// The ones shown count as offered, so the tray stops offering them.
pub async fn agents(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    agents_of(&Bridge::of(ctx))
}

fn agents_of(b: &Bridge) -> Result<Value, String> {
    let list = clawd_agents::infos(b.registry());
    let found: Vec<&str> = list
        .iter()
        .filter(|i| i.found)
        .map(|i| i.id.as_str())
        .collect();
    let _ = clawd_store::firstrun::mark_seen(b.paths(), &found);
    Ok(
        json!({"ok": true, "agents": serde_json::to_value(&list).unwrap_or_else(|_| json!([])), "wsl": has_wsl()}),
    )
}

/// `SettingsApi.set_agent(aid, on)`.
pub async fn set_agent(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let aid = text_of(arg(&args, 0));
    let on = truthy(arg(&args, 1));
    let Some(agent) = b.registry().get(&aid) else {
        return Ok(fail("bad", "Không biết agent này."));
    };
    let result = if on {
        agent.install()
    } else {
        agent.uninstall()
    };
    match result {
        Err(e) => Ok(merged(
            fail("error", &e.to_string()),
            json!({"agents": infos(&b)}),
        )),
        Ok(msg) => {
            b.notify(ShellCmd::Setup);
            Ok(json!({"ok": true, "msg": msg, "agents": infos(&b)}))
        }
    }
}

/// `SettingsApi.set_agents(ids)`: the wizard's Agent step: connect the ticked agents,
/// disconnect the unticked ones that are connected; nothing else is touched. Errors are
/// listed, the rest still go.
pub async fn set_agents(ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let want: BTreeSet<String> = arg(&args, 0)
        .as_array()
        .map(|ids| ids.iter().map(text_of).collect())
        .unwrap_or_default();
    let mut errors = Vec::new();
    for agent in b.registry().iter() {
        let on = agent.is_installed().unwrap_or(false);
        let result = if want.contains(agent.id()) && !on {
            agent.install().map(drop)
        } else if !want.contains(agent.id()) && on {
            agent.uninstall().map(drop)
        } else {
            Ok(())
        };
        if let Err(e) = result {
            errors.push(format!("{}: {e}", agent.name()));
        }
    }
    b.notify(ShellCmd::Setup);
    let out = json!({"ok": errors.is_empty(), "agents": infos(&b)});
    Ok(if errors.is_empty() {
        out
    } else {
        merged(out, json!({"error": errors.join("\n")}))
    })
}

/// `SettingsApi.disconnect_all`.
pub async fn disconnect_all(ctx: &CommandCtx<'_>, _args: Vec<Value>) -> Result<Value, String> {
    let b = Bridge::of(ctx);
    let lines = clawd_agents::uninstall_all(b.registry());
    b.notify(ShellCmd::Setup);
    Ok(json!({"ok": true, "msg": lines.join("\n"), "agents": infos(&b)}))
}
