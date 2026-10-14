"""Gemini CLI: command hooks in ~/.gemini/settings.json.

Format and events: https://geminicli.com/docs/hooks/reference/ . Every payload has
session_id and hook_event_name. Hooks run in line, so ours give up within 0.3 s
and print nothing (empty stdout = allow). ~/.gemini alone is not enough to say
Gemini CLI is here: the Antigravity IDE makes that folder too.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from clawd_desktop import events
from clawd_desktop.agents.base import (Agent, base_event, edit_json, first_str, has_ours, load,
                                       merge_groups)

SIMPLE = {"SessionStart": "start", "AfterAgent": "stop", "PreCompress": "compact",
          "SessionEnd": "end"}
EVENTS = ("SessionStart", "BeforeAgent", "BeforeTool", "AfterTool", "AfterAgent", "PreCompress",
          "Notification", "SessionEnd")


class Gemini(Agent):
    id = "gemini"
    name = "Gemini CLI"
    signals = ("think", "tool", "error", "attention")
    tested: frozenset[str] = frozenset()
    note = "Khởi động lại Gemini CLI sau khi nối."

    def config_dir(self) -> Path:
        return self.home / ".gemini"

    def file(self) -> Path:
        return self.config_dir() / "settings.json"

    def found(self) -> bool:
        d = self.config_dir()
        return (self._which("gemini") is not None or (d / "settings.json").exists()
                or (d / "oauth_creds.json").exists())

    def handler(self, event: str) -> dict:
        return {"name": f"clawd-{event}", "type": "command", "command": self.command(event),
                "timeout": 2000}

    def is_installed(self) -> bool:
        return self.file().exists() and has_ours(load(self.file()), self.ours)

    def install(self) -> str:
        self.prepare()
        add = {e: self.handler(e) for e in EVENTS}
        edit_json(self.file(), lambda d: merge_groups(d, add, self.ours, matcher="*"))
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
        ev = base_event("gemini", sid, tool, inp)
        if name == "BeforeAgent":
            return [{**ev, "ev": "prompt", **({"debug": True} if events.is_debug_prompt(p.get("prompt")) else {})}]
        if name in SIMPLE:
            return [{**ev, "ev": SIMPLE[name]}]
        if name == "BeforeTool":
            return [{**ev, "ev": "attention" if events.is_question_tool(tool) else "tool"}]
        if name == "AfterTool":
            resp = p.get("tool_response")
            failed = isinstance(resp, dict) and bool(resp.get("error"))
            return [{**ev, "ev": "tool_error" if failed else "tool_done"}]
        if name == "Notification":
            if str(p.get("notification_type") or "") != "ToolPermission":
                return []
            return [{**ev, "ev": "attention", "msg": str(p.get("message") or "Gemini needs you")}]
        return []
