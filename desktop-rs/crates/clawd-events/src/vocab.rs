//! Shared vocabulary: tracker event names, Claude hook names and the text/tool helpers every
//! agent adapter uses (`events.py:96-230`). `clawd-agents` imports these; it must not copy them.

/// `events.EVENTS`: every tracker event name an adapter may emit
/// (`start prompt tool tool_done tool_error subagent_start subagent_stop attention
/// attention_seen attention_done compact compact_done stop stop_error abort end`).
pub const EVENTS: [&str; 16] = [
    "start",
    "prompt",
    "tool",
    "tool_done",
    "tool_error",
    "subagent_start",
    "subagent_stop",
    "attention",
    "attention_seen",
    "attention_done",
    "compact",
    "compact_done",
    "stop",
    "stop_error",
    "abort",
    "end",
];

/// `events.CLAUDE_HOOK_EVENTS`: the Claude Code hooks Clawd installs, sorted
/// (`sorted([*_CLAUDE_SIMPLE, "PreToolUse", "PermissionRequest", "Notification"])`).
pub const CLAUDE_HOOK_EVENTS: [&str; 14] = [
    "Notification",
    "PermissionRequest",
    "PostCompact",
    "PostToolUse",
    "PostToolUseFailure",
    "PreCompact",
    "PreToolUse",
    "SessionEnd",
    "SessionStart",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
    "UserPromptSubmit",
];

/// `events.is_test_command(cmd)`: the shell command looks like a test run (`TEST_CMD`).
/// `None` (no command) is false.
pub fn is_test_command(cmd: Option<&str>) -> bool {
    todo!("phase 04")
}

/// `events.is_debug_prompt(text)`: the user prompt asks for debugging (`/ak:debug`, "fix", "bug",
/// "lỗi", an `ak-debug` skill block...). Only the verdict leaves this machine, never the prompt.
pub fn is_debug_prompt(text: Option<&str>) -> bool {
    todo!("phase 04")
}

/// `events.is_debug_skill_load(tool, inp)`: a tool call that reads the `ak-debug`/`ak-fix`
/// `SKILL.md`, or invokes that skill.
pub fn is_debug_skill_load(tool: &str, input: &serde_json::Value) -> bool {
    todo!("phase 04")
}

/// `events.ascii_text(text, limit)`: the clock's fonts are ASCII: fold accents (d-stroke too),
/// collapse whitespace, cap the length at `limit` with a trailing `...`. Default limit
/// `TEXT_MAX` = 90.
pub fn ascii_text(text: &str, limit: usize) -> String {
    todo!("phase 04")
}

/// `events.is_question_tool(name)`: a "ask the user" tool (`AskUserQuestion`, `ask_user`,
/// `question`, ...; an `mcp__<server>__<tool>` name is looked up by its own tool name).
pub fn is_question_tool(name: &str) -> bool {
    todo!("phase 04")
}

/// `events.tool_action(name)`: the action key (`run read edit ...`) for a raw tool name (any
/// case; MCP tools by their own name); `""` when not listed (plain "Working").
pub fn tool_action(name: &str) -> &'static str {
    todo!("phase 04")
}
