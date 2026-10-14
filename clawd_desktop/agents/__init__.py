"""The coding agents Clawd can listen to, one adapter each (see base.py).

    from clawd_desktop import agents
    agents.registry()["codex"].install()
    agents.normalize("codex", payload, hint)      # the event server's /hook/<id>

Adding an agent = one module with an Agent subclass, listed in ORDER.
"""
from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

from clawd_desktop.agents.base import Agent, AgentError
from clawd_desktop.agents.claude import Claude
from clawd_desktop.agents.codex import Codex
from clawd_desktop.agents.copilot import Copilot
from clawd_desktop.agents.cursor import Cursor
from clawd_desktop.agents.gemini import Gemini
from clawd_desktop.agents.opencode import OpenCode
from clawd_desktop.agents.pi import Pi

ORDER: tuple[type[Agent], ...] = (Claude, Pi, Codex, Gemini, Cursor, OpenCode, Copilot)
IDS = tuple(a.id for a in ORDER)

_DEFAULT: dict[str, Agent] | None = None


def registry(home: Path | None = None, platform: str = sys.platform, **kw: Any) -> dict[str, Agent]:
    """id -> adapter, in display order. With no arguments: the shared real-machine one."""
    global _DEFAULT
    if home is None and platform == sys.platform and not kw:
        if _DEFAULT is None:
            _DEFAULT = {a.id: a() for a in ORDER}
        return _DEFAULT
    return {a.id: a(home=home, platform=platform, **kw) for a in ORDER}


def normalize(agent_id: str, payload: Any, hint: str = "") -> list[dict] | None:
    """Tracker events for one hook payload; None for an agent id we do not know."""
    a = registry().get(agent_id)
    if a is None:
        return None
    try:
        return a.normalize(payload, hint)
    except Exception:                      # a payload shape we did not expect: drop it, never 500
        return []


def infos(reg: dict[str, Agent] | None = None) -> list[dict]:
    return [a.info() for a in (reg or registry()).values()]


def found_ids(reg: dict[str, Agent] | None = None) -> list[str]:
    return [i for i, a in (reg or registry()).items() if a.found()]


def uninstall_all(reg: dict[str, Agent] | None = None) -> list[str]:
    """Remove every hook / plugin Clawd added (the uninstaller, "Ngắt kết nối tất cả")."""
    out = []
    for a in (reg or registry()).values():
        try:
            out.append(f"{a.id}: {a.uninstall()}")
        except (AgentError, OSError) as e:
            out.append(f"{a.id}: {e}")
    return out


def refresh_plugins(reg: dict[str, Agent] | None = None) -> list[str]:
    """Replace plugin copies older than the app's (pi, opencode). Ids that were updated."""
    out = []
    for a in (reg or registry()).values():
        refresh = getattr(a, "refresh", None)
        if refresh is not None and refresh():
            out.append(a.id)
    return out


__all__ = ["Agent", "AgentError", "IDS", "ORDER", "registry", "normalize", "infos", "found_ids",
           "uninstall_all", "refresh_plugins"]
