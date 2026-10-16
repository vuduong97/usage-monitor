//! FROZEN (phase 02, plan.md rule 2): the single invoke handler.
//!
//! One `generate_handler![py_api, popover_msg]` (red team #7). `py_api(name, args)` replaces
//! pywebview's `js_api`: it refuses any window but `settings`, checks `name` against the
//! 32-name allowlist (the names `app/ui/*.js` call, verified by the test below) and
//! calls the function in `commands/<group>.rs` (phase 13). `popover_msg` refuses any window but
//! `popover` and calls `crate::popover::handle` (phase 12). Not exposed on purpose: `preview`,
//! `busy`, `set_hooks`, `close_mdns`. Capabilities (phase 13) allow `py_api` only for `settings`
//! and `popover_msg` only for `popover`; the label checks here are the second lock.

use std::sync::Arc;

use clawd_contracts::{WINDOW_POPOVER, WINDOW_SETTINGS};
use serde_json::Value;
use tauri::{AppHandle, State, WebviewWindow, Wry};

use crate::state::{AppState, SettingsSession};

pub mod agents;
pub mod backup;
pub mod clock;
pub mod firmware;
pub mod jobs;
pub mod misc;
pub mod session;
pub mod setup;
pub mod state;
pub mod update;
pub mod usage;

/// What every settings command receives. JS arguments arrive positionally as `args`
/// (`api.save(edited, style)` -> `[edited, style]`), like pywebview.
pub struct CommandCtx<'a> {
    pub app: &'a AppHandle,
    pub state: &'a AppState,
    /// The session of the open settings window.
    pub session: Arc<SettingsSession>,
}

/// The allowlist and the dispatcher come from one table, so they cannot disagree.
macro_rules! py_api_table {
    ($($name:literal => $f:path),+ $(,)?) => {
        /// The command names the settings page may call.
        pub const PY_API_NAMES: &[&str] = &[$($name),+];

        async fn dispatch(name: &str, ctx: &CommandCtx<'_>, args: Vec<Value>) -> Result<Value, String> {
            match name {
                $($name => $f(ctx, args).await,)+
                _ => Err(format!("unknown command: {name}")),
            }
        }
    };
}

py_api_table! {
    "agents" => agents::agents,
    "app_update" => update::app_update,
    "app_update_apply" => update::app_update_apply,
    "app_update_check" => update::app_update_check,
    "app_update_page" => update::app_update_page,
    "check_update" => firmware::check_update,
    "clawd_pak" => usage::clawd_pak,
    "disconnect_all" => agents::disconnect_all,
    "export_backup" => backup::export_backup,
    "factory_reset" => clock::factory_reset,
    "finish_setup" => setup::finish_setup,
    "fw" => firmware::fw,
    "import_backup" => backup::import_backup,
    "job" => jobs::job,
    "login" => clock::login,
    "open_log" => misc::open_log,
    "open_web" => misc::open_web,
    "reboot" => clock::reboot,
    "save" => clock::save,
    "scan" => clock::scan,
    "set_agent" => agents::set_agent,
    "set_agents" => agents::set_agents,
    "set_autostart" => misc::set_autostart,
    "set_clock" => clock::set_clock,
    "set_usage_source" => usage::set_usage_source,
    "setup_clocks" => setup::setup_clocks,
    "setup_state" => setup::setup_state,
    "setup_use_clock" => setup::setup_use_clock,
    "state" => state::state,
    "update" => firmware::update,
    "usage_preview" => usage::usage_preview,
    "usage_sources" => usage::usage_sources,
}

/// A command is callable only from its own window: the popover page must not reach the settings
/// commands (which can save to the clock, install firmware, ...) and the other way round.
fn only_for_window(label: &str, allowed: &str, command: &str) -> Result<(), String> {
    if label == allowed {
        Ok(())
    } else {
        Err(format!(
            "{command} is only available to the {allowed} window"
        ))
    }
}

/// `window.pywebview.api.<name>(...args)` from the settings page.
#[tauri::command]
pub async fn py_api(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    args: Vec<Value>,
) -> Result<Value, String> {
    only_for_window(window.label(), WINDOW_SETTINGS, "py_api")?;
    if !PY_API_NAMES.contains(&name.as_str()) {
        return Err(format!("unknown command: {name}"));
    }
    let session = state
        .session
        .get()
        .ok_or_else(|| "no settings session".to_string())?;
    let ctx = CommandCtx {
        app: &app,
        state: &state,
        session,
    };
    dispatch(&name, &ctx, args).await
}

/// `window.webkit.messageHandlers.clawd.postMessage(...)` from the popover page.
#[tauri::command]
pub fn popover_msg(window: WebviewWindow, app: AppHandle, msg: Value) -> Result<(), String> {
    only_for_window(window.label(), WINDOW_POPOVER, "popover_msg")?;
    crate::popover::handle(&app, msg)
}

/// Install the one invoke handler. `main.rs` calls this on the builder.
pub fn register(builder: tauri::Builder<Wry>) -> tauri::Builder<Wry> {
    builder.invoke_handler(tauri::generate_handler![py_api, popover_msg])
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use super::{only_for_window, PY_API_NAMES};

    #[test]
    fn a_command_answers_only_the_window_it_belongs_to() {
        assert!(only_for_window("settings", "settings", "py_api").is_ok());
        assert_eq!(
            only_for_window("popover", "settings", "py_api").unwrap_err(),
            "py_api is only available to the settings window"
        );
        assert_eq!(
            only_for_window("settings", "popover", "popover_msg").unwrap_err(),
            "popover_msg is only available to the popover window"
        );
        // Any other label (a window the app never creates) gets nothing either.
        assert!(only_for_window("main", "settings", "py_api").is_err());
        assert!(only_for_window("", "popover", "popover_msg").is_err());
    }

    /// `api.<name>(` call sites in a JS/HTML source.
    fn api_calls(src: &str, out: &mut BTreeSet<String>) {
        let bytes = src.as_bytes();
        let mut i = 0;
        while let Some(pos) = src[i..].find("api.") {
            let at = i + pos;
            let prev_ok = at == 0
                || !matches!(bytes[at - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'.');
            let rest = &src[at + 4..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .collect();
            if prev_ok && !name.is_empty() && rest[name.len()..].starts_with('(') {
                out.insert(name);
            }
            i = at + 4;
        }
    }

    #[test]
    fn allowlist_is_the_32_names_the_ui_calls() {
        assert_eq!(PY_API_NAMES.len(), 32);
        let unique: BTreeSet<_> = PY_API_NAMES.iter().copied().collect();
        assert_eq!(unique.len(), 32, "duplicate name");
        for hidden in ["preview", "busy", "set_hooks", "close_mdns"] {
            assert!(!unique.contains(hidden));
        }
        let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let mut called = BTreeSet::new();
        for entry in std::fs::read_dir(ui).unwrap() {
            let path = entry.unwrap().path();
            if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("js" | "html")
            ) {
                api_calls(&std::fs::read_to_string(&path).unwrap(), &mut called);
            }
        }
        let allowed: BTreeSet<String> = unique.iter().map(|s| s.to_string()).collect();
        assert_eq!(called, allowed);
    }
}
