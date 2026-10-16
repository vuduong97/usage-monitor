#!/usr/bin/env python3
"""Settings exporter (phase 13): the pure shaping logic of `settings_api.py`, recorded as
`desktop-rs/fixtures/settings/shaping.json` for `app/tests/settings_golden.rs`.

Runs the real `clean`, `_clock_view`, `config_diff`, `_err`, `SettingsApi.scan` and
`SettingsApi.job` on small fixed inputs: a fake clock client, no network, no renderer, no pak,
no art, no real HOME (the harness isolates it). Nothing here reads `clawd.pak` or a feed.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _harness as h  # noqa: E402

ctx = h.isolate()
from clawd_desktop import settings_api as sa  # noqa: E402  (after isolate())
from clawd_desktop.clockapi import AuthNeeded, ClockError, config_diff  # noqa: E402
from clawd_desktop.discovery import ClockStore  # noqa: E402


def attempt(fn, *a):
    try:
        return {"ok": fn(*a)}
    except ValueError as e:
        return {"error": str(e)}


CLEAN_INPUTS = [
    {"hostname": "clawd", "brightness": 50, "evil": 1, "usage": {"screen": 1, "bogus": 2}},
    {"brightness": 101},
    {"brightness": True},
    {"brightness": 5.0},
    {"hostname": "Bad Name"},
    {"hostname": "a" * 32},
    {"hostname": "a" * 33},
    {"rotation": 3, "usage": {"anim": -1, "layout": 15}},
    {"usage": {"anim": 64}},
    {"clock": {"tz": "Asia/Ho_Chi_Minh", "nightEnabled": True, "nightStart": "22:30", "nightEnd": "06:00",
               "nightLevel": 10}},
    {"clock": {"nightStart": "24:00"}},
    {"clock": {"nightEnabled": 1}},
    {"wifi": [{"ssid": "Home", "pass": "", "passSet": True}, {"ssid": "Phone", "pass": "password1"}]},
    {"wifi": []},
    {"wifi": [{"ssid": "A"}, {"ssid": "A"}]},
    {"wifi": [{"ssid": "A", "pass": "short"}]},
    {"wifi": ["x"]},
    "not a dict",
]

CLOCK_CFGS = [
    {"hostname": "clawd", "brightness": 999, "uptime": 5,
     "wifi": [{"ssid": "Home", "passSet": True}, {"ssid": ""}, "junk"]},
    {"hostname": "clawd", "brightness": 40, "rotation": 1, "wifi": [{"ssid": "Home", "passSet": True}],
     "usage": {"screen": 1, "anim": 3, "layout": 2},
     "clock": {"tz": "UTC", "nightEnabled": False, "nightStart": "22:00", "nightEnd": "06:00", "nightLevel": 5}},
]

DIFFS = [
    {"cfg": CLOCK_CFGS[1], "edited": {"brightness": 60, "usage": {"screen": 2}}},
    {"cfg": CLOCK_CFGS[1], "edited": {"brightness": 40}},
    {"cfg": CLOCK_CFGS[1], "edited": {"wifi": [{"ssid": "Home"}, {"ssid": "Cafe", "pass": "password1"}]}},
    {"cfg": CLOCK_CFGS[1], "edited": {"clock": {"nightEnabled": True}}},
]

ERRORS = [
    ("AuthNeeded", AuthNeeded("login")),
    ("NoAnswer", ClockError("no answer from http://10.0.0.5 (ConnectError)")),
    ("Http", ClockError("HTTP 500: boom")),
]


class FakeApi:
    def __init__(self, base, auth=None):
        self.base = base

    def scan(self):
        return [{"ssid": "Home", "rssi": -70, "enc": 3}, {"ssid": "Cafe", "rssi": -50},
                {"ssid": "Home", "rssi": -60, "enc": 3}, {"ssid": "", "rssi": -10}, {"rssi": -10},
                {"ssid": "Far"}]


api = sa.SettingsApi("http://10.0.0.5", "clawd", api_factory=FakeApi,
                     store=ClockStore(ctx.config_dir / "clock.json"), tab="")

result = {
    "clean": [{"input": i, "result": attempt(sa.clean, i)} for i in CLEAN_INPUTS],
    "clock_view": [{"cfg": c, "view": sa._clock_view(c)} for c in CLOCK_CFGS],
    "diff": [{"cfg": d["cfg"], "edited": d["edited"],
              "changes": config_diff(sa._clock_view(d["cfg"]), sa.clean(d["edited"]))} for d in DIFFS],
    "errors": [{"name": n, "response": sa._err(e)} for n, e in ERRORS],
    "scan": api.scan(),
    "job_idle": api.job(),
}

out = h.write_fixture("settings", "shaping", input={"note": "pure shaping of settings_api.py"},
                      output=result, ctx=ctx)
print(f"wrote {out.relative_to(h.REPO)}")
