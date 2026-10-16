#!/usr/bin/env python3
"""Golden exporter for `clawd-store` (phase 03): runs the REAL `clawd_desktop.{paths,pause,
firstrun,autostart,creds}` inside the isolation harness and records what the Rust crate must
reproduce, byte for byte where the result is a file:

  prefs_update      desktop.json after update_prefs / save_prefs (wrong types, floats, unicode,
                    key order, missing / corrupt file) + clock_screen reads
  firstrun          migrate / load / pending / mark_done / should_open / mark_shown / seen /
                    mark_seen / new_agents
  pause             pause.json text written by hold(), PauseReader.reason() for many file states
  ota_busy          ota_busy() around the 900 s stale limit
  usage_payload     usage_payload.json text and what usage_payload() makes of odd files
  autostart         launch_agent dict, the plist bytes (text + sha256), the Run value strings
  creds             the raw keyring secret put() stores and what get() makes of stored secrets

`prefs_roundtrip.json` is written by `export_sample.py` (kept as is). Nothing here reads the real
home, keyring or registry: the harness redirects HOME, fakes keyring and winreg (red team #14).
Run:  python tools/golden/export_store.py
"""
from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _harness as h  # noqa: E402

ctx = h.isolate()
from clawd_desktop import __version__, autostart, creds, firstrun, pause, paths  # noqa: E402

VERSION_TOKEN = "@VERSION@"        # the Rust side substitutes its own APP_VERSION
PID_TOKEN = '"pid": 0'


def reset_config() -> None:
    """An empty config dir (the previous case's files removed)."""
    cfg = paths.CONFIG_DIR
    if cfg.exists():
        for p in cfg.iterdir():
            p.unlink()
    cfg.mkdir(parents=True, exist_ok=True)


def put_prefs(text: str | None) -> None:
    reset_config()
    if text is not None:
        paths.PREFS_FILE.write_text(text, encoding="utf-8", newline="")


def prefs_text() -> str | None:
    return paths.PREFS_FILE.read_text(encoding="utf-8") if paths.PREFS_FILE.exists() else None


