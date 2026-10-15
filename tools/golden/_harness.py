"""Isolation + fixture writer for the golden exporters (`tools/golden/export_<area>.py`).

An exporter runs the REAL Python app (`clawd_desktop`) and records `{input, output}` JSON that the
Rust tests must reproduce. It must never read or write the developer's real home, keychain, registry
or network (red team #14), so `isolate()`:

1. computes the real home, clears every `CLAWD_*` variable and points HOME / USERPROFILE /
   CODEX_HOME / XDG_CONFIG_HOME ... at a temp dir BEFORE `clawd_desktop` is imported (its path
   constants are computed at import);
2. monkeypatches `Path.home` and rebases every `Path` constant of every `clawd_desktop` module
   that still points into the real home (belt and braces: `usage.CONFIG_FILE`, ...);
3. replaces the keyring with an in-memory one (`creds._keyring`) and `autostart.winreg` with an
   in-memory registry (`ctx.winreg.values`), so autostart goldens run on every OS and never touch
   the real HKCU `Run` key;
4. installs an audit hook that raises `Denied` for: any file open/list/mkdir/remove/rename under the
   real home (other than the interpreter, site-packages, this repo and the temp dir);
   `subprocess.Popen`, `os.system`, `os.exec*`/`spawn*`; any `winreg.*`; any `socket.connect` /
   DNS lookup to a non-loopback host. An exporter that really needs one of them (a test with a
   scripted fake program, a loopback server) opts in explicitly: `with h.allow("subprocess"):`.
   Kinds: `subprocess`, `winreg`, `network`.

Fixtures: `{input, output}`, nested key order PRESERVED (order is part of the contract for wire
bodies, usage payloads and hook JSON; use `canonical_text` for the exact file text), stable float
repr, `ensure_ascii`, each file < 512 KB, no binary (use `sha256_of` and store the digest), no
absolute home paths (use `rel`). `write_fixture` refuses to run before `isolate()`.

    from tools.golden import _harness as h
    ctx = h.isolate()
    from clawd_desktop import paths
    ...
    h.write_fixture("store", "prefs_roundtrip", input=..., output=..., ctx=ctx)
"""
from __future__ import annotations

import contextlib
import hashlib
import ipaddress
import json
import os
import re
import site
import sys
import sysconfig
import tempfile
from pathlib import Path
from typing import Any, Iterator

REPO = Path(__file__).resolve().parents[2]
FIXTURES = REPO / "desktop-rs" / "fixtures"
MAX_FIXTURE_BYTES = 512_000          # check_public.py MAX_BYTES; stay under it
HOME_ENV = ("HOME", "USERPROFILE", "CODEX_HOME", "COPILOT_HOME", "XDG_CONFIG_HOME",
            "PI_CODING_AGENT_DIR", "APPDATA", "LOCALAPPDATA")
FILE_EVENTS = {"open", "os.listdir", "os.scandir", "os.mkdir", "os.remove", "os.rename",
               "os.rmdir", "shutil.copyfile", "shutil.rmtree"}
PROCESS_EVENTS = {"subprocess.Popen", "os.system", "os.exec", "os.spawn", "os.posix_spawn"}
LOOPBACK_NAMES = {"localhost", "127.0.0.1", "::1", ""}


class Denied(AssertionError):
    """An exporter did something the harness forbids by default."""


class RealHomeAccess(Denied):
    """An exporter touched the real home directory."""


def _norm(p: str | os.PathLike[str]) -> str:
    return os.path.normcase(os.path.realpath(os.fspath(p)))


# Computed once, at import, before any isolation: the user's real home.
REAL_HOME = _norm(Path.home())
_ALLOWED: list[str] = []          # extra roots that may be touched (interpreter, repo, temp)
_OPTED_IN: dict[str, int] = {}    # kind -> nesting depth of `allow(kind)`
_STATE: dict[str, Any] = {"root": None, "installed": False, "ctx": None}


def _under(path: str, root: str) -> bool:
    return path == root or path.startswith(root.rstrip(os.sep) + os.sep)


