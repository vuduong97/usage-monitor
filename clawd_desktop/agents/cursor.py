"""Cursor (the editor's agent and the cursor-agent CLI): ~/.cursor/hooks.json.

Format and events: https://cursor.com/docs/hooks . Only observing hooks are used.
On Cursor's permission hooks (preToolUse, beforeShellExecution, beforeReadFile,
beforeMCPExecution, subagentStart) an empty or non-JSON answer BLOCKS the action,
so a hook that just reports would stop the agent whenever Clawd is not running.
Hence no "tool started": a tool shows once it is done (postToolUse), and no
subagent count (subagentStart is a permission hook).

Cursor also runs Claude Code's hooks from ~/.claude/settings.json (Settings ->
Agents -> Third-Party Imports, on by default); events.claude_event drops those
Cursor payloads, so a Cursor chat never counts twice.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from clawd_desktop import events
from clawd_desktop.agents.base import (Agent, base_event, edit_json, first_str, has_ours, load,
                                       merge_flat)

EVENTS = ("sessionStart", "beforeSubmitPrompt", "postToolUse", "postToolUseFailure", "preCompact",
          "stop", "sessionEnd")
STOP = {"completed": "stop", "aborted": "abort", "error": "stop_error"}


def _tool(raw: Any) -> str:
    t = raw if isinstance(raw, str) else ""
    return t[4:] if t.startswith("MCP:") else t


class Cursor(Agent):
    id = "cursor"
    name = "Cursor"
    signals = ("think", "tool", "error")
    tested: frozenset[str] = frozenset()

    def config_dir(self) -> Path:
        return self.home / ".cursor"

    def file(self) -> Path:
        return self.config_dir() / "hooks.json"

    def handler(self, event: str) -> dict:
        return {"command": self.command(event), "timeout": 5}

    def is_installed(self) -> bool:
        return self.file().exists() and has_ours(load(self.file()), self.ours)

    def install(self) -> str:
        self.prepare()
        add = {e: self.handler(e) for e in EVENTS}

        def change(d: dict) -> dict:
            out = merge_flat(d, add, self.ours)
            out.setdefault("version", 1)
            return out
        edit_json(self.file(), change)
        return "Đã nối."

    def uninstall(self) -> str:
        edit_json(self.file(), lambda d: merge_flat(d, None, self.ours), create=False)
        return "Đã ngắt."

    def normalize(self, p: Any, hint: str = "") -> list[dict]:
        if not isinstance(p, dict):
            return []
        name = first_str(p, "hook_event_name") or hint
        sid = first_str(p, "conversation_id", "session_id")
        if not sid:
            return []
        tool = _tool(p.get("tool_name"))
        ev = base_event("cursor", sid, tool, p.get("tool_input"))
        if name == "sessionStart":
            return [{**ev, "ev": "start", "sub": bool(p.get("is_background_agent"))}]
        if name == "beforeSubmitPrompt":
            return [{**ev, "ev": "prompt", **({"debug": True} if events.is_debug_prompt(p.get("prompt")) else {})}]
        if name == "postToolUse":
            return [{**ev, "ev": "tool"}, {**ev, "ev": "tool_done"}]
        if name == "postToolUseFailure":
            return [{**ev, "ev": "abort" if p.get("is_interrupt") else "tool_error"}]
        if name == "preCompact":
            return [{**ev, "ev": "compact"}]
        if name == "stop":
            return [{**ev, "ev": STOP.get(str(p.get("status") or "completed"), "stop")}]
        if name == "sessionEnd":
            return [{**ev, "ev": "end"}]
        return []
