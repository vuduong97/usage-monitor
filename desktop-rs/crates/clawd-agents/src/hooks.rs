//! Register (or remove) the Claude Code hooks that let Clawd react to agents (`hooks.py`).
//!
//! Merges, never replaces: every existing hook stays; only our entries (a command running
//! `clawd-event`, or curl posting to `/hook/claude`) are added or removed. The settings file is
//! backed up first and rewritten atomically. Never touches a file that is not valid JSON.
//!
//! The hook must never slow Claude Code down or fail it:
//! * macOS/Linux: a tiny sh + curl script (always exits 0), copied into the config dir so
//!   moving or updating the app cannot break it;
//! * Windows: `curl.exe` in exec form, `async`, so a stopped app shows no hook error.
//!
//! Frozen command shapes (scout §3):
//! posix `{"type":"command","command":"'<CONFIG_DIR>/clawd-event.sh'","timeout":2}`; Windows
//! `{"type":"command","command":"curl.exe","args":["-s","-o","NUL","--max-time","0.3",
//! "--connect-timeout","0.2","-H","Content-Type: application/json","--data-binary","@-",
//! "http://127.0.0.1:<port>/hook/claude"],"async":true,"timeout":5}`.

use std::io;
use std::path::{Path, PathBuf};

use clawd_contracts::{AppPaths, Platform};
use serde_json::Value;

/// `hooks.MARKS`: substrings that identify our hook entries (also those Python wrote).
pub const MARKS: [&str; 2] = ["clawd-event", "/hook/claude"];

/// `hooks.EVENTS`: the Claude hook names we register (`clawd_events::CLAUDE_HOOK_EVENTS`).
pub const EVENTS: [&str; 14] = clawd_events::CLAUDE_HOOK_EVENTS;

/// `hooks.HOOK_SH % {"port": port}`: the text of `clawd-event.sh` (a `sh` script that forwards
/// stdin to `http://127.0.0.1:<port>/hook/${1:-claude}[?e=$2]` with curl and always exits 0).
/// Must be byte-identical to Python's rendering.
pub fn hook_script_text(port: u16) -> String {
    todo!("phase 05")
}

/// `hooks.event_port()`: the event port for hook URLs (`clawd_events::read_event_port` over the
/// config file).
pub fn event_port(paths: &AppPaths) -> u16 {
    todo!("phase 05")
}

/// `hooks.settings_files()`: `settings.json` of every Claude config dir the usage poller reads
/// (`clawd_config::read_config_dirs`), deduplicated.
pub fn settings_files(paths: &AppPaths) -> Vec<PathBuf> {
    todo!("phase 05")
}

/// `hooks.handler(platform, port, script)`: the hook entry for Claude Code on `platform`
/// (shapes in the module docs). `port`/`script` default to [`event_port`] and
/// `paths.hook_script()`.
pub fn handler(
    paths: &AppPaths,
    platform: Platform,
    port: Option<u16>,
    script: Option<&Path>,
) -> Value {
    todo!("phase 05")
}

/// `hooks.write_script(path, port)`: write `clawd-event.sh` when missing or different and make
/// it executable (`0o755` effect); returns the path. Default `paths.hook_script()`.
pub fn write_script(
    paths: &AppPaths,
    path: Option<&Path>,
    port: Option<u16>,
) -> io::Result<PathBuf> {
    todo!("phase 05")
}

/// `hooks.merge(settings, hook)`: `settings` with our hook groups removed and, when `hook` is
/// given, re-added (`{"matcher": "*", "hooks": [hook]}` per event of [`EVENTS`]).
pub fn merge(settings: &Value, hook: Option<&Value>) -> Value {
    todo!("phase 05")
}

/// `hooks.is_installed(files)`: any of `files` (default: [`settings_files`]) holds one of our
/// hook groups.
pub fn is_installed(paths: &AppPaths, files: Option<&[PathBuf]>) -> bool {
    todo!("phase 05")
}

/// `hooks.apply(enable, files, platform, hook)`: install or remove our hooks in every settings
/// file (a missing Claude config dir is skipped when enabling; a file that is not JSON is left
/// alone). Returns one message per file; also logs them.
pub fn apply(
    paths: &AppPaths,
    enable: bool,
    files: Option<&[PathBuf]>,
    platform: Platform,
    hook: Option<&Value>,
) -> Vec<String> {
    todo!("phase 05")
}
