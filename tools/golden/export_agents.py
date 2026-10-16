#!/usr/bin/env python3
"""Golden exporter for `clawd-agents` (phase 05): `desktop-rs/fixtures/agents/*.json`.

Runs the REAL Python adapters (`clawd_desktop.agents`, `clawd_desktop.hooks`) inside the isolating
harness (temp home, no real config, no network) and records what Rust must reproduce:

* `hooks`     : `hooks.py` handler shapes, the `clawd-event.sh` text, `merge`, `is_installed`;
* `configs`   : install / install again / uninstall of every adapter on realistic initial config
                files (foreign hooks, unknown keys, key order), final `info()`;
* `normalize` : hook payloads -> tracker events, per adapter.

Paths: every string that holds the temp home becomes `<home>...` with `/` separators, so no
`C:\\Users` or temp dir leaks into a fixture. `needs_other_phases` marks cases whose Rust side calls
code that phase 03 (`clawd-config`) / phase 04 (`clawd-events`) implement.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _harness as h  # noqa: E402

ctx = h.isolate()
from clawd_desktop import agents, hooks  # noqa: E402  (after isolate())
from clawd_desktop.agents.base import AgentError  # noqa: E402

PORT = 47913
HOME_PLACEHOLDER = "<home>"


def norm(value):
    """`h.rel` plus `/` separators in every string that held a path."""
    value = ctx.rel(value)
    if isinstance(value, dict):
        return {k: norm(v) for k, v in value.items()}
    if isinstance(value, list):
        return [norm(v) for v in value]
    if isinstance(value, str) and HOME_PLACEHOLDER in value:
        return value.replace("\\", "/")
    return value


def expand(value):
    """`<home>` in a case's input -> the temp home (as a string)."""
    if isinstance(value, dict):
        return {k: expand(v) for k, v in value.items()}
    if isinstance(value, list):
        return [expand(v) for v in value]
    if isinstance(value, str):
        return value.replace(HOME_PLACEHOLDER, ctx.home.as_posix())
    return value


def fresh_home() -> Path:
    import shutil
    if ctx.home.exists():
        shutil.rmtree(ctx.home)
    ctx.home.mkdir(parents=True)
    return ctx.home


def snapshot(files: list[str]) -> dict:
    out = {}
    for rel in files:
        f = ctx.home / rel
        if not f.is_file():
            out[rel] = None
            continue
        text = f.read_text(encoding="utf-8")
        try:
            value = norm(json.loads(text))
        except ValueError:
            out[rel] = {"raw": text}
            continue
        entry = {"value": value, "backed_up": bool(list(f.parent.glob(f.name + ".bak-clawd-*")))}
        if value == json.loads(text):          # no path in it: the exact text is checkable
            entry["text"] = text
        out[rel] = entry
    return out


# ---- hooks.py -------------------------------------------------------------------------
def export_hooks() -> None:
    script = PurePosixPath("/home/u/.config/claude-usage-monitor/clawd-event.sh")
    handlers = {p: hooks.handler(p, port=PORT, script=script) for p in ("darwin", "win32")}
    ours = {"matcher": "*", "hooks": [hooks.handler("darwin", port=PORT, script=script)]}
    foreign = {"matcher": "Bash", "hooks": [{"type": "command", "command": "node mine.cjs"}]}
    shared = {"matcher": "*", "hooks": [{"type": "command", "command": "'/x/clawd-event.sh'"},
                                         {"type": "command", "command": "keep.sh"}]}
    settings = [
        {},
        {"model": "opus", "hooks": {"Stop": [foreign], "PreToolUse": [ours]}, "zeta": [1, 2.5, None]},
        {"b": 1, "hooks": {"Stop": [shared], "Other": []}, "a": {"x": True}},
    ]
    merges = []
    for s in settings:
        for hook in (handlers["darwin"], handlers["win32"], None):
            merges.append({"settings": s, "hook": hook, "result": hooks.merge(s, hook)})
    installed_cases = []
    for s in settings + [merges[1]["result"]]:
        f = ctx.home / "probe" / "settings.json"
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(json.dumps(s))
        installed_cases.append({"settings": s, "installed": hooks.is_installed([f])})
    h.write_fixture(
        "agents", "hooks",
        input={"port": PORT, "script": str(script), "marks": list(hooks.MARKS),
               "events": list(hooks.EVENTS), "merges": [{k: m[k] for k in ("settings", "hook")} for m in merges],
               "installed": [{"settings": c["settings"]} for c in installed_cases]},
        output={"handlers": handlers, "script_text": hooks.HOOK_SH % {"port": PORT},
                "merges": [m["result"] for m in merges],
                "installed": [c["installed"] for c in installed_cases]},
        ctx=ctx)


