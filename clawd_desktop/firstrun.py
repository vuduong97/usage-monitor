"""The setup wizard's bookkeeping, kept in desktop.json (paths.load_prefs).

- setup_done: the wizard steps finished or skipped ("clock", "agents", "usage").
  A fresh install has none, so it sees every step. 0.1.x had only "first_run_done"
  (its users already found the clock), so an upgrade sees only the new steps.
- setup_shown: the app version that last opened the wizard by itself, so it opens
  once per version while steps are left; "Hoàn tất thiết lập…" in the tray and the
  settings window open it any time.
- agents_seen: agents already offered to the user (in the wizard, the Agent tab or
  the tray). A newly installed agent is offered once; never again once seen, whatever
  the answer.
"""
from __future__ import annotations

from typing import Iterable

from clawd_desktop import __version__, paths

STEPS = ("clock", "agents", "usage")


def migrate(prefs: dict) -> dict:
    """Prefs with setup_done, from 0.1.x's first_run_done when that is all there is."""
    if isinstance(prefs.get("setup_done"), list):
        return prefs
    out = dict(prefs)
    out["setup_done"] = ["clock"] if prefs.get("first_run_done") else []
    return out


def load() -> dict:
    prefs = paths.load_prefs()
    new = migrate(prefs)
    if new is not prefs:
        paths.save_prefs(new)
    return new


def pending(prefs: dict | None = None) -> list[str]:
    done = set(migrate(prefs if prefs is not None else paths.load_prefs())["setup_done"])
    return [s for s in STEPS if s not in done]


def mark_done(steps: Iterable[str]) -> list[str]:
    prefs = load()
    done = list(prefs["setup_done"])
    for s in steps:
        if s in STEPS and s not in done:
            done.append(s)
    prefs["setup_done"] = done
    paths.save_prefs(prefs)
    return pending(prefs)


def should_open(prefs: dict | None = None) -> bool:
    """Open the wizard by itself at this start: steps left and not yet shown by this version."""
    prefs = migrate(prefs if prefs is not None else paths.load_prefs())
    return bool(pending(prefs)) and prefs.get("setup_shown") != __version__


def mark_shown() -> None:
    prefs = load()
    prefs["setup_shown"] = __version__
    paths.save_prefs(prefs)


def seen() -> set[str]:
    return {str(x) for x in paths.load_prefs().get("agents_seen") or []}


def mark_seen(ids: Iterable[str]) -> None:
    prefs = load()
    now = sorted(seen() | set(ids))
    if now != sorted(prefs.get("agents_seen") or []):
        prefs["agents_seen"] = now
        paths.save_prefs(prefs)


def new_agents(found: Iterable[str], installed: Iterable[str]) -> list[str]:
    """Agents on this machine never offered before and not connected yet."""
    skip = seen() | set(installed)
    return [i for i in found if i not in skip]
