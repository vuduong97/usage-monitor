"""Isolation + fixture writer for the golden exporters (`tools/golden/export_<area>.py`).

An exporter runs the REAL Python app (`clawd_desktop`) and records `{input, output}` JSON that the
Rust tests must reproduce. It must never read or write the developer's real home, keychain or
OS stores (red team #14), so `isolate()`:

1. computes the real home, then points HOME / USERPROFILE / CODEX_HOME / XDG_CONFIG_HOME ... at a
   temp dir BEFORE `clawd_desktop` is imported (its path constants are computed at import);
2. monkeypatches `Path.home` and rebases every `Path` constant of every `clawd_desktop` module
   that still points into the real home (belt and braces: `usage.CONFIG_FILE`, ...);
3. replaces the keyring with an in-memory one (`creds._keyring`);
4. installs an audit hook: any open/list/mkdir/remove/rename under the real home (other than the
   interpreter, site-packages, this repo and the temp dir) raises `RealHomeAccess`.

Fixtures: sorted keys, stable float repr, `ensure_ascii`, each file < 512 KB, no binary (use
`sha256_of` and store the digest), no absolute home paths (use `rel`).

    from tools.golden import _harness as h
    ctx = h.isolate()
    from clawd_desktop import paths
    ...
    h.write_fixture("store", "prefs_roundtrip", input=..., output=...)
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import site
import sys
import sysconfig
import tempfile
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[2]
FIXTURES = REPO / "desktop-rs" / "fixtures"
MAX_FIXTURE_BYTES = 512_000          # check_public.py MAX_BYTES; stay under it
HOME_ENV = ("HOME", "USERPROFILE", "CODEX_HOME", "COPILOT_HOME", "XDG_CONFIG_HOME",
            "PI_CODING_AGENT_DIR", "APPDATA", "LOCALAPPDATA")
AUDIT_EVENTS = {"open", "os.listdir", "os.scandir", "os.mkdir", "os.remove", "os.rename",
                "os.rmdir", "shutil.copyfile", "shutil.rmtree"}


class RealHomeAccess(AssertionError):
    """An exporter touched the real home directory."""


def _norm(p: str | os.PathLike[str]) -> str:
    return os.path.normcase(os.path.realpath(os.fspath(p)))


# Computed once, at import, before any isolation: the user's real home.
REAL_HOME = _norm(Path.home())
_ALLOWED: list[str] = []          # extra roots that may be touched (interpreter, repo, temp)
_STATE: dict[str, Any] = {"root": None, "installed": False}


def _under(path: str, root: str) -> bool:
    return path == root or path.startswith(root.rstrip(os.sep) + os.sep)


def _audit(event: str, args: tuple) -> None:
    if event not in AUDIT_EVENTS or not args:
        return
    target = args[0]
    if isinstance(target, int):              # a file descriptor
        return
    if isinstance(target, bytes):
        target = os.fsdecode(target)
    if not isinstance(target, (str, os.PathLike)) or target in ("", "."):
        return
    try:
        path = _norm(target)
    except (OSError, ValueError):
        return
    if _under(path, REAL_HOME) and not any(_under(path, root) for root in _ALLOWED):
        raise RealHomeAccess(f"{event} touched the real home: {path}")


def _allowed_roots(temp_root: str) -> list[str]:
    roots = {sys.prefix, sys.base_prefix, sys.exec_prefix, sys.base_exec_prefix, str(REPO), temp_root,
             tempfile.gettempdir()}
    roots.update(sysconfig.get_paths().values())
    roots.update(site.getsitepackages())
    roots.update(p for p in sys.path if p)
    return sorted({_norm(r) for r in roots if r})


class _MemKeyring:
    """Stands in for `keyring` (`creds._keyring()`): a dict, nothing leaves the process."""

    def __init__(self) -> None:
        self.store: dict[tuple[str, str], str] = {}

    def get_password(self, service: str, user: str) -> str | None:
        return self.store.get((service, user))

    def set_password(self, service: str, user: str, password: str) -> None:
        self.store[(service, user)] = password

    def delete_password(self, service: str, user: str) -> None:
        if (service, user) not in self.store:
            raise KeyError(user)
        del self.store[(service, user)]


class Isolation:
    """What `isolate()` set up."""

    def __init__(self, root: Path, keyring: _MemKeyring) -> None:
        self.root = root
        self.home = root / "home"
        self.keyring = keyring

    @property
    def config_dir(self) -> Path:
        return self.home / ".config" / "claude-usage-monitor"

    def rel(self, value: Any) -> Any:
        """Replace the temp home in a path/string by `<home>`; recurse into dicts and lists."""
        return rel(self, value)


def rel(ctx: Isolation, value: Any) -> Any:
    if isinstance(value, dict):
        return {k: rel(ctx, v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [rel(ctx, v) for v in value]
    if isinstance(value, os.PathLike):
        value = os.fspath(value)
    if isinstance(value, str):
        for form in (str(ctx.home), ctx.home.as_posix()):
            value = value.replace(form, "<home>")
        for form in (str(ctx.root), ctx.root.as_posix()):
            value = value.replace(form, "<tmp>")
        return value
    return value


def isolate() -> Isolation:
    """Call first, before importing `clawd_desktop`."""
    if _STATE["installed"]:
        return _STATE["ctx"]
    if "clawd_desktop" in sys.modules:
        raise RuntimeError("call isolate() before importing clawd_desktop")
    root = Path(tempfile.mkdtemp(prefix="clawd-golden-")).resolve()
    home = root / "home"
    home.mkdir()
    for var in HOME_ENV:
        os.environ.pop(var, None)
    os.environ["HOME"] = os.environ["USERPROFILE"] = str(home)
    os.environ["XDG_CONFIG_HOME"] = str(home / ".config")
    os.environ["CODEX_HOME"] = str(home / ".codex")
    if os.name == "nt":
        os.environ["APPDATA"] = str(home / "AppData" / "Roaming")
        os.environ["LOCALAPPDATA"] = str(home / "AppData" / "Local")
        os.environ["HOMEDRIVE"], os.environ["HOMEPATH"] = home.drive, str(home)[len(home.drive):]
    Path.home = classmethod(lambda cls: home)               # type: ignore[method-assign]
    os.path.expanduser = _expanduser_factory(home)           # type: ignore[assignment]
    _ALLOWED[:] = _allowed_roots(_norm(root))
    sys.path.insert(0, str(REPO))
    if not _STATE["installed"]:
        sys.addaudithook(_audit)
        _STATE["installed"] = True
    ctx = Isolation(root, _MemKeyring())
    _STATE["ctx"], _STATE["root"] = ctx, root
    import clawd_desktop                                    # noqa: F401  (after the env is set)
    _patch_modules(ctx, home)
    return ctx


def _expanduser_factory(home: Path):
    original = os.path.expanduser

    def expanduser(path):                                    # noqa: ANN001
        text = os.fspath(path)
        if text == "~" or text.startswith(("~/", "~\\")):
            return str(home) + text[1:]
        return original(text)
    return expanduser


def _patch_modules(ctx: Isolation, home: Path) -> None:
    """Rebase any Path constant still pointing into the real home; in-memory keyring."""
    import importlib
    from clawd_desktop import creds
    for name in list(sys.modules):
        if not name.startswith("clawd_desktop"):
            continue
        mod = sys.modules[name]
        for attr, val in list(vars(mod).items()):
            if isinstance(val, Path) and _under(_norm(val), REAL_HOME) and not _under(_norm(val), _norm(home)):
                tail = os.path.relpath(_norm(val), REAL_HOME)
                setattr(mod, attr, home / tail)
    creds._keyring = lambda: ctx.keyring                     # type: ignore[assignment]
    importlib.invalidate_caches()


def sha256_of(data: bytes) -> str:
    """Binary data is stored as its digest, never as bytes/base64 (check_public bans art)."""
    return hashlib.sha256(data).hexdigest()


_FORBIDDEN = (re.compile(r"iVBORw0KGgo"), re.compile(r"CLWD"), re.compile(r"/Users/"),
              re.compile(r"[A-Za-z]:\\{1,2}Users\\"))


def write_fixture(area: str, name: str, *, input: Any, output: Any, ctx: Isolation | None = None) -> Path:
    """Write `desktop-rs/fixtures/<area>/<name>.json` as `{input, output}`."""
    ctx = ctx or _STATE.get("ctx")
    data = {"input": input, "output": output}
    if ctx is not None:
        data = ctx.rel(data)
    text = json.dumps(data, sort_keys=True, indent=2, ensure_ascii=True) + "\n"
    if ctx is not None and (str(ctx.root) in text or ctx.root.as_posix() in text):
        raise ValueError("fixture contains the temp dir path: use ctx.rel()")
    for rx in _FORBIDDEN:
        if rx.search(text):
            raise ValueError(f"fixture {area}/{name} matches forbidden pattern {rx.pattern!r}")
    if len(text.encode()) >= MAX_FIXTURE_BYTES:
        raise ValueError(f"fixture {area}/{name} is {len(text)} bytes (limit {MAX_FIXTURE_BYTES})")
    path = FIXTURES / area / f"{name}.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")
    return path


def selftest() -> None:
    """The audit hook must stop a read of the real home (and allow the temp dir)."""
    ctx = isolate()
    (ctx.home / "ok.txt").write_text("fine")
    probe = Path(REAL_HOME) / ".config" / "claude-usage-monitor" / "desktop.json"
    try:
        open(probe)
    except RealHomeAccess:
        print("selftest: real-home access blocked")
        return
    except OSError:
        pass
    raise SystemExit("selftest FAILED: the audit hook did not block the real home")


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        selftest()
    else:
        print(__doc__)