# ---- adapters -------------------------------------------------------------------------
FOREIGN_STOP = {"matcher": "Edit|Write", "hooks": [{"type": "command", "command": "node k.cjs"}]}

CONFIG_CASES = [
    # (name, agent, platform, env, {relpath: json|text|None}, steps, watched files)
    ("claude_darwin_foreign", "claude", "darwin", {},
     {".claude/settings.json": {"model": "opus", "hooks": {"Stop": [FOREIGN_STOP]}, "z": 1.5}},
     ["install", "install", "uninstall"], [".claude/settings.json"], True),
    ("claude_win32_empty", "claude", "win32", {}, {".claude/settings.json": {}},
     ["install", "uninstall"], [".claude/settings.json"], True),
    ("claude_bad_json", "claude", "darwin", {}, {".claude/settings.json": "{not json"},
     ["install", "uninstall"], [".claude/settings.json"], True),
    ("codex_darwin_foreign", "codex", "darwin", {}, {".codex/hooks.json": {"hooks": {"PostToolUse": [FOREIGN_STOP]}}},
     ["install", "install", "uninstall"], [".codex/hooks.json"], False),
    ("codex_win32_codex_home", "codex", "win32", {"CODEX_HOME": "<home>/alt-codex"}, {},
     ["install", "uninstall"], ["alt-codex/hooks.json", ".codex/hooks.json"], False),
    ("gemini_darwin_foreign", "gemini", "darwin", {},
     {".gemini/settings.json": {"theme": "x", "hooks": {"BeforeTool": [FOREIGN_STOP]}}},
     ["install", "install", "uninstall"], [".gemini/settings.json"], False),
    ("gemini_win32", "gemini", "win32", {}, {".gemini/settings.json": {"theme": "x"}},
     ["install", "uninstall"], [".gemini/settings.json"], False),
    ("cursor_darwin_foreign", "cursor", "darwin", {},
     {".cursor/hooks.json": {"version": 1, "hooks": {"afterFileEdit": [{"command": "./format.sh"}]}}},
     ["install", "install", "uninstall"], [".cursor/hooks.json"], False),
    ("cursor_win32_no_version", "cursor", "win32", {}, {".cursor/hooks.json": {"hooks": {}}},
     ["install", "uninstall"], [".cursor/hooks.json"], False),
    ("cursor_missing_file", "cursor", "darwin", {}, {}, ["uninstall", "install"], [".cursor/hooks.json"], False),
    ("copilot_darwin", "copilot", "darwin", {}, {}, ["install", "install", "uninstall"],
     [".copilot/hooks/clawd.json"], False),
    ("copilot_win32_home_env", "copilot", "win32", {"COPILOT_HOME": "<home>/alt-copilot"}, {},
     ["install", "uninstall"], ["alt-copilot/hooks/clawd.json"], False),
    ("copilot_foreign_file", "copilot", "darwin", {},
     {".copilot/hooks/clawd.json": {"version": 1, "hooks": {"Stop": [{"type": "command", "bash": "mine.sh"}]}}},
     ["install", "uninstall"], [".copilot/hooks/clawd.json"], False),
    ("copilot_stale_backups", "copilot", "darwin", {},
     {".copilot/hooks/clawd.json.bak-clawd-200101-000000": "{}"}, ["install", "install", "uninstall"],
     [".copilot/hooks/clawd.json"], False),
    ("pi_darwin", "pi", "darwin", {}, {}, ["install", "install", "uninstall", "uninstall"],
     [".pi/agent/extensions/clawd-home.ts"], False),
    ("pi_env_dir", "pi", "win32", {"PI_CODING_AGENT_DIR": "<home>/alt-pi"}, {},
     ["install", "uninstall"], ["alt-pi/extensions/clawd-home.ts"], False),
    ("pi_foreign_file", "pi", "darwin", {}, {".pi/agent/extensions/clawd-home.ts": "export default {}\n"},
     ["install", "uninstall"], [".pi/agent/extensions/clawd-home.ts"], False),
    ("pi_older_copy", "pi", "darwin", {},
     {".pi/agent/extensions/clawd-home.ts": "// pi extension: report agent lifecycle to the clawd-home daemon\nold\n"},
     ["refresh", "refresh"], [".pi/agent/extensions/clawd-home.ts"], False),
    ("opencode_darwin", "opencode", "darwin", {}, {}, ["install", "install", "uninstall", "uninstall"],
     [".config/opencode/plugins/clawd.js"], False),
    ("opencode_xdg", "opencode", "win32", {"XDG_CONFIG_HOME": "<home>/xdg"}, {},
     ["install", "uninstall"], ["xdg/opencode/plugins/clawd.js"], False),
    ("opencode_foreign_file", "opencode", "darwin", {}, {".config/opencode/plugins/clawd.js": "export {}\n"},
     ["install", "uninstall"], [".config/opencode/plugins/clawd.js"], False),
]


