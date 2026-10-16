//! Shared vocabulary: tracker event names, Claude hook names and the text/tool helpers every
//! agent adapter uses (`events.py:96-230`). `clawd-agents` imports these; it must not copy them.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::pyval::{is_py_space, py_str, truthy};

/// `events.TEXT_MAX`: the longest text line the clock shows.
pub const TEXT_MAX: usize = 90;

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

/// `events.ACTION_TEXT`: `(while running, when it failed)` for an action key (`""` = plain).
pub fn action_text(action: &str) -> (&'static str, &'static str) {
    match action {
        "run" => ("Running", "Run failed"),
        "read" => ("Reading", "Read failed"),
        "edit" => ("Editing", "Edit failed"),
        "search" => ("Searching", "Search failed"),
        "browse" => ("Browsing", "Browse failed"),
        "plan" => ("Planning", "Plan failed"),
        _ => ("Working", "Tool failed"),
    }
}

/// `events.READING_ACTIONS`: the last tool read or searched: "reading".
pub const READING_ACTIONS: [&str; 2] = ["read", "search"];

/// `events.SESSION_SHOWS_THROUGH`: per-session animations that still show (with the agent count)
/// when 2+ agents run.
pub const SESSION_SHOWS_THROUGH: [&str; 3] = ["ultrathink", "debugger", "reading"];

/// `events.VARIANTS`: same-meaning variants of a long agent animation (the state's `v`).
pub fn variants_of(anim: &str) -> &'static [&'static str] {
    match anim {
        "typing" => &["typing boss"],
        "typing boss" => &["typing"],
        "wizard" => &["typing", "building", "carrying"],
        "ultrathink" => &["thinking"],
        _ => &[],
    }
}

/// `events.WORKING_VARIANTS`: a tool with no action: "Working".
pub const WORKING_VARIANTS: [&str; 3] = ["building", "carrying", "typing boss"];
pub const WIZARD_TEXT: &str = "Long task";
pub const DEBUG_TEXT: &str = "Debugging";

/// `events.QUESTION_SEQ`: the clock looks left and right with a "?".
pub const QUESTION_SEQ: [&str; 2] = ["react left", "react right"];

/// `events.QUESTION_TEXTS`: what the clock says while a question is open (ASCII, <= 20 chars).
pub const QUESTION_TEXTS: [&str; 9] = [
    "Hey! Got a question",
    "Your turn!",
    "Hey! Over to you",
    "Your call, boss",
    "Help me pick?",
    "Psst... a question",
    "Knock knock!",
    "Hmm, help me out?",
    "Pick one for me?",
];

/// `events.TEST_CMD` with the trailing lookahead `(?=$|[\s;&|)])` as a consuming group (only
/// whether it matches is used).
static TEST_CMD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:^|[\s;&|(])(?:pytest|py\.test|jest|vitest|mocha|tox|nox|rspec|phpunit|ctest|go\s+test|cargo\s+(?:test|nextest)|(?:npm|pnpm|yarn|bun)(?:\s+run)?\s+test(?::\S+)?|node\s+--test|deno\s+test|make\s+(?:test|check)|python3?\s+-m\s+(?:pytest|unittest)|pio\s+test|swift\s+test|dotnet\s+test|mvn\s+test|gradlew?\s+test)(?:$|[\s;&|)])",
    )
    .expect("TEST_CMD")
});

/// `events.DEBUG_SKILLS` (`$` also matches before a final newline in Python).
static DEBUG_SKILLS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:ak[:-])?(?:debug|fix)\n?$").expect("DEBUG_SKILLS"));

/// `events.DEBUG_WORDS` with `(?<!\w)` / `(?!\w)` as consuming groups.
static DEBUG_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|\W)(?:/(?:ak:|skill:ak-)(?:debug|fix)|debug\w*|fix|bugs?|lỗi)(?:$|\W)")
        .expect("DEBUG_WORDS")
});

static SKILL_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<skill\b[^>]*>[\s\S]*?(?:</skill>|$)").expect("SKILL_BLOCK"));
static SKILL_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<skill\s+name="([^"]+)""#).expect("SKILL_NAME"));
static DEBUG_SKILL_FILE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[/\\])ak-(?:debug|fix)[/\\]SKILL\.md\n?$").expect("DEBUG_SKILL_FILE")
});

/// `events.is_test_command(cmd)`: the shell command looks like a test run (`TEST_CMD`).
/// `None` (no command) is false.
pub fn is_test_command(cmd: Option<&str>) -> bool {
    cmd.is_some_and(|c| TEST_CMD.is_match(c))
}

/// `events.is_debug_prompt(text)`: the user prompt asks for debugging (`/ak:debug`, "fix", "bug",
/// "lỗi", an `ak-debug` skill block...). Only the verdict leaves this machine, never the prompt.
pub fn is_debug_prompt(text: Option<&str>) -> bool {
    let Some(text) = text else {
        return false;
    };
    let text: String = text.nfc().collect();
    if SKILL_NAME
        .captures_iter(&text)
        .any(|c| DEBUG_SKILLS.is_match(&c[1]))
    {
        return true;
    }
    DEBUG_WORDS.is_match(&SKILL_BLOCK.replace_all(&text, " "))
}

