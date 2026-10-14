"""Codex CLI: command hooks in ~/.codex/hooks.json (CODEX_HOME moves it).

Format and events: https://developers.openai.com/codex/hooks . Payloads are
Claude-like (session_id, hook_event_name, tool_name, tool_input). Subagent hooks
carry the parent's session id. Codex runs a new or changed hook only after the
user trusts it in `/hooks`, hence the note. Hooks run in the background
(`async`), so they can never hold up a tool call.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from clawd_desktop import events
from clawd_desktop.agents.base import (Agent, base_event, edit_json, first_str, has_ours, load,
                                       merge_groups, tool_summary)

SIMPLE = {"SessionStart": "start", "PostToolUse": "tool_done", "PreCompact": "compact",
          "PostCompact": "compact_done", "SubagentStart": "subagent_start",
          "SubagentStop": "subagent_stop", "Stop": "stop", "Interrupt": "abort", "SessionEnd": "end"}
EVENTS = ("SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "PostToolUse",
          "PreCompact", "PostCompact", "SubagentStart", "SubagentStop", "Stop", "Interrupt",
          "SessionEnd")
SHORT = {"SessionEnd", "Interrupt"}          # Codex caps these at 1-3 s and runs SessionEnd in line


class Codex(Agent):
    id = "codex"
    name = "Codex CLI"
    signals = ("think", "tool", "subagent", "attention")
    tested: frozenset[str] = frozenset()
    note = "Mở Codex, gõ /hooks và chọn tin cậy các hook của Clawd (một lần)."

    def config_dir(self) -> Path:
        return self.env_dir("CODEX_HOME", self.home / ".codex")

    def file(self) -> Path:
        return self.config_dir() / "hooks.json"

    def handler(self, event: str) -> dict:
        h = {"type": "command", "command": self.posix_command(event),
             "commandWindows": self.windows_command(event), "timeout": 2 if event in SHORT else 5}
        if event != "SessionEnd":
            h["async"] = True
        return h

    def is_installed(self) -> bool:
        return self.file().exists() and has_ours(load(self.file()), self.ours)

    def install(self) -> str:
        self.prepare()
        add = {e: self.handler(e) for e in EVENTS}
        edit_json(self.file(), lambda d: merge_groups(d, add, self.ours))
        return "Đã nối. " + self.note

    def uninstall(self) -> str:
        edit_json(self.file(), lambda d: merge_groups(d, None, self.ours), create=False)
        return "Đã ngắt."

    def normalize(self, p: Any, hint: str = "") -> list[dict]:
        if not isinstance(p, dict):
            return []
        name = first_str(p, "hook_event_name") or hint
        sid = first_str(p, "session_id")
        if not sid:
            return []
        tool, inp = p.get("tool_name"), p.get("tool_input")
        ev = base_event("codex", sid, tool, inp)
        if name == "UserPromptSubmit":
            return [{**ev, "ev": "prompt", **({"debug": True} if events.is_debug_prompt(p.get("prompt")) else {})}]
        if name in SIMPLE:
            return [{**ev, "ev": SIMPLE[name]}]
        if name == "PreToolUse":
            return [{**ev, "ev": "attention" if events.is_question_tool(tool) else "tool"}]
        if name == "PermissionRequest":
            return [{**ev, "ev": "attention", "msg": "Allow " + tool_summary(ev["tool"], inp)}]
        return []