def bundled_text(rel: str) -> str:
    return h.REPO.joinpath("integrations", *rel.split("/")).read_text(encoding="utf-8")


def asset_text(rel: str) -> str:
    return (h.REPO / "desktop-rs" / "crates" / "clawd-agents" / "assets" / rel).read_text(encoding="utf-8")


def write_initial(initial: dict) -> None:
    for rel, content in initial.items():
        f = ctx.home / rel
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(content if isinstance(content, str) else json.dumps(content, indent=2), encoding="utf-8")


def run_step(agent, step: str):
    try:
        if step == "refresh":
            return {"refreshed": agent.refresh()}
        return {"message": getattr(agent, step)()}
    except AgentError as e:
        return {"error": str(e)}


def export_configs() -> None:
    cases_in, cases_out = [], []
    for name, aid, platform, env, initial, steps, watched, other in CONFIG_CASES:
        fresh_home()
        write_initial(initial)
        agent = agents.registry(home=ctx.home, platform=platform, port=PORT, which=lambda _n: None,
                                env=expand(env))[aid]
        outs = []
        for step in steps:
            res = run_step(agent, step)
            outs.append({"step": step, **norm(res), "installed": _installed(agent), "files": snapshot(watched)})
        if aid == "copilot" and name == "copilot_stale_backups":
            outs[-1]["stale_backup_left"] = bool(list((ctx.home / ".copilot" / "hooks").glob("*.bak-clawd-*")))
        info = agent.info()
        cases_in.append({"name": name, "agent": aid, "platform": platform, "env": env, "initial": initial,
                         "steps": steps, "watched": watched, "needs_other_phases": other})
        cases_out.append({"name": name, "steps": outs, "info": norm(info)})
    plugin_checks = {rel: bundled_text(rel) == asset_text(rel).replace("\r\n", "\n")
                     for rel in ("pi/clawd-home.ts", "opencode/clawd.js")}
    h.write_fixture("agents", "configs", input={"port": PORT, "cases": cases_in},
                    output={"cases": cases_out, "assets_equal_integrations": plugin_checks}, ctx=ctx)