/// `events.is_debug_skill_load(tool, inp)`: a tool call that reads the `ak-debug`/`ak-fix`
/// `SKILL.md`, or invokes that skill.
pub fn is_debug_skill_load(tool: &str, input: &Value) -> bool {
    let Some(map) = input.as_object() else {
        return false;
    };
    let name = tool.to_lowercase();
    if name == "skill" {
        let raw = [map.get("skill"), map.get("command")]
            .into_iter()
            .find(|v| truthy(*v))
            .flatten()
            .map(py_str)
            .unwrap_or_default();
        return DEBUG_SKILLS.is_match(raw.trim_matches(|c| c == '/' || c == ' '));
    }
    if tool_action(&name) == "read" {
        let path = [map.get("file_path"), map.get("path")]
            .into_iter()
            .find(|v| truthy(*v))
            .flatten();
        return path
            .and_then(Value::as_str)
            .is_some_and(|p| DEBUG_SKILL_FILE.is_match(p));
    }
    false
}

/// `events.ascii_text(text, limit)`: the clock's fonts are ASCII: fold accents (d-stroke too),
/// collapse whitespace, cap the length at `limit` with a trailing `...`. Default limit
/// `TEXT_MAX` = 90.
pub fn ascii_text(text: &str, limit: usize) -> String {
    let folded: String = text
        .replace('\u{111}', "d")
        .replace('\u{110}', "D")
        .nfkd()
        .filter(char::is_ascii)
        .collect();
    let collapsed = folded
        .split(is_py_space)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.len() <= limit {
        return collapsed;
    }
    let cut = limit.saturating_sub(3).min(collapsed.len());
    format!("{}...", collapsed[..cut].trim_end_matches(is_py_space))
}

/// `name.strip().lower()`, an `mcp__<server>__<tool>` name reduced to its own tool name.
fn tool_key(name: &str) -> String {
    let lowered = name.trim().to_lowercase();
    if lowered.starts_with("mcp__") {
        lowered.splitn(3, "__").last().unwrap_or("").to_owned()
    } else {
        lowered
    }
}

/// `events.is_question_tool(name)`: a "ask the user" tool (`AskUserQuestion`, `ask_user`,
/// `question`, ...; an `mcp__<server>__<tool>` name is looked up by its own tool name).
pub fn is_question_tool(name: &str) -> bool {
    matches!(
        tool_key(name).as_str(),
        "ask_user_question" | "ask_user" | "question" | "askuserquestion" | "request_user_input"
    )
}

/// `events.tool_action(name)`: the action key (`run read edit ...`) for a raw tool name (any
/// case; MCP tools by their own name); `""` when not listed (plain "Working").
pub fn tool_action(name: &str) -> &'static str {
    match tool_key(name).as_str() {
        "bash" | "bashoutput" | "bg_run" | "shell" | "run_shell_command" | "powershell"
        | "exec_command" => "run",
        "read" | "notebookread" | "ls" | "read_symbol" | "read_enclosing" | "read_file"
        | "read_many_files" | "list_directory" | "view" => "read",
        "edit" | "multiedit" | "write" | "notebookedit" | "apply_patch" | "write_file"
        | "replace" | "create" | "str_replace_editor" | "delete" => "edit",
        "grep" | "glob" | "find" | "symbol_search" | "search_file_content" | "rg" => "search",
        "webfetch" | "websearch" | "web_fetch" | "batch_web_fetch" | "web_search"
        | "google_web_search" => "browse",
        "todowrite" | "todo" | "update_plan" | "write_todos" | "update_todo" => "plan",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ascii_text_folds_accents_and_caps_length() {
        assert_eq!(ascii_text("Đã xong việc", 90), "Da xong viec");
        assert_eq!(ascii_text("  a \t b\n", 90), "a b");
        assert_eq!(
            ascii_text(&"x".repeat(100), 90),
            format!("{}...", "x".repeat(87))
        );
        assert_eq!(ascii_text("", 90), "");
        // the cut ends on a space: rstrip before the dots
        assert_eq!(ascii_text("abcde fghij", 9), "abcde...");
        assert_eq!(ascii_text("abcd efghij", 9), "abcd e...");
    }

    #[test]
    fn tool_actions_and_mcp_names() {
        assert_eq!(tool_action("Bash"), "run");
        assert_eq!(tool_action("mcp__srv__Read"), "read");
        assert_eq!(tool_action("mcp__srv__a__grep"), "");
        assert_eq!(tool_action("unknown"), "");
        assert!(is_question_tool("AskUserQuestion"));
        assert!(is_question_tool("mcp__x__question"));
        assert!(!is_question_tool("Bash"));
    }

    #[test]
    fn test_commands() {
        for (cmd, expected) in [
            ("pytest -q", true),
            ("cd x && cargo test -p a", true),
            ("npm run test:unit", true),
            ("echo pytest", true),
            ("pytests", false),
            ("cargo testing", false),
            ("ls", false),
        ] {
            assert_eq!(is_test_command(Some(cmd)), expected, "{cmd}");
        }
        assert!(!is_test_command(None));
    }

    #[test]
    fn debug_prompts_and_skill_loads() {
        assert!(is_debug_prompt(Some("please fix this")));
        assert!(is_debug_prompt(Some("có lỗi")));
        assert!(is_debug_prompt(Some("/ak:debug the thing")));
        assert!(!is_debug_prompt(Some("prefix suffix")));
        assert!(!is_debug_prompt(None));
        assert!(!is_debug_prompt(Some(
            "<skill name=\"ak-cook\">fix bug everywhere</skill> build it"
        )));
        assert!(is_debug_prompt(Some("<skill name=\"ak-debug\">x</skill>")));
        assert!(is_debug_skill_load("Skill", &json!({"skill": "ak:debug"})));
        assert!(!is_debug_skill_load("Skill", &json!({"skill": "ak:cook"})));
        assert!(is_debug_skill_load(
            "Read",
            &json!({"file_path": "/x/skills/ak-debug/SKILL.md"})
        ));
        assert!(!is_debug_skill_load("Read", &json!("not a dict")));
    }
}
