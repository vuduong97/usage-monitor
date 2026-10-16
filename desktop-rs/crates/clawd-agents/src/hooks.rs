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

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clawd_contracts::{AppPaths, Env, Platform};
use serde_json::{json, Map, Value};

use crate::json_edit::{read_json, write_json};

/// `hooks.MARKS`: substrings that identify our hook entries (also those Python wrote).
pub const MARKS: [&str; 2] = ["clawd-event", "/hook/claude"];

/// `hooks.EVENTS`: the Claude hook names we register (`clawd_events::CLAUDE_HOOK_EVENTS`).
pub const EVENTS: [&str; 14] = clawd_events::CLAUDE_HOOK_EVENTS;

/// `hooks.HOOK_SH` with Python's `%(port)d` placeholder.
const HOOK_SH: &str = include_str!("../assets/hook_sh.txt");

/// `hooks.HOOK_SH % {"port": port}`: the text of `clawd-event.sh` (a `sh` script that forwards
/// stdin to `http://127.0.0.1:<port>/hook/${1:-claude}[?e=$2]` with curl and always exits 0).
/// Must be byte-identical to Python's rendering.
pub fn hook_script_text(port: u16) -> String {
    HOOK_SH.replace("%(port)d", &port.to_string())
}

/// `hooks.event_port()`: the event port for hook URLs (`clawd_events::read_event_port` over the
/// config file).
pub fn event_port(paths: &AppPaths, env: &Env) -> u16 {
    clawd_events::read_event_port(env, &|name| clawd_config::read_config_value(paths, name))
}

/// `hooks.settings_files()`: `settings.json` of every Claude config dir the usage poller reads
/// (`clawd_config::read_config_dirs`), deduplicated.
pub fn settings_files(paths: &AppPaths) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in clawd_config::read_config_dirs(paths) {
        let file = dir.join("settings.json");
        if !out.contains(&file) {
            out.push(file);
        }
    }
    out
}

/// `hooks.handler(platform, port, script)`: the hook entry for Claude Code on `platform`
/// (shapes in the module docs). `port`/`script` default to [`event_port`] and
/// `paths.hook_script()`.
pub fn handler(
    paths: &AppPaths,
    env: &Env,
    platform: Platform,
    port: Option<u16>,
    script: Option<&Path>,
) -> Value {
    let port = port
        .filter(|p| *p != 0)
        .unwrap_or_else(|| event_port(paths, env));
    if platform == Platform::Win32 {
        return json!({
            "type": "command",
            "command": "curl.exe",
            "args": ["-s", "-o", "NUL", "--max-time", "0.3", "--connect-timeout", "0.2",
                     "-H", "Content-Type: application/json", "--data-binary", "@-",
                     format!("http://127.0.0.1:{port}/hook/claude")],
            "async": true,
            "timeout": 5,
        });
    }
    let script = script.map_or_else(|| paths.hook_script(), Path::to_path_buf);
    json!({"type": "command", "command": format!("'{}'", script.display()), "timeout": 2})
}

/// Python's universal-newline read: `\r\n` and lone `\r` become `\n`.
pub(crate) fn universal_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// `hooks.write_script(path, port)`: write `clawd-event.sh` when missing or different and make
/// it executable (`0o755` effect); returns the path. Default `paths.hook_script()`.
pub fn write_script(
    paths: &AppPaths,
    env: &Env,
    path: Option<&Path>,
    port: Option<u16>,
) -> io::Result<PathBuf> {
    let path = path.map_or_else(|| paths.hook_script(), Path::to_path_buf);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let port = port
        .filter(|p| *p != 0)
        .unwrap_or_else(|| event_port(paths, env));
    let text = hook_script_text(port);
    let current = fs::read(&path)
        .ok()
        .map(|b| universal_newlines(&String::from_utf8_lossy(&b)));
    if current.as_deref() != Some(text.as_str()) {
        fs::write(&path, &text)?;
    }
    make_executable(&path)?;
    Ok(path)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode();
    fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o111))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// `hooks._ours(group)`: a group with a hook whose command text holds one of [`MARKS`].