def _installed(agent):
    try:
        return agent.is_installed()
    except AgentError as e:
        return {"error": norm(str(e))}


# ---- normalize ------------------------------------------------------------------------
CLAUDE_PAYLOADS = [
    ("", {"hook_event_name": "UserPromptSubmit", "session_id": "s1", "prompt": "hello"}),
    ("", {"hook_event_name": "PreToolUse", "session_id": "s1", "tool_name": "Bash",
          "tool_input": {"command": "ls"}}),
    ("", {"hook_event_name": "Stop", "session_id": "s1"}),
    ("", {"hook_event_name": "Stop"}),
    ("Stop", [1, 2]),
]
PLUGIN_PAYLOADS = [
    ("", {"ev": "prompt", "sid": "abc"}),
    ("", {"ev": "tool", "sid": "abc", "tool": "bash", "cmd": "ls"}),
    ("", {"ev": "stop", "sid": 7}),
    ("", {"ev": "nonsense", "sid": "abc"}),
    ("", {"sid": "abc"}),
    ("", "text"),
]


def sid(n):
    return {"session_id": f"{n}"}


COMMON = {
    "codex": [
        ("", {**sid("c1"), "hook_event_name": "SessionStart"}),
        ("", {**sid("c1"), "hook_event_name": "UserPromptSubmit", "prompt": "fix the bug"}),
        ("", {**sid("c1"), "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "x" * 400}}),
        ("", {**sid("c1"), "hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion"}),
        ("", {**sid("c1"), "hook_event_name": "PostToolUse", "tool_name": "Bash"}),
        ("", {**sid("c1"), "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": {"command": "rm -rf x"}}),
        ("Stop", sid("c1")),
        ("Interrupt", sid("c1")),
        ("", {**sid("c1"), "hook_event_name": "SubagentStop"}),
        ("", {**sid("c1"), "hook_event_name": "PostCompact"}),
        ("", {**sid("c1"), "hook_event_name": "Unknown"}),
        ("Stop", {"hook_event_name": "Stop"}),
        ("", "not a dict"),
    ],
    "gemini": [
        ("", {**sid("g1"), "hook_event_name": "BeforeAgent", "prompt": "hi"}),
        ("", {**sid("g1"), "hook_event_name": "BeforeTool", "tool_name": "run_shell_command", "tool_input": {"command": "ls"}}),
        ("", {**sid("g1"), "hook_event_name": "AfterTool", "tool_response": {"error": "boom"}}),
        ("", {**sid("g1"), "hook_event_name": "AfterTool", "tool_response": {"llmContent": "ok"}}),
        ("", {**sid("g1"), "hook_event_name": "AfterTool", "tool_response": "text"}),
        ("", {**sid("g1"), "hook_event_name": "AfterAgent"}),
        ("", {**sid("g1"), "hook_event_name": "PreCompress"}),
        ("", {**sid("g1"), "hook_event_name": "Notification", "notification_type": "ToolPermission", "message": "ok?"}),
        ("", {**sid("g1"), "hook_event_name": "Notification", "notification_type": "ToolPermission"}),
        ("", {**sid("g1"), "hook_event_name": "Notification", "notification_type": "Other"}),
        ("SessionEnd", sid("g1")),
        ("", {"hook_event_name": "AfterAgent"}),
    ],
    "cursor": [
        ("", {"conversation_id": "k1", "hook_event_name": "sessionStart", "is_background_agent": True}),
        ("", {"conversation_id": "k1", "hook_event_name": "sessionStart"}),
        ("", {"conversation_id": "k1", "hook_event_name": "beforeSubmitPrompt", "prompt": "debug this"}),
        ("", {"conversation_id": "k1", "hook_event_name": "postToolUse", "tool_name": "MCP:search", "tool_input": {"command": "q"}}),
        ("", {"session_id": "k2", "hook_event_name": "postToolUseFailure", "is_interrupt": True}),
        ("", {"conversation_id": "k1", "hook_event_name": "postToolUseFailure"}),
        ("", {"conversation_id": "k1", "hook_event_name": "preCompact"}),
        ("", {"conversation_id": "k1", "hook_event_name": "stop", "status": "aborted"}),
        ("", {"conversation_id": "k1", "hook_event_name": "stop", "status": "error"}),
        ("", {"conversation_id": "k1", "hook_event_name": "stop"}),
        ("", {"conversation_id": "k1", "hook_event_name": "stop", "status": "weird"}),
        ("sessionEnd", {"conversation_id": "k1"}),
        ("", {"hook_event_name": "stop"}),
    ],
    "copilot": [
        ("", {**sid("p1"), "hook_event_name": "SessionStart"}),
        ("", {"sessionId": "p2", "hook_event_name": "subagentStart"}),
        ("", {**sid("p1"), "hook_event_name": "UserPromptSubmit", "prompt": "fix"}),
        ("", {**sid("p1"), "hook_event_name": "PreToolUse", "tool_name": "bash", "tool_input": {"command": "ls"}}),
        ("", {"sessionId": "p2", "hook_event_name": "PreToolUse", "toolName": "bash", "toolArgs": {"command": "pwd"}}),
        ("", {**sid("p1"), "hook_event_name": "PostToolUseFailure"}),
        ("", {**sid("p1"), "hook_event_name": "ErrorOccurred"}),
        ("", {**sid("p1"), "hook_event_name": "ErrorOccurred", "recoverable": False}),
        ("", {**sid("p1"), "hook_event_name": "ErrorOccurred", "recoverable": True}),
        ("", {**sid("p1"), "hook_event_name": "notification", "notification_type": "permission_prompt", "message": "m"}),
        ("", {"sessionId": "p2", "hook_event_name": "notification", "notificationType": "Elicitation_dialog"}),
        ("", {**sid("p1"), "hook_event_name": "notification", "notification_type": "info"}),
        ("PreCompact", sid("p1")),
        ("", {"hook_event_name": "Stop"}),
    ],
}
NEEDS_EVENTS_NAMES = {"UserPromptSubmit", "PreToolUse", "BeforeAgent", "BeforeTool",
                      "beforeSubmitPrompt", "PermissionRequest"}


def export_normalize() -> None:
    cases, outs = [], []

    def add(aid, hint, payload, other):
        cases.append({"agent": aid, "hint": hint, "payload": payload, "needs_other_phases": other})
        outs.append(agents.normalize(aid, payload, hint))

    for aid, items in COMMON.items():
        for hint, payload in items:
            name = payload.get("hook_event_name", "") if isinstance(payload, dict) else ""
            add(aid, hint, payload, (name or hint) in NEEDS_EVENTS_NAMES)
    for hint, payload in CLAUDE_PAYLOADS:
        add("claude", hint, payload, True)
    for aid in ("pi", "opencode"):
        for hint, payload in PLUGIN_PAYLOADS:
            add(aid, hint, payload, True)
    add("nobody", "", {"session_id": "x"}, False)
    h.write_fixture("agents", "normalize", input={"cases": cases}, output={"events": outs}, ctx=ctx)


def export_registry() -> None:
    fresh_home()
    reg = agents.registry(home=ctx.home, platform="win32", port=PORT, which=lambda _n: None, env={})
    (ctx.home / ".gemini").mkdir()
    (ctx.home / ".cursor").mkdir()
    h.write_fixture(
        "agents", "registry",
        input={"platform": "win32", "port": PORT, "dirs": [".gemini", ".cursor"]},
        output={"ids": list(agents.IDS), "found": agents.found_ids(reg),
                "infos": norm([a.info() for a in reg.values() if a.id != "claude"])},
        ctx=ctx)


if __name__ == "__main__":
    export_hooks()
    export_configs()
    export_normalize()
    export_registry()
    print("agents fixtures written")
