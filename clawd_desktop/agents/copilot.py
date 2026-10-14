"""GitHub Copilot CLI: our own hook file, ~/.copilot/hooks/clawd.json (COPILOT_HOME moves it).

Format and events: https://docs.github.com/en/copilot/reference/hooks-reference .
Copilot reads every *.json in that folder, so ours is a file of its own: nothing of
the user's is ever edited. PascalCase event names select the payload format with
snake_case fields (session_id, tool_name, tool_input), like Claude Code's;
subagentStart and notification only exist in camelCase (sessionId). A command
hook with no output falls through to Copilot's normal behaviour.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from clawd_desktop import events
from clawd_desktop.agents.base import Agent, AgentError, base_event, first_str, has_ours, read_json, write_json

SIMPLE = {"SessionStart": "start", "PostToolUse": "tool_done", "PostToolUseFailure": "tool_error",
          "Stop": "stop", "SubagentStop": "subagent_stop", "subagentStart": "subagent_start",
          "PreCompact": "compact", "SessionEnd": "end"}
EVENTS = ("SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure",
          "Stop", "subagentStart", "SubagentStop", "PreCompact", "ErrorOccurred", "notification",
          "SessionEnd")


class Copilot(Agent):
    id = "copilot"
    name = "Copilot CLI"
    signals = ("think", "tool", "subagent", "error", "attention")
    tested: frozenset[str] = frozenset()

    def config_dir(self) -> Path:
        return self.env_dir("COPILOT_HOME", self.home / ".copilot")

    def found(self) -> bool:
        return self.config_dir().is_dir() or self._which("copilot") is not None

    def file(self) -> Path:
        return self.config_dir() / "hooks" / "clawd.json"

    def handler(self, event: str) -> dict:
        return {"type": "command", "bash": self.posix_command(event),
                "powershell": self.windows_command(event), "timeoutSec": 5}

    def is_installed(self) -> bool:
        return self.file().exists() and has_ours(read_json(self.file()), self.ours)

    def install(self) -> str:
        self.prepare()
        f = self.file()
        if f.exists() and not self.is_installed():
            raise AgentError(f"{f} đã có và không phải của Clawd, Clawd không ghi đè.")
        data = {"version": 1, "hooks": {e: [self.handler(e)] for e in EVENTS}}
        if read_json(f) != data:
            write_json(f, data)
        return "Đã nối."

    def uninstall(self) -> str:
        f = self.file()
        if self.is_installed():
            f.unlink()
            for bak in f.parent.glob(f"{f.name}.bak-clawd-*"):
                bak.unlink(missing_ok=True)       # Copilot loads every *.json: leave none behind
            return "Đã ngắt."
        return "Chưa nối."

    def normalize(self, p: Any, hint: str = "") -> list[dict]:
        if not isinstance(p, dict):
            return []
        name = first_str(p, "hook_event_name") or hint
        sid = first_str(p, "session_id", "sessionId")
        if not sid:
            return []
        tool = p.get("tool_name") if "tool_name" in p else p.get("toolName")
        inp = p.get("tool_input") if "tool_input" in p else p.get("toolArgs")
        ev = base_event("copilot", sid, tool, inp)
        if name == "UserPromptSubmit":
            return [{**ev, "ev": "prompt", **({"debug": True} if events.is_debug_prompt(p.get("prompt")) else {})}]
        if name in SIMPLE:
            return [{**ev, "ev": SIMPLE[name]}]
        if name == "PreToolUse":
            return [{**ev, "ev": "attention" if events.is_question_tool(tool) else "tool"}]
        if name == "ErrorOccurred":
            return [] if p.get("recoverable", True) else [{**ev, "ev": "stop_error"}]
        if name == "notification":
            kind = str(p.get("notification_type") or p.get("notificationType") or "").lower()
            if "permission" in kind or "elicitation" in kind:
                return [{**ev, "ev": "attention", "msg": str(p.get("message") or "Copilot needs you")}]
        return []
