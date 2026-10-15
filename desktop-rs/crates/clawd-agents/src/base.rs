//! What every agent adapter shares (`agents/base.py`).
//!
//! Hooks never slow an agent down or fail it:
//! * macOS/Linux: the shared `clawd-event.sh` (sh + curl, always exits 0), called as
//!   `'<CONFIG_DIR>/clawd-event.sh' <agent> <event>`;
//! * Windows: `cmd /c "curl.exe -s -o NUL --max-time 0.3 --connect-timeout 0.2 --data-binary @-
//!   http://127.0.0.1:<port>/hook/<agent>?e=<event> || exit 0"`.
//!
//! These strings are frozen byte-for-byte (scout §3): hooks installed by the Python 0.1.4 must be
//! recognised ([`Agent::ours`]) and removed by the Rust app.

use std::path::PathBuf;
use std::sync::Arc;

use clawd_contracts::{AgentError, AppPaths, Env, Platform};
use serde::Serialize;
use serde_json::Value;

/// `agents.base.SCRIPT_MARK`: our hook script's file name.
pub const SCRIPT_MARK: &str = "clawd-event";
/// `agents.base.HTTP_MARK`: every hook we write posts to `.../hook/<agent>`.
pub const HTTP_MARK: &str = "/hook/";

/// Everything an adapter needs from its surroundings, injectable (Python: `home`, `platform`,
/// `port`, `script`, `which`, `env` constructor arguments).
#[derive(Clone)]
pub struct AgentCtx {
    /// `Path.home()` and our config dir (`hook_script`, `config_dir`).
    pub paths: AppPaths,
    /// `sys.platform`; golden fixtures run both `Win32` and `Darwin`.
    pub platform: Platform,
    /// Event port to put in hook URLs; `None` = `clawd_events::read_event_port` at use time.
    pub port: Option<u16>,
    /// The hook script path; `None` = `paths.hook_script()`.
    pub script: Option<PathBuf>,
    /// `shutil.which`: where an executable is on PATH.
    pub which: Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>,
    /// Environment (`CODEX_HOME`, `COPILOT_HOME`, `XDG_CONFIG_HOME`, `PI_CODING_AGENT_DIR`).
    pub env: Env,
}

impl AgentCtx {
    /// The real machine: system paths, current platform, process environment, PATH search.
    pub fn system() -> Self {
        todo!("phase 05")
    }
}

/// `Agent.info()`: one row of the settings window's agent list (`where` is the JSON key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub found: bool,
    pub installed: bool,
    #[serde(rename = "where")]
    pub where_: String,
    pub beta: bool,
    pub signals: Vec<String>,
    pub note: String,
    /// Why `installed` could not be determined (a config that does not parse), else empty.
    pub error: String,
}

/// `agents.base.Agent`: one coding agent. The identity methods are data; the rest is behaviour
/// implemented per adapter in phase 05 (the provided bodies below are the shared Python logic).
pub trait Agent: Send + Sync {
    /// `Agent.id` (`claude pi codex gemini cursor opencode copilot`).
    fn id(&self) -> &'static str;
    /// `Agent.name`, shown in the window.
    fn name(&self) -> &'static str;
    /// `Agent.signals`: what reaches the clock (`think tool subagent error attention`).
    fn signals(&self) -> &'static [&'static str];
    /// `Agent.tested`: platforms checked end to end with the real agent.
    fn tested(&self) -> &'static [Platform];
    /// `Agent.note`: one line for the settings window, empty for none.
    fn note(&self) -> &'static str;
    /// The injected surroundings.
    fn ctx(&self) -> &AgentCtx;

    /// `Agent.config_dir()`.
    fn config_dir(&self) -> PathBuf;
    /// `Agent.is_installed()`: our hook/plugin is in the agent's config. `Err` when that config
    /// does not parse.
    fn is_installed(&self) -> Result<bool, AgentError>;
    /// `Agent.install()`: connect; returns the message shown to the user (Vietnamese).
    fn install(&self) -> Result<String, AgentError>;
    /// `Agent.uninstall()`: disconnect; touches only our entries.
    fn uninstall(&self) -> Result<String, AgentError>;
    /// `Agent.normalize(payload, hint)`: tracker events for one hook payload (never fails: an
    /// unexpected shape yields an empty list).
    fn normalize(&self, p: &Value, hint: &str) -> Vec<Value>;

    /// `Agent.found()`: the agent is on this machine (its config dir exists; some also check PATH).
    fn found(&self) -> bool {
        todo!("phase 05")
    }
    /// `Agent.where()`: shown path(s).
    fn where_(&self) -> String {
        todo!("phase 05")
    }
    /// `Agent.beta`: this platform is not in [`Agent::tested`].
    fn beta(&self) -> bool {
        todo!("phase 05")
    }
    /// `Agent.port`.
    fn port(&self) -> u16 {
        todo!("phase 05")
    }
    /// `Agent.url(event)`: `http://127.0.0.1:<port>/hook/<id>[?e=<event>]`.
    fn url(&self, event: &str) -> String {
        todo!("phase 05")
    }
    /// `Agent.posix_command(event)`: `'<script>' <id> <event>`.
    fn posix_command(&self, event: &str) -> String {
        todo!("phase 05")
    }
    /// `Agent.windows_command(event)`: the `cmd /c "curl.exe ..."` string.
    fn windows_command(&self, event: &str) -> String {
        todo!("phase 05")
    }
    /// `Agent.command(event)`: the one for the injected platform.
    fn command(&self, event: &str) -> String {
        todo!("phase 05")
    }
    /// `Agent.prepare()`: the hook script must exist before any posix hook points at it.
    fn prepare(&self) -> std::io::Result<()> {
        todo!("phase 05")
    }
    /// `Agent.ours(handler)`: this hook entry is ours (`/hook/<id>` in its text, or the script
    /// mark plus ` <id> `), including entries Python 0.1.4 wrote.
    fn ours(&self, handler: &Value) -> bool {
        todo!("phase 05")
    }
    /// `PluginAgent.refresh()`: replace a plugin copy older than the app's; true when it did.
    /// Only pi and opencode do anything.
    fn refresh(&self) -> bool {
        false
    }
    /// `Agent.info()`.
    fn info(&self) -> AgentInfo {
        todo!("phase 05")
    }
}
