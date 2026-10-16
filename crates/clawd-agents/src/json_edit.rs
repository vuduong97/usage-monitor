//! Safe edits of an agent's JSON config and the helpers adapters share (`agents/base.py`
//! lower half, `hooks._read`/`hooks._write`). Merges, never replaces: every foreign entry stays,
//! key order is preserved (`serde_json` `preserve_order`), a file that does not parse is never
//! written, and every write backs the old file up first.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clawd_contracts::AgentError;
use serde_json::{Map, Value};

/// `hooks._read(path)` / `agents.base.read_json`: the settings object; `Some({})` when the
/// file is missing, `None` when it is unusable (not JSON, or not an object).
pub fn read_json(path: &Path) -> Option<Value> {
    if !path.exists() {
        return Some(Value::Object(Map::new()));
    }
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str::<Value>(&text) {
        Ok(v @ Value::Object(_)) => Some(v),
        _ => None,
    }
}

/// `path` with `suffix` appended to its file name (`Path.with_name(name + suffix)`).
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.file_name().unwrap_or_default().to_owned();
    name.push(suffix);
    path.with_file_name(name)
}

/// `yymmdd-HHMMSS` of now in UTC (Python used local time; the suffix only has to be unique and
/// match `*.bak-clawd-*`).
fn backup_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil_from_days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{:02}{:02}{:02}-{:02}{:02}{:02}",
        year % 100,
        month,
        day,
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// A backup path next to `path` that does not exist yet (Python overwrote a backup made in the
/// same second, losing the user's original: here a counter is appended instead).
fn backup_path(path: &Path) -> PathBuf {
    let base = format!(".bak-clawd-{}", backup_stamp());
    let first = sibling(path, &base);
    if !first.exists() {
        return first;
    }
    (2u32..)
        .map(|n| sibling(path, &format!("{base}-{n}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// `hooks._write(path, data)` / `agents.base.write_json`: back up an existing file as
/// `<name>.bak-clawd-<yymmdd-HHMMSS>`, write `<name>.clawd-tmp` (2-space indent,
/// `ensure_ascii=False`, trailing newline, original permissions) and replace atomically.
pub fn write_json(path: &Path, data: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let existing = fs::metadata(path).ok();
    if existing.is_some() {
        fs::copy(path, backup_path(path))?;
    }
    let mut text = serde_json::to_string_pretty(data).map_err(io::Error::other)?;
    text.push('\n');
    let tmp = sibling(path, ".clawd-tmp");
    fs::write(&tmp, text)?;
    let finish = || -> io::Result<()> {
        if let Some(meta) = &existing {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, path)
    };
    finish().inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// `agents.base.load(path)`: like [`read_json`] but an unusable file is an [`AgentError`]
/// (`"<path> không đọc được (JSON lỗi), Clawd không sửa file này."`).
pub fn load(path: &Path) -> Result<Value, AgentError> {
    read_json(path).ok_or_else(|| {
        AgentError(format!(
            "{} không đọc được (JSON lỗi), Clawd không sửa file này.",
            path.display()
        ))
    })
}

/// `str(value or "")` for one hook-entry field.
fn field_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(false)) => String::new(),
        Some(other) => other.to_string(),
    }
}

/// `agents.base.handler_text(h)`: everything that can hold a command in one hook entry, across
/// agent formats (`command commandWindows command_windows bash powershell exec url args`).
pub fn handler_text(h: &Value) -> String {
    let Some(obj) = h.as_object() else {
        return String::new();
    };
    const KEYS: [&str; 7] = [
        "command",
        "commandWindows",
        "command_windows",
        "bash",
        "powershell",
        "exec",
        "url",
    ];
    let mut parts: Vec<String> = KEYS.iter().map(|k| field_text(obj.get(*k))).collect();
    if let Some(Value::Array(args)) = obj.get("args") {
        parts.extend(args.iter().map(|a| match a {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }));
    }
    parts.join(" ")
}

/// `dict(out.get("hooks") or {}) if isinstance(out.get("hooks"), dict) else {}`.
fn hooks_table(out: &Map<String, Value>) -> Map<String, Value> {
    match out.get("hooks") {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    }
}

/// `if table: out["hooks"] = table else: out.pop("hooks", None)` (position kept, order kept).
fn put_hooks_table(out: &mut Map<String, Value>, table: Map<String, Value>) {
    if table.is_empty() {
        out.shift_remove("hooks");
    } else {
        out.insert("hooks".to_owned(), Value::Object(table));
    }
}

/// `agents.base.merge_groups`: Claude-style `hooks: {Event: [{matcher, hooks: [handler]}]}`:
/// drop our handlers (a group keeps its place while anything else is in it), then, when `add`
/// is given, append one group per event (with `matcher` when `Some`).
///
/// Deviation: an event whose existing value is not an array is left alone (Python raised
/// `AttributeError` there).
pub fn merge_groups(
    data: &Value,
    add: Option<&[(String, Value)]>,
    ours: &dyn Fn(&Value) -> bool,
    matcher: Option<&str>,
) -> Value {
    let mut out = data.as_object().cloned().unwrap_or_default();
    let mut table = hooks_table(&out);
    let events: Vec<String> = table.keys().cloned().collect();
    for event in events {
        let Some(Value::Array(groups)) = table.get(&event) else {
            continue;
        };
        let mut kept: Vec<Value> = Vec::with_capacity(groups.len());
        for group in groups {
            let mut group = group.clone();
            let hs = group
                .get("hooks")
                .and_then(Value::as_array)
                .filter(|hs| hs.iter().any(ours));
            if let Some(hs) = hs {
                let rest: Vec<Value> = hs.iter().filter(|h| !ours(h)).cloned().collect();
                if rest.is_empty() {
                    continue;
                }
                group["hooks"] = Value::Array(rest);
            }
            kept.push(group);
        }
        if kept.is_empty() {
            table.shift_remove(&event);
        } else {
            table.insert(event, Value::Array(kept));
        }
    }
    for (event, handler) in add.unwrap_or_default() {
        let mut group = Map::new();
        if let Some(m) = matcher {
            group.insert("matcher".to_owned(), Value::String(m.to_owned()));
        }
        group.insert("hooks".to_owned(), Value::Array(vec![handler.clone()]));
        push_entry(&mut table, event, Value::Object(group));
    }
    put_hooks_table(&mut out, table);
    Value::Object(out)
}

/// `table.setdefault(event, []).append(entry)`; an existing non-array value stays untouched.
fn push_entry(table: &mut Map<String, Value>, event: &str, entry: Value) {
    match table.get_mut(event) {
        Some(Value::Array(list)) => list.push(entry),
        Some(_) => {}
        None => {
            table.insert(event.to_owned(), Value::Array(vec![entry]));
        }
    }
}

/// `agents.base.merge_flat`: Cursor/Copilot-style `hooks: {event: [handler]}`.
pub fn merge_flat(
    data: &Value,
    add: Option<&[(String, Value)]>,
    ours: &dyn Fn(&Value) -> bool,
) -> Value {
    let mut out = data.as_object().cloned().unwrap_or_default();
    let mut table = hooks_table(&out);
    let events: Vec<String> = table.keys().cloned().collect();
    for event in events {
        let Some(Value::Array(entries)) = table.get(&event) else {
            continue;
        };
        let kept: Vec<Value> = entries.iter().filter(|h| !ours(h)).cloned().collect();
        if kept.is_empty() {
            table.shift_remove(&event);
        } else {
            table.insert(event, Value::Array(kept));
        }
    }
    for (event, handler) in add.unwrap_or_default() {
        push_entry(&mut table, event, handler.clone());
    }
    put_hooks_table(&mut out, table);
    Value::Object(out)
}

/// `agents.base.has_ours`: any entry (or handler inside a group) in `data["hooks"]` is ours.
pub fn has_ours(data: Option<&Value>, ours: &dyn Fn(&Value) -> bool) -> bool {
    let Some(Value::Object(table)) = data.and_then(|d| d.get("hooks")) else {
        return false;
    };
    table.values().any(|entries| {
        entries.as_array().is_some_and(|list| {
            list.iter().any(|e| {
                ours(e)
                    || e.get("hooks")
                        .and_then(Value::as_array)
                        .is_some_and(|hs| hs.iter().any(ours))
            })
        })
    })
}

/// `agents.base.edit_json(path, change, create)`: apply `change` to a JSON config; `"changed"`,
/// `"unchanged"` or `"missing"` (when `create` is false and the file does not exist).
pub fn edit_json(
    path: &Path,
    change: &dyn Fn(&Value) -> Value,
    create: bool,
) -> Result<&'static str, AgentError> {
    if !path.exists() && !create {
        return Ok("missing");
    }
    let current = load(path)?;
    let new = change(&current);
    if new == current {
        return Ok("unchanged");
    }
    write_json(path, &new).map_err(|e| AgentError(format!("{}: {e}", path.display())))?;
    Ok("changed")
}

/// `agents.base.first_str(p, *keys)`: the first non-empty string among `keys`, else `""`.
pub fn first_str(p: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| p.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
        .unwrap_or_default()
        .to_owned()
}

/// `agents.base.base_event(src, sid, tool, inp)`: `{"src", "sid": "<src>:<sid[:160]>", "tool",
/// "cmd"?}` (`cmd` = the tool input's `command`, first 300 chars).
pub fn base_event(src: &str, sid: &str, tool: &Value, input: &Value) -> Value {
    let mut ev = Map::new();
    ev.insert("src".to_owned(), Value::String(src.to_owned()));
    let sid: String = sid.chars().take(160).collect();
    ev.insert("sid".to_owned(), Value::String(format!("{src}:{sid}")));
    ev.insert(
        "tool".to_owned(),
        Value::String(tool.as_str().unwrap_or_default().to_owned()),
    );
    if let Some(cmd) = input.get("command").and_then(Value::as_str) {
        let cmd: String = cmd.chars().take(300).collect();
        ev.insert("cmd".to_owned(), Value::String(cmd));
    }
    Value::Object(ev)
}

/// Python truthiness of a JSON value (`bool(x)`).
pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `str(p.get(key) or default)`.
pub(crate) fn str_or(p: &Value, key: &str, default: &str) -> String {
    match p.get(key) {
        Some(v) if truthy(v) => py_str(v),
        _ => default.to_owned(),
    }
}

/// `{**ev, "ev": kind}`.
pub(crate) fn with_ev(ev: &Value, kind: &str) -> Value {
    let mut out = ev.clone();
    out["ev"] = Value::String(kind.to_owned());
    out
}

/// `{**ev, "ev": kind, **({"debug": True} if is_debug_prompt(p.get("prompt")) else {})}`.
pub(crate) fn prompt_event(ev: &Value, p: &Value) -> Value {
    let mut out = with_ev(ev, "prompt");
    if clawd_events::is_debug_prompt(p.get("prompt").and_then(Value::as_str)) {
        out["debug"] = Value::Bool(true);
    }
    out
}

/// `"attention" if is_question_tool(tool) else "tool"`.
pub(crate) fn pre_tool_kind(tool: &Value) -> &'static str {
    if clawd_events::is_question_tool(tool.as_str().unwrap_or_default()) {
        "attention"
    } else {
        "tool"
    }
}