@contextlib.contextmanager
def allow(*kinds: str) -> Iterator[None]:
    """Opt in to `subprocess`, `winreg` or `network` for a block. Everything else stays denied."""
    for kind in kinds:
        if kind not in ("subprocess", "winreg", "network"):
            raise ValueError(f"unknown kind {kind!r}")
    for kind in kinds:
        _OPTED_IN[kind] = _OPTED_IN.get(kind, 0) + 1
    try:
        yield
    finally:
        for kind in kinds:
            _OPTED_IN[kind] -= 1


def _is_loopback(host: Any) -> bool:
    if isinstance(host, bytes):
        host = os.fsdecode(host)
    if not isinstance(host, str):
        return True
    if host.lower() in LOOPBACK_NAMES:
        return True
    try:
        return ipaddress.ip_address(host.split("%")[0]).is_loopback
    except ValueError:
        return False


def _audit(event: str, args: tuple) -> None:
    if event in FILE_EVENTS:
        _audit_file(event, args)
    elif event in PROCESS_EVENTS or event.startswith("os.exec") or event.startswith("os.spawn"):
        if not _OPTED_IN.get("subprocess"):
            raise Denied(f"{event} is denied in exporters (use `with h.allow('subprocess'):`): {args[:1]}")
    elif event.startswith("winreg."):
        if not event.endswith("/result") and not _OPTED_IN.get("winreg"):
            raise Denied(f"{event} would touch the real registry (autostart.winreg is faked)")
    elif event == "socket.connect":
        address = args[1] if len(args) > 1 else None
        if isinstance(address, tuple) and address and not _is_loopback(address[0]) \
                and not _OPTED_IN.get("network"):
            raise Denied(f"socket.connect to {address[0]!r} is denied (use `with h.allow('network'):`)")
    elif event == "socket.getaddrinfo":
        host = args[0] if args else None
        if not _is_loopback(host) and not _OPTED_IN.get("network"):
            raise Denied(f"DNS lookup of {host!r} is denied (use `with h.allow('network'):`)")


def _audit_file(event: str, args: tuple) -> None:
    if not args:
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
    normed = {_norm(r) for r in roots if r}
    # a root that IS the real home, or contains it, would allow everything under it
    return sorted(r for r in normed if r != REAL_HOME and not _under(REAL_HOME, r))


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


class _MemWinreg:
    """The slice of `winreg` that `autostart.py` uses, in memory: `values[(key, name)] = (data, type)`."""

    HKEY_CURRENT_USER = "HKCU"
    REG_SZ = 1
    KEY_SET_VALUE = 2

    def __init__(self) -> None:
        self.values: dict[tuple[str, str], tuple[str, int]] = {}

    @contextlib.contextmanager
    def _key(self, path: str):
        yield path

    def OpenKey(self, root, path, reserved=0, access=0):         # noqa: N802 (winreg API)
        return self._key(path)

    def CreateKey(self, root, path):                              # noqa: N802
        return self._key(path)

    def QueryValueEx(self, key, name):                            # noqa: N802
        try:
            return self.values[(key, name)]
        except KeyError:
            raise OSError(2, "The system cannot find the file specified") from None

    def SetValueEx(self, key, name, reserved, kind, data):        # noqa: N802
        self.values[(key, name)] = (data, kind)

    def DeleteValue(self, key, name):                             # noqa: N802
        if (key, name) not in self.values:
            raise OSError(2, "The system cannot find the file specified")
        del self.values[(key, name)]


class Isolation:
    """What `isolate()` set up."""

    def __init__(self, root: Path, keyring: _MemKeyring, winreg: _MemWinreg) -> None:
        self.root = root
        self.home = root / "home"
        self.keyring = keyring
        self.winreg = winreg

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
    for var in [v for v in os.environ if v.upper().startswith("CLAWD_")]:
        del os.environ[var]                                   # feed token/url, update repo, event port...
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
    sys.addaudithook(_audit)
    _STATE["installed"] = True
    ctx = Isolation(root, _MemKeyring(), _MemWinreg())
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
    """Rebase any Path constant still pointing into the real home; in-memory keyring/registry."""
    import importlib
    from clawd_desktop import autostart, creds
    for name in list(sys.modules):
        if not name.startswith("clawd_desktop"):
            continue
        mod = sys.modules[name]
        for attr, val in list(vars(mod).items()):
            if isinstance(val, Path) and _under(_norm(val), REAL_HOME) and not _under(_norm(val), _norm(home)):
                tail = os.path.relpath(_norm(val), REAL_HOME)
                setattr(mod, attr, home / tail)
    creds._keyring = lambda: ctx.keyring                     # type: ignore[assignment]
    autostart.winreg = ctx.winreg                            # type: ignore[assignment]
    importlib.invalidate_caches()


