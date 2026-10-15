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
    if migrate(prefs) is prefs:
        return prefs
    return paths.update_prefs(migrate)


def pending(prefs: dict | None = None) -> list[str]:
    done = set(migrate(prefs if prefs is not None else paths.load_prefs())["setup_done"])
    return [s for s in STEPS if s not in done]


def mark_done(steps: Iterable[str]) -> list[str]:
    steps = list(steps)

    def change(prefs: dict) -> dict:
        prefs = migrate(prefs)
        done = list(prefs["setup_done"])
        done += [s for s in dict.fromkeys(steps) if s in STEPS and s not in done]
        return {**prefs, "setup_done": done}
    return pending(paths.update_prefs(change))


def should_open(prefs: dict | None = None) -> bool:
    """Open the wizard by itself at this start: steps left and not yet shown by this version."""
    prefs = migrate(prefs if prefs is not None else paths.load_prefs())
    return bool(pending(prefs)) and prefs.get("setup_shown") != __version__


def mark_shown() -> None:
    paths.update_prefs(lambda prefs: {**migrate(prefs), "setup_shown": __version__})


def seen() -> set[str]:
    return {str(x) for x in paths.load_prefs().get("agents_seen") or []}


def mark_seen(ids: Iterable[str]) -> None:
    ids = set(ids)

    def change(prefs: dict) -> dict:
        prefs = migrate(prefs)
        had = {str(x) for x in prefs.get("agents_seen") or []}
        if had | ids == had:
            return prefs
        return {**prefs, "agents_seen": sorted(had | ids)}
    paths.update_prefs(change)


def new_agents(found: Iterable[str], installed: Iterable[str]) -> list[str]:
    """Agents on this machine never offered before and not connected yet."""
    skip = seen() | set(installed)
    return [i for i in found if i not in skip]