/// `str(value)` for the `sid` of a plugin payload.
fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        other => other.to_string(),
    }
}

/// `agents.base.prepared(src, p)`: a payload the agent's own plugin already shaped like the
/// tracker's events (pi, opencode): validated through `clawd_events::pi_event`, re-labelled
/// with `src`. Empty when invalid.
pub fn prepared(src: &str, p: &Value) -> Vec<Value> {
    let Some(Value::Object(mut ev)) = clawd_events::pi_event(p) else {
        return Vec::new();
    };
    let sid = py_str(p.get("sid").unwrap_or(&Value::Null));
    let sid: String = sid.chars().take(160).collect();
    ev.insert("src".to_owned(), Value::String(src.to_owned()));
    ev.insert("sid".to_owned(), Value::String(format!("{src}:{sid}")));
    vec![Value::Object(ev)]
}

/// `agents.base.tool_summary` is `clawd_events::tool_summary` (one copy, in events).
pub use clawd_events::tool_summary;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ours(h: &Value) -> bool {
        handler_text(h).contains("clawd-event")
    }

    #[test]
    fn backup_stamp_has_the_python_shape() {
        let s = backup_stamp();
        assert_eq!(s.len(), 13);
        assert_eq!(s.as_bytes()[6], b'-');
        assert!(s
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 6 || b.is_ascii_digit()));
    }

    #[test]
    fn merge_groups_and_flat_keep_foreign_entries() {
        let data = json!({"x": 1, "hooks": {"Stop": [{"hooks": [{"command": "node mine.cjs"}]}]}});
        let add = [(
            "Stop".to_owned(),
            json!({"command": "clawd-event codex Stop"}),
        )];
        let once = merge_groups(&data, Some(&add), &ours, None);
        assert_eq!(once["hooks"]["Stop"][0], data["hooks"]["Stop"][0]);
        assert_eq!(once["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(merge_groups(&once, None, &ours, None), data);

        let flat = json!({"version": 1, "hooks": {"stop": [{"command": "./audit.sh"}]}});
        let add = [
            (
                "stop".to_owned(),
                json!({"command": "clawd-event cursor stop"}),
            ),
            ("sessionEnd".to_owned(), json!({"command": "clawd-event"})),
        ];
        let f1 = merge_flat(&flat, Some(&add), &ours);
        assert_eq!(f1["hooks"]["stop"][0], json!({"command": "./audit.sh"}));
        assert_eq!(merge_flat(&f1, None, &ours), flat);
    }

    #[test]
    fn removal_keeps_key_order_and_shared_groups() {
        let data = json!({"b": 1, "hooks": {"Z": [{"hooks": [{"command": "clawd-event"}, {"command": "mine"}]}], "A": []}, "a": 2});
        let out = merge_groups(&data, None, &ours, None);
        assert_eq!(
            out,
            json!({"b": 1, "hooks": {"Z": [{"hooks": [{"command": "mine"}]}]}, "a": 2})
        );
        let keys: Vec<&String> = out.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["b", "hooks", "a"]);
    }

    #[test]
    fn handler_text_covers_every_agent_format() {
        let h = json!({"bash": "x", "args": ["a", "b"]});
        let text = handler_text(&h);
        assert_eq!(text, "   x    a b");
        assert_eq!(handler_text(&json!("x")), "");
    }
}