def sha256_of(data: bytes) -> str:
    """Binary data is stored as its digest, never as bytes/base64 (check_public bans art)."""
    return hashlib.sha256(data).hexdigest()


_FORBIDDEN = (re.compile(r"iVBORw0KGgo"), re.compile(r"CLWD"), re.compile(r"/Users/"),
              re.compile(r"[A-Za-z]:\\{1,2}Users\\"))


def canonical_text(obj: Any) -> str:
    """The exact text of a fixture / of a file the Rust side must reproduce: 2-space indent,
    `ensure_ascii`, trailing newline, **insertion order of every object kept** (do not sort)."""
    return json.dumps(obj, indent=2, ensure_ascii=True) + "\n"


def write_fixture(area: str, name: str, *, input: Any, output: Any, ctx: Isolation | None = None) -> Path:
    """Write `desktop-rs/fixtures/<area>/<name>.json` as `{input, output}`. Needs `isolate()`."""
    if not _STATE["installed"]:
        raise RuntimeError("write_fixture before isolate(): the exporter may have read the real home")
    ctx = ctx or _STATE["ctx"]
    data = ctx.rel({"input": input, "output": output})
    text = canonical_text(data)
    if str(ctx.root) in text or ctx.root.as_posix() in text:
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
    """Every guard must fire (and the temp dir must stay usable). Exits non-zero on failure."""
    import socket
    import subprocess

    def expect(exc: type[BaseException], what: str, fn) -> None:
        try:
            fn()
        except exc:
            print(f"selftest: {what}: blocked")
            return
        except OSError:
            pass
        raise SystemExit(f"selftest FAILED: {what} was not blocked")

    # before isolate(): refuses to write fixtures
    try:
        write_fixture("store", "selftest", input={}, output={})
    except RuntimeError:
        print("selftest: write_fixture before isolate(): refused")
    else:
        raise SystemExit("selftest FAILED: write_fixture ran before isolate()")

    os.environ["CLAWD_FEED_TOKEN"] = "leak"
    os.environ["CLAWD_EVENT_PORT"] = "1"
    ctx = isolate()
    (ctx.home / "ok.txt").write_text("fine")
    if any(k.upper().startswith("CLAWD_") for k in os.environ):
        raise SystemExit("selftest FAILED: CLAWD_* variables survived isolate()")
    print("selftest: CLAWD_* environment cleared")

    # the allowed roots never include the real home or one of its ancestors
    if any(r == REAL_HOME or _under(REAL_HOME, r) for r in _ALLOWED):
        raise SystemExit("selftest FAILED: an allowed root covers the real home")
    print("selftest: no allowed root covers the real home")

    probe = Path(REAL_HOME) / ".config" / "claude-usage-monitor" / "desktop.json"
    expect(RealHomeAccess, "real-home access", lambda: open(probe))
    cmd = ["cmd", "/c", "echo", "x"] if os.name == "nt" else ["echo", "x"]
    expect(Denied, "subprocess.Popen", lambda: subprocess.Popen(cmd))
    expect(Denied, "os.system", lambda: os.system("echo x"))
    expect(Denied, "non-loopback socket.connect",
           lambda: socket.create_connection(("192.0.2.1", 9), timeout=0.1))

    from clawd_desktop import autostart
    if not autostart.winreg.HKEY_CURRENT_USER == "HKCU":
        raise SystemExit("selftest FAILED: autostart.winreg is not the in-memory fake")
    autostart.enable("win32")
    data = ctx.winreg.values.get((autostart.RUN_KEY, autostart.RUN_VALUE))
    if data is None or not autostart.is_enabled("win32"):
        raise SystemExit("selftest FAILED: autostart did not use the in-memory registry")
    print("selftest: autostart.winreg is in memory")
    with allow("subprocess"):
        subprocess.run(cmd, capture_output=True, check=False)
    print("selftest: allow('subprocess') opt-in works")
    print("selftest: ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        selftest()
    else:
        print(__doc__)
