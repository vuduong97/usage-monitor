"""Entry point.

  Clawd                     tray app (default)
  Clawd run                 headless: same work, logs to the terminal (Ctrl+C stops)
  Clawd find                list the clocks on this network (mDNS, then a LAN scan)
  Clawd hooks on|off        install / remove the Claude Code hooks
  Clawd autostart on|off    start at login, or not
  Clawd cleanup             what the uninstaller runs: autostart off, hooks off
"""
from __future__ import annotations

import argparse
import asyncio
import signal
import sys
import time

from clawd_desktop import __version__, autostart, hooks


def _run_headless() -> int:
    from clawd_desktop.engine import Engine, daemon_running
    if daemon_running():
        print("Another Clawd daemon is already running (event port in use).")
        return 1
    engine = Engine()

    async def go() -> None:
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            try:
                loop.add_signal_handler(sig, stop.set)
            except (NotImplementedError, RuntimeError):
                signal.signal(sig, lambda *_: loop.call_soon_threadsafe(stop.set))
        await engine.run(stop)
    try:
        asyncio.run(go())
    except KeyboardInterrupt:
        pass
    return 0


def _find(seconds: float) -> int:
    from clawd_desktop import discovery

    async def go() -> None:
        b = discovery.MdnsBrowser()
        b.start()
        await asyncio.sleep(seconds)
        clocks = [c for c in (await asyncio.gather(*(asyncio.to_thread(b.resolve, n) for n in b.names())))
                  if c is not None]
        b.stop()
        for c in clocks:
            print(f"mDNS  {c.name:<20} {c.base:<28} v{c.version}")
        if not clocks:
            print("mDNS: nothing; scanning the local network…")
            for c in await discovery.scan_lan():
                print(f"scan  {'':<20} {c.base:<28} v{c.version}")
    asyncio.run(go())
    return 0


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    # macOS passes -psn_… when an app is opened from Finder on old systems.
    argv = [a for a in argv if not a.startswith("-psn_")]
    ap = argparse.ArgumentParser(prog="Clawd", description="Clawd desk clock companion")
    ap.add_argument("--version", action="version", version=f"Clawd {__version__}")
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("tray")
    sub.add_parser("run")
    f = sub.add_parser("find")
    f.add_argument("--seconds", type=float, default=10.0)
    for name in ("hooks", "autostart"):
        p = sub.add_parser(name)
        p.add_argument("state", choices=["on", "off"])
    sub.add_parser("cleanup")
    a = ap.parse_args(argv)

    if a.cmd in (None, "tray"):
        from clawd_desktop import tray
        return tray.main()
    if a.cmd == "run":
        return _run_headless()
    if a.cmd == "find":
        return _find(a.seconds)
    if a.cmd == "hooks":
        hooks.apply(a.state == "on")
        return 0
    if a.cmd == "autostart":
        ok = autostart.set_enabled(a.state == "on")
        print(f"autostart {'on' if a.state == 'on' else 'off'}" if ok else "autostart: not supported here")
        return 0 if ok else 1
    if a.cmd == "cleanup":
        autostart.disable()
        hooks.apply(False)
        print(f"[{time.strftime('%H:%M:%S')}] cleanup done")
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