fn group_is_ours(group: &Value) -> bool {
    let Some(list) = group.get("hooks").and_then(Value::as_array) else {
        return false;
    };
    list.iter().any(|h| {
        let Some(obj) = h.as_object() else {
            return false;
        };
        let mut parts = vec![match obj.get("command") {
            None => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
        }];
        if let Some(Value::Array(args)) = obj.get("args") {
            parts.extend(args.iter().map(|a| match a {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            }));
        }
        let text = parts.join(" ");
        MARKS.iter().any(|m| text.contains(m))
    })
}

/// `hooks.merge(settings, hook)`: `settings` with our hook groups removed and, when `hook` is
/// given, re-added (`{"matcher": "*", "hooks": [hook]}` per event of [`EVENTS`]).
pub fn merge(settings: &Value, hook: Option<&Value>) -> Value {
    let mut out = settings.as_object().cloned().unwrap_or_default();
    let mut table: Map<String, Value> = match out.get("hooks") {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    let events: Vec<String> = table.keys().cloned().collect();
    for event in events {
        let Some(Value::Array(groups)) = table.get(&event) else {
            continue;
        };
        let kept: Vec<Value> = groups
            .iter()
            .filter(|g| !group_is_ours(g))
            .cloned()
            .collect();
        if kept.is_empty() {
            table.shift_remove(&event);
        } else {
            table.insert(event, Value::Array(kept));
        }
    }
    if let Some(hook) = hook {
        for event in EVENTS {
            let group = json!({"matcher": "*", "hooks": [hook.clone()]});
            match table.get_mut(event) {
                Some(Value::Array(list)) => list.push(group),
                Some(_) => {}
                None => {
                    table.insert(event.to_owned(), Value::Array(vec![group]));
                }
            }
        }
    }
    if table.is_empty() {
        out.shift_remove("hooks");
    } else {
        out.insert("hooks".to_owned(), Value::Object(table));
    }
    Value::Object(out)
}

/// `hooks.is_installed(files)`: any of `files` (default: [`settings_files`]) holds one of our
/// hook groups.
pub fn is_installed(paths: &AppPaths, files: Option<&[PathBuf]>) -> bool {
    let defaults;
    let files = match files {
        Some(f) => f,
        None => {
            defaults = settings_files(paths);
            &defaults
        }
    };
    files.iter().any(|f| {
        let Some(Value::Object(data)) = read_json(f) else {
            return false;
        };
        let Some(Value::Object(table)) = data.get("hooks") else {
            return false;
        };
        table.values().any(|groups| {
            groups
                .as_array()
                .is_some_and(|l| l.iter().any(group_is_ours))
        })
    })
}

/// `hooks.apply(enable, files, platform, hook)`: install or remove our hooks in every settings
/// file (a missing Claude config dir is skipped when enabling; a file that is not JSON is left
/// alone). Returns one message per file; also logs them.
pub fn apply(
    paths: &AppPaths,
    env: &Env,
    enable: bool,
    files: Option<&[PathBuf]>,
    platform: Platform,
    hook: Option<&Value>,
) -> Vec<String> {
    let mut msgs: Vec<String> = Vec::new();
    let made;
    let mut hook = hook;
    if enable && hook.is_none() {
        if platform != Platform::Win32 {
            if let Err(e) = write_script(paths, env, None, None) {
                msgs.push(format!("{}: {e}", paths.hook_script().display()));
            }
        }
        made = handler(paths, env, platform, None, None);
        hook = Some(&made);
    }
    let defaults;
    let files = match files {
        Some(f) => f,
        None => {
            defaults = settings_files(paths);
            &defaults
        }
    };
    for f in files {
        if !enable && !f.exists() {
            continue;
        }
        if enable && !f.parent().is_some_and(Path::exists) {
            continue; // that Claude config dir does not exist: Claude Code not set up there
        }
        let Some(current) = read_json(f) else {
            msgs.push(format!("{}: not valid JSON, left alone", f.display()));
            continue;
        };
        let new = merge(&current, if enable { hook } else { None });
        if new == current {
            msgs.push(format!("{}: already up to date", f.display()));
            continue;
        }
        if let Err(e) = write_json(f, &new) {
            msgs.push(format!("{}: write failed: {e}", f.display()));
            continue;
        }
        let what = if enable {
            "hooks installed"
        } else {
            "hooks removed"
        };
        msgs.push(format!("{}: {what}", f.display()));
    }
    for m in &msgs {
        tracing::info!("{m}");
    }
    msgs
}
