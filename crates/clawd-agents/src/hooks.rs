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
    universal_newlines(HOOK_SH).replace("%(port)d", &port.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use clawd_testkit::TempConfig;

    const PORT: u16 = 47913;

    fn hook(tc: &TempConfig, platform: Platform) -> Value {
        let script = tc.paths().hook_script();
        handler(
            tc.paths(),
            &Env::empty(),
            platform,
            Some(PORT),
            Some(&script),
        )
    }

    fn apply_with(tc: &TempConfig, enable: bool, file: &Path, hook: &Value) -> Vec<String> {
        apply(
            tc.paths(),
            &Env::empty(),
            enable,
            Some(&[file.to_path_buf()]),
            Platform::Win32,
            Some(hook),
        )
    }

    fn read(file: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
    }

    #[test]
    fn apply_backs_up_installs_and_removes() {
        let tc = TempConfig::new();
        let original =
            json!({"model": "opus", "hooks": {"Stop": [{"hooks": [{"command": "mine"}]}]}});
        let file = tc.write_home(".claude/settings.json", original.to_string());
        let hook = hook(&tc, Platform::Win32);

        let msgs = apply_with(&tc, true, &file, &hook);
        assert_eq!(msgs, [format!("{}: hooks installed", file.display())]);
        let installed = read(&file);
        assert_eq!(installed["model"], "opus");
        assert_eq!(installed["hooks"]["Stop"][0], original["hooks"]["Stop"][0]);
        assert_eq!(installed["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert!(is_installed(tc.paths(), Some(std::slice::from_ref(&file))));
        let backups = fs::read_dir(file.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("settings.json.bak-clawd-")
            })
            .count();
        assert_eq!(backups, 1);

        let msgs = apply_with(&tc, true, &file, &hook);
        assert_eq!(msgs, [format!("{}: already up to date", file.display())]);

        let msgs = apply_with(&tc, false, &file, &hook);
        assert_eq!(msgs, [format!("{}: hooks removed", file.display())]);
        assert_eq!(read(&file), original);
        assert!(!is_installed(tc.paths(), Some(&[file])));
    }

    #[test]
    fn apply_leaves_broken_or_missing_dirs_alone() {
        let tc = TempConfig::new();
        let hook = hook(&tc, Platform::Win32);
        let broken = tc.write_home(".claude/settings.json", "{ // comments");
        for enable in [true, false] {
            let msgs = apply_with(&tc, enable, &broken, &hook);
            assert_eq!(
                msgs,
                [format!("{}: not valid JSON, left alone", broken.display())]
            );
            assert_eq!(fs::read_to_string(&broken).unwrap(), "{ // comments");
        }
        let siblings = fs::read_dir(broken.parent().unwrap()).unwrap().count();
        assert_eq!(siblings, 1, "no backup or tmp file next to a broken config");

        let missing = tc.home().join(".other-claude").join("settings.json");
        assert_eq!(apply_with(&tc, true, &missing, &hook), Vec::<String>::new());
        assert_eq!(
            apply_with(&tc, false, &missing, &hook),
            Vec::<String>::new()
        );
        assert!(!missing.parent().unwrap().exists());
    }

    #[test]
    fn apply_creates_settings_in_an_existing_claude_dir() {
        let tc = TempConfig::new();
        let hook = hook(&tc, Platform::Win32);
        let dir = tc.home().join(".claude");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");

        let msgs = apply_with(&tc, true, &file, &hook);
        assert_eq!(msgs, [format!("{}: hooks installed", file.display())]);
        let data = read(&file);
        assert_eq!(data["hooks"].as_object().unwrap().len(), EVENTS.len());
        assert_eq!(data["hooks"]["Stop"][0]["hooks"][0], hook);
        assert!(is_installed(tc.paths(), Some(&[file])));
    }

    #[test]
    fn write_script_writes_once_and_repairs_a_changed_copy() {
        let tc = TempConfig::new();
        let env = Env::empty();
        let target = tc.home().join("bin").join("clawd-event.sh");

        let path = write_script(tc.paths(), &env, Some(&target), Some(PORT)).unwrap();
        assert_eq!(path, target);
        assert_eq!(fs::read_to_string(&target).unwrap(), hook_script_text(PORT));
        assert!(hook_script_text(PORT).contains("${CLAWD_EVENT_PORT:-47913}"));

        fs::write(&target, "#!/bin/sh\nexit 1\n").unwrap();
        write_script(tc.paths(), &env, Some(&target), Some(PORT)).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), hook_script_text(PORT));

        // a CRLF copy (a Windows checkout) counts as the same text: left as it is
        let crlf = hook_script_text(PORT).replace('\n', "\r\n");
        fs::write(&target, &crlf).unwrap();
        write_script(tc.paths(), &env, Some(&target), Some(PORT)).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), crlf);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111);
        }
    }

    #[test]
    fn hook_script_text_is_lf_only() {
        assert!(!hook_script_text(PORT).contains('\r'));
        assert!(hook_script_text(PORT).starts_with("#!/bin/sh\n"));
    }

    #[test]
    fn settings_files_default_to_the_home_claude_dir() {
        let tc = TempConfig::new();
        assert_eq!(
            settings_files(tc.paths()),
            [tc.home().join(".claude").join("settings.json")]
        );
    }
}
