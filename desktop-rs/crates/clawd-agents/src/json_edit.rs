//! Safe edits of an agent's JSON config and the helpers adapters share (`agents/base.py`
//! lower half, `hooks._read`/`hooks._write`). Merges, never replaces: every foreign entry stays,
//! key order is preserved (`serde_json` `preserve_order`), a file that does not parse is never
//! written, and every write backs the old file up first.

use std::io;
use std::path::Path;

use clawd_contracts::AgentError;
use serde_json::Value;

/// `hooks._read(path)` / `agents.base.read_json`: the settings object; `Some({})` when the
/// file is missing, `None` when it is unusable (not JSON, or not an object).
pub fn read_json(path: &Path) -> Option<Value> {
    todo!("phase 05")
}

/// `hooks._write(path, data)` / `agents.base.write_json`: back up an existing file as
/// `<name>.bak-clawd-<yymmdd-HHMMSS>`, write `<name>.clawd-tmp` (2-space indent,
/// `ensure_ascii=False`, trailing newline, original permissions) and replace atomically.
pub fn write_json(path: &Path, data: &Value) -> io::Result<()> {
    todo!("phase 05")
}

/// `agents.base.load(path)`: like [`read_json`] but an unusable file is an [`AgentError`]
/// (`"<path> không đọc được (JSON lỗi), Clawd không sửa file này."`).
pub fn load(path: &Path) -> Result<Value, AgentError> {
    todo!("phase 05")
}

/// `agents.base.handler_text(h)`: everything that can hold a command in one hook entry, across
/// agent formats (`command commandWindows command_windows bash powershell exec url args`).
pub fn handler_text(h: &Value) -> String {
    todo!("phase 05")
}

/// `agents.base.merge_groups`: Claude-style `hooks: {Event: [{matcher, hooks: [handler]}]}`:
/// drop our handlers (a group keeps its place while anything else is in it), then, when `add`
/// is given, append one group per event (with `matcher` when `Some`).
pub fn merge_groups(
    data: &Value,
    add: Option<&[(String, Value)]>,
    ours: &dyn Fn(&Value) -> bool,
    matcher: Option<&str>,
) -> Value {
    todo!("phase 05")
}

/// `agents.base.merge_flat`: Cursor/Copilot-style `hooks: {event: [handler]}`.
pub fn merge_flat(
    data: &Value,
    add: Option<&[(String, Value)]>,
    ours: &dyn Fn(&Value) -> bool,
) -> Value {
    todo!("phase 05")
}

/// `agents.base.has_ours`: any entry (or handler inside a group) in `data["hooks"]` is ours.
pub fn has_ours(data: Option<&Value>, ours: &dyn Fn(&Value) -> bool) -> bool {
    todo!("phase 05")
}

/// `agents.base.edit_json(path, change, create)`: apply `change` to a JSON config; `"changed"`,
/// `"unchanged"` or `"missing"` (when `create` is false and the file does not exist).
pub fn edit_json(
    path: &Path,
    change: &dyn Fn(&Value) -> Value,
    create: bool,
) -> Result<&'static str, AgentError> {
    todo!("phase 05")
}

/// `agents.base.first_str(p, *keys)`: the first non-empty string among `keys`, else `""`.
pub fn first_str(p: &Value, keys: &[&str]) -> String {
    todo!("phase 05")
}

/// `agents.base.base_event(src, sid, tool, inp)`: `{"src", "sid": "<src>:<sid[:160]>", "tool",
/// "cmd"?}` (`cmd` = the tool input's `command`, first 300 chars).
pub fn base_event(src: &str, sid: &str, tool: &Value, input: &Value) -> Value {
    todo!("phase 05")
}

/// `agents.base.prepared(src, p)`: a payload the agent's own plugin already shaped like the
/// tracker's events (pi, opencode): validated through `clawd_events::pi_event`, re-labelled
/// with `src`. Empty when invalid.
pub fn prepared(src: &str, p: &Value) -> Vec<Value> {
    todo!("phase 05")
}

/// `agents.base.tool_summary` is `clawd_events::tool_summary` (one copy, in events).
pub use clawd_events::tool_summary;