def swap_version(value):
    """Replace the app version by a token (in text or nested values) so fixtures do not pin it."""
    if isinstance(value, str):
        return value.replace(__version__, VERSION_TOKEN)
    if isinstance(value, dict):
        return {k: swap_version(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [swap_version(v) for v in value]
    return value


def unswap_version(value):
    if isinstance(value, str):
        return value.replace(VERSION_TOKEN, __version__)
    if isinstance(value, dict):
        return {k: unswap_version(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [unswap_version(v) for v in value]
    return value


# =====================================================================================
# prefs
# =====================================================================================
def apply_ops(prefs: dict, ops: list) -> None:
    for op in ops:
        if op[0] == "set":
            prefs[op[1]] = op[2]
        elif op[0] == "del":
            prefs.pop(op[1], None)
        else:
            raise ValueError(op)


FLOATS = ("{\"tiny\": 1e-05, \"big\": 1e16, \"huge\": 1.5e22, \"sum\": 0.30000000000000004, "
          "\"hundred\": 1e2, \"neg_zero\": -0.0, \"denorm\": 5e-324, \"max\": 1.7976931348623157e308, "
          "\"i53\": 9007199254740993, \"u64\": 18446744073709551615, \"neg\": -42, "
          "\"exp_int\": 12E3, \"short\": 0.1, \"mid\": 123456.789e3}")
UNICODE = json.dumps({
    "name": "Đồng hồ \u2603 \U0001F600", "ctl": "tab\t nl\n cr\r q\" bs\\ del\x7f nul\x00 ls\u2028",
    "Khóa": ["é", {"ü": None}], "empty": [], "empty_obj": {}, "nested": {"a": {"b": [[], {}, [1, [2]]]}},
})   # ASCII-only text: Python reads desktop.json with the locale codec (cp1252 on Windows)
WRONG_TYPES = json.dumps({
    "setup_done": "clock", "usage": {"screen": 300}, "clock_screen": 1.0, "usage_source": 7,
    "agents_seen": "codex", "first_run_done": "yes", "setup_shown": 5, "future_key": {"n": [1, 2.5, None, True]},
}, indent=2)

PREFS_CASES = [
    ("wrong types survive a one-key change", WRONG_TYPES, [["set", "clock_screen", 1]]),
    ("no-op update leaves the file text alone", '{"b":1,"a":[1,2]}', []),
    ("update that sets the same value does not rewrite", '{"a":1}', [["set", "a", 1]]),
    ("missing file + no-op writes {}", None, []),
    ("missing file + set", None, [["set", "x", {"k": [1, "é"]}]]),
    ("corrupt JSON reads as {}", '{"a": 1,', [["set", "x", 1]]),
    ("non-object JSON reads as {}", "[1, 2, 3]", [["set", "x", 1]]),
    ("empty file reads as {}", "", [["set", "x", 1]]),
    ("floats use python repr", FLOATS, [["set", "marker", True]]),
    ("unicode and control characters are escaped", UNICODE, [["set", "marker", True]]),
    ("existing key keeps its position, new key goes last", '{"z": 1, "a": 2, "m": 3}',
     [["set", "a", 20], ["set", "n", 4]]),
    ("deleted then re-added key moves last", '{"z": 1, "a": 2, "m": 3}', [["del", "z"], ["set", "z", 9]]),
    ("set of unicode value and float", "{}", [["set", "name", "Đ \U0001F600"], ["set", "ratio", 0.1 + 0.2]]),
    ("clock_screen bool and zero", '{"clock_screen": true}', [["set", "clock_screen", 0]]),
    ("negative and big clock_screen", '{}', [["set", "clock_screen", -7]]),
]

CLOCK_SCREEN_READS = [
    '{"clock_screen": 0}', '{"clock_screen": 1}', '{"clock_screen": 2}', '{"clock_screen": 300}',
    '{"clock_screen": -1}', '{"clock_screen": true}', '{"clock_screen": false}',
    '{"clock_screen": 1.0}', '{"clock_screen": "1"}', '{"clock_screen": null}', '{"clock_screen": [1]}',
    '{}', '[]', '{bad', '',
]

SAVE_CASES = [
    ("save replaces everything", '{"old": 1, "keep": 2}', {"new": [1, "x"], "f": 2.50, "u": "é"}),
    ("save onto a missing file", None, {"a": 1}),
    ("save an empty object", '{"old": 1}', {}),
]


def export_prefs() -> None:
    cases = []
    for name, initial, ops in PREFS_CASES:
        put_prefs(initial)

        def change(prefs, ops=ops):
            apply_ops(prefs, ops)
        result = paths.update_prefs(change)
        cases.append({"name": name, "input": {"file": initial, "ops": ops},
                      "output": {"file_text": prefs_text(), "prefs": result}})
    reads = []
    for text in CLOCK_SCREEN_READS:
        put_prefs(text)
        reads.append({"file": text, "clock_screen": paths.clock_screen(),
                      "load_prefs": paths.load_prefs()})
    put_prefs('{"a": 1}')
    paths.set_clock_screen(2)
    set_screen = {"file_text": prefs_text()}
    saves = []
    for name, initial, data in SAVE_CASES:
        put_prefs(initial)
        paths.save_prefs(data)
        saves.append({"name": name, "input": {"file": initial, "data": data},
                      "output": {"file_text": prefs_text()}})
    h.write_fixture("store", "prefs_update", ctx=ctx,
                    input={"update_cases": [c["name"] for c in cases]},
                    output={"update": cases, "reads": reads, "set_clock_screen_2_on_a1": set_screen,
                            "save": saves})


# =====================================================================================
# firstrun
# =====================================================================================
FIRSTRUN_PREFS = {
    "fresh": None,
    "empty": {},
    "upgrade_from_0_1_x": {"first_run_done": True, "usage_source": "claude"},
    "upgrade_falsy": {"first_run_done": 0},
    "done_list": {"setup_done": ["clock", "agents"], "x": 1},
    "done_all": {"setup_done": ["clock", "agents", "usage"]},
    "done_junk": {"setup_done": ["clock", 7, None], "setup_shown": "0.0.1"},
    "done_wrong_type": {"setup_done": "clock", "first_run_done": 1},
    "shown_current": {"setup_done": ["clock"], "setup_shown": VERSION_TOKEN},
    "shown_old": {"setup_done": ["clock"], "setup_shown": "0.0.1"},
    "shown_wrong_type": {"setup_done": [], "setup_shown": 5},
    "seen_list": {"setup_done": [], "agents_seen": ["pi", "codex"]},
    "seen_wrong": {"setup_done": [], "agents_seen": "codex"},
    "key_order": {"b": 1, "a": 2},
}
FIRSTRUN_CALLS = [
    ("migrate", []),
    ("load", []),
    ("pending", []),
    ("should_open", []),
    ("mark_done", [["clock"]]),
    ("mark_done", [["usage", "bogus", "usage", "clock"]]),
    ("mark_done", [["agents", "usage", "clock"]]),
    ("mark_done", [[]]),
    ("mark_shown", []),
    ("seen", []),
    ("mark_seen", [["pi", "codex", "pi"]]),
    ("mark_seen", [["amp"]]),
    ("mark_seen", [[]]),
    ("new_agents", [["codex", "pi", "amp", "cursor", "pi"], ["amp"]]),
]


def export_firstrun() -> None:
    cases = []
    for pname, prefs in FIRSTRUN_PREFS.items():
        for fn, args in FIRSTRUN_CALLS:
            initial = None if prefs is None else json.dumps(unswap_version(prefs), indent=2)
            put_prefs(initial)
            real = getattr(firstrun, fn)
            if fn == "migrate":
                result = real(paths.load_prefs())
            elif fn in ("pending", "should_open"):
                # both forms: from the file, and from an explicit dict
                result = real()
                explicit = real(paths.load_prefs())
                assert explicit == result
            else:
                result = real(*args)
            if isinstance(result, set):
                result = sorted(result)
            cases.append({"prefs": pname, "fn": fn, "args": args, "file": swap_version(initial),
                          "result": swap_version(result), "file_text": swap_version(prefs_text())})
    h.write_fixture("store", "firstrun", ctx=ctx,
                    input={"steps": list(firstrun.STEPS), "version_token": VERSION_TOKEN},
                    output={"cases": cases})


# =====================================================================================
# pause + ota.busy
# =====================================================================================
PAUSE_READS = [
    ("active", '{"until": 1100.5, "reason": "firmware", "pid": 1}', 1000.0),
    ("last tick", '{"until": 1100, "reason": "fw"}', 1099.999),
    ("expired exactly", '{"until": 1100, "reason": "fw"}', 1100.0),
    ("expired", '{"until": 900, "reason": "fw"}', 1000.0),
    ("lease boundary 300", '{"until": 1300, "reason": "edge"}', 1000.0),
    ("beyond lease 301", '{"until": 1301, "reason": "far"}', 1000.0),
    ("no reason", '{"until": 1100}', 1000.0),
    ("empty reason", '{"until": 1100, "reason": ""}', 1000.0),
    ("null reason", '{"until": 1100, "reason": null}', 1000.0),
    ("numeric reason", '{"until": 1100, "reason": 7}', 1000.0),
    ("string until", '{"until": "1100", "reason": "s"}', 1000.0),
    ("bool until", '{"until": true, "reason": "b"}', 0.5),
    ("null until", '{"until": null, "reason": "n"}', 1000.0),
    ("list until", '{"until": [1], "reason": "l"}', 1000.0),
    ("missing until", '{"reason": "m"}', 1000.0),
    ("array file", '[1100]', 1000.0),
    ("garbage", '{nope', 1000.0),
    ("empty file", '', 1000.0),
    ("unicode reason", '{"until": 1100, "reason": "C\\u1eadp nh\\u1eadt \\u2603"}', 1000.0),
]
HOLDS = [("firmware", 60.0, 1000.5), ("fw upload", 3600.0, 1000.0), ("short", 0.25, 5.0),
         ("zero", 0.0, 1000.0), ("negative", -5.0, 1000.0), ("cap edge", 300.0, 1700000000.123456),
         ("unicode Đ \u2603", 10.0, 1.0)]
OTA_AGES = [0.0, 1.0, 899.0, 899.9, 900.0, 900.1, 901.0, 5000.0, -50.0]
OTA_MTIME = 1_700_000_000


def export_pause() -> None:
    holds = []
    for reason, seconds, now in HOLDS:
        reset_config()
        pause.hold(seconds, reason, path=pause.PAUSE_FILE, now=lambda now=now: now)
        text = pause.PAUSE_FILE.read_text(encoding="utf-8")
        text = text.replace(f'"pid": {os.getpid()}', PID_TOKEN)
        holds.append({"input": {"seconds": seconds, "reason": reason, "now": now},
                      "output": {"file_text": text}})
    released = {}
    reset_config()
    pause.release()                                       # missing file is fine
    pause.hold(10, "x", now=lambda: 1.0)
    pause.release()
    released["exists_after_release"] = pause.PAUSE_FILE.exists()
    reads = []
    for name, text, now in PAUSE_READS:
        reset_config()
        pause.PAUSE_FILE.write_text(text, encoding="utf-8", newline="")
        reader = pause.PauseReader(path=pause.PAUSE_FILE, now=lambda now=now: now)
        reads.append({"name": name, "input": {"file": text, "now": now}, "output": {"reason": reader.reason()}})
    reset_config()
    missing = pause.PauseReader(path=pause.PAUSE_FILE, now=lambda: 1.0).reason()
    h.write_fixture("store", "pause", ctx=ctx,
                    input={"max_lease": pause.MAX_LEASE, "pid_token": PID_TOKEN},
                    output={"hold": holds, "release": released, "reads": reads, "missing_file": missing})


def export_ota_busy() -> None:
    cases = []
    for age in OTA_AGES:
        reset_config()
        paths.OTA_BUSY_FILE.write_text("1")
        os.utime(paths.OTA_BUSY_FILE, (OTA_MTIME, OTA_MTIME))
        now = OTA_MTIME + age
        cases.append({"age": age, "now": now, "busy": paths.ota_busy(now=now)})
    reset_config()
    missing = paths.ota_busy(now=OTA_MTIME)
    h.write_fixture("store", "ota_busy", ctx=ctx,
                    input={"mtime": OTA_MTIME, "stale": paths.OTA_BUSY_STALE},
                    output={"cases": cases, "missing_file": missing})


# =====================================================================================
# usage payload
# =====================================================================================
PAYLOADS = [
    {"s": 5, "m": "alert", "b": 1, "w": 9, "ok": True, "x": 1},
    {"w": 1, "s": 2, "sr": 3, "wr": 4, "st": "allowed", "ok": True, "acct": "pro", "tp": 50, "rd": "Nov 1",
     "t": 1760000000, "tf": 24, "c": 1, "src": "codex", "why": "login", "unknown": [1]},
    {},
    {"s": 12.5, "ok": False, "acct": "Đồng \u2603"},
]
PAYLOAD_NOWS = [1760000000.5, 1760000000.0, 5.0, 1e-05]
PAYLOAD_READS = [
    '{"at": 1760000000.5, "payload": {"s": 5}}', '{"at": 5, "payload": {}}', '{"at": 5.0, "payload": {"z": 1, "a": 2}}',
    '{"at": true, "payload": {}}', '{"at": "5", "payload": {}}', '{"at": null, "payload": {}}',
    '{"payload": {}}', '{"at": 5}', '{"at": 5, "payload": []}', '{"at": 5, "payload": "x"}',
    '[]', '"x"', '{bad', '', '{"at": 1e2, "payload": {"s": 1}}',
]


def export_usage_payload() -> None:
    writes = []
    for payload, now in zip(PAYLOADS, PAYLOAD_NOWS * 2):
        reset_config()
        paths.set_usage_payload(payload, now=now)
        writes.append({"input": {"payload": payload, "now": now},
                       "output": {"file_text": paths.PAYLOAD_FILE.read_text(encoding="utf-8"),
                                  "read": paths.usage_payload()}})
    reads = []
    for text in PAYLOAD_READS:
        reset_config()
        paths.PAYLOAD_FILE.write_text(text, encoding="utf-8", newline="")
        got = paths.usage_payload()
        reads.append({"file": text, "result": None if got is None else {"payload": got[0], "at": got[1]}})
    reset_config()
    missing = paths.usage_payload()
    h.write_fixture("store", "usage_payload", ctx=ctx,
                    input={"payload_keys": list(paths.PAYLOAD_KEYS)},
                    output={"writes": writes, "reads": reads, "missing_file": missing})


# =====================================================================================
# autostart
# =====================================================================================
MAC_EXE = "/Applications/Clawd.app/Contents/MacOS/Clawd"
WIN_EXES = [r"D:\Apps\Clawd\Clawd.exe", r"D:\Apps\Clawd \u00e9 & Co\Clawd.exe".replace("\\u00e9", "\u00e9"),
            r"C:\Program Files\Clawd\Clawd.exe"]
MAC_CMDS = [[MAC_EXE], ["/opt/Clawd & <Co>/Clawd é"], [MAC_EXE, "--flag"]]


def export_autostart() -> None:
    paths_frozen = paths.frozen
    paths.frozen = lambda: True            # the packaged app: no WorkingDirectory key
    try:
        mac = []
        for cmd in MAC_CMDS:
            autostart.command = lambda cmd=cmd: list(cmd)
            reset_config()
            if autostart.PLIST.exists():
                autostart.PLIST.unlink()
            before = autostart.is_enabled("darwin")
            enabled = autostart.enable("darwin")
            raw = autostart.PLIST.read_bytes()
            after = autostart.is_enabled("darwin")
            disabled = autostart.disable("darwin")
            gone = not autostart.is_enabled("darwin")
            again = autostart.disable("darwin")
            mac.append({"input": {"cmd": cmd},
                        "output": {"dict": autostart.launch_agent(cmd), "plist_text": raw.decode("utf-8"),
                                   "plist_sha256": h.sha256_of(raw), "enabled_returns": enabled,
                                   "is_enabled_before": before, "is_enabled_after": after,
                                   "disable_returns": disabled, "gone_after_disable": gone,
                                   "disable_again_returns": again}})
        win = []
        for exe in WIN_EXES:
            cmd = [exe]
            autostart.command = lambda cmd=cmd: list(cmd)
            ctx.winreg.values.clear()
            before = autostart.is_enabled("win32")
            enabled = autostart.enable("win32")
            data = ctx.winreg.values.get((autostart.RUN_KEY, autostart.RUN_VALUE))
            after = autostart.is_enabled("win32")
            disabled = autostart.disable("win32")
            gone = not autostart.is_enabled("win32")
            again = autostart.disable("win32")
            win.append({"input": {"cmd": cmd},
                        "output": {"run_value": data[0] if data else None, "run_type": "REG_SZ" if data and data[1] == 1 else None,
                                   "enabled_returns": enabled, "is_enabled_before": before,
                                   "is_enabled_after": after, "disable_returns": disabled,
                                   "gone_after_disable": gone, "disable_again_returns": again}})
        unsupported = {
            "supported": {p: autostart.supported(p) for p in ("darwin", "win32", "linux")},
            "linux_enable": autostart.enable("linux"), "linux_disable": autostart.disable("linux"),
            "linux_is_enabled": autostart.is_enabled("linux"),
            "linux_set_enabled_true": autostart.set_enabled(True, "linux"),
        }
    finally:
        paths.frozen = paths_frozen
    h.write_fixture("store", "autostart", ctx=ctx,
                    input={"label": autostart.LABEL, "run_key": autostart.RUN_KEY, "run_value": autostart.RUN_VALUE},
                    output={"mac": mac, "windows": win, "unsupported": unsupported})


# =====================================================================================
# creds
# =====================================================================================
CRED_PUTS = [("clock-1", "root", "pa\nss"), ("clock-2", "admin", ""), ("Đồng hồ", "người dùng", "mật khẩu \U0001F600"),
             ("k", "", "pw"), ("k2", "u", "a\r\nb")]
CRED_STORED = ["", "plain", "u\np", "u\n", "\nx", "a\nb\nc", "admin\n", "\n", "é\nü"]


def export_creds() -> None:
    puts = []
    for key, user, pw in CRED_PUTS:
        ctx.keyring.store.clear()
        ok = creds.put(key, user, pw)
        stored = ctx.keyring.store.get((creds.SERVICE, key))
        got = creds.get(key)
        puts.append({"input": {"key": key, "user": user, "password": pw},
                     "output": {"ok": ok, "secret": stored, "get": list(got) if got else None}})
    gets = []
    for raw in CRED_STORED:
        ctx.keyring.store.clear()
        ctx.keyring.store[(creds.SERVICE, "k")] = raw
        got = creds.get("k")
        gets.append({"secret": raw, "get": list(got) if got else None})
    ctx.keyring.store.clear()
    missing = creds.get("nope")
    ctx.keyring.store[(creds.SERVICE, "k")] = "u\np"
    creds.forget("k")
    creds.forget("k")                                     # twice: failures ignored
    forgotten = ctx.keyring.store.get((creds.SERVICE, "k"))
    h.write_fixture("store", "creds", ctx=ctx,
                    input={"service": creds.SERVICE, "default_user": creds.DEFAULT_USER},
                    output={"puts": puts, "gets": gets, "missing": missing, "after_forget": forgotten})


if __name__ == "__main__":
    export_prefs()
    export_firstrun()
    export_pause()
    export_ota_busy()
    export_usage_payload()
    export_autostart()
    export_creds()
    for p in sorted((h.FIXTURES / "store").glob("*.json")):
        print(f"{p.relative_to(h.REPO)}  {p.stat().st_size} bytes")
