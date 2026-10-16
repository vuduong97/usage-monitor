#!/usr/bin/env python3
"""Golden exporter for phase 07 (`clawd-usage`): writes `desktop-rs/fixtures/usage/*.json`.

Runs the REAL `clawd_desktop.usage` / `codex_usage` inside the isolating harness (temp home, no
real keychain, no network: the HTTP probe talks to an `httpx.MockTransport`) and records what the
Rust port must reproduce, number for number:

* `oauth_mapping`   `usage_from_oauth_usage(body, now)` for a Max/Pro account, warnings, locked
                    windows, clamping, string / odd utilization, banker's-rounding boundaries
                    (`.5` percents and minutes) and the "not applicable" bodies.
* `usage_result`    HTTP outcome classification (401/403, 404, not-applicable 400, 5xx, non-JSON,
                    200 with chime/clock fields).
* `probe_headers`   the Haiku probe (`poll_api`): Pro windows, Enterprise spend limit with billing
                    period, missing headers, rounding boundary, 401, 500.
* `payload_text`    `to_ascii` and `encode_payload` (exact bytes, `ensure_ascii`, shortening).
* `attention`       `take_attention_flag` for files of different ages and contents.
* `codex`           `codex_usage.payload_from` and `read` over session logs.

Tokens in fixtures use the `sk-ant-test-*` shape only. Times are fixed; the local-time fields
(`t`, `rd`, `tp`) depend on the machine's zone, so the exporter records `tz_gmtoff` and picks
billing dates at 12:00 UTC mid-month, where every zone from UTC-11 to UTC+11 agrees.

Run: python tools/golden/export_usage.py
"""
from __future__ import annotations

import asyncio
import calendar
import copy
import datetime
import json
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _harness as h  # noqa: E402

ctx = h.isolate()
import httpx  # noqa: E402
from clawd_desktop import codex_usage, usage  # noqa: E402  (after isolate())

REPO = Path(__file__).resolve().parents[2]
FIXTURE = json.loads((REPO / "desktop-rs" / "fixtures" / "usage" / "oauth_usage_max.json").read_text())
# 2026-10-11 09:40 UTC: 2h20m before the 5h reset, 6d0h20m before the weekly one.
NOW = datetime.datetime(2026, 10, 11, 9, 40, tzinfo=datetime.timezone.utc).timestamp()
time.time = lambda: NOW                                   # every `time.time()` of usage.py


def utc(y, mo, d, hh=0, mm=0, ss=0, micro=0):
    return datetime.datetime(y, mo, d, hh, mm, ss, micro, tzinfo=datetime.timezone.utc)


def iso(dt):
    return dt.isoformat()


def write_config(text):
    ctx.config_dir.mkdir(parents=True, exist_ok=True)
    (ctx.config_dir / "config").write_text(text)


def body_with(**changes):
    body = copy.deepcopy(FIXTURE)
    for path, value in changes.items():
        node = body
        *parents, leaf = path.split("__")
        for p in parents:
            node = node[p]
        node[leaf] = value
    return body


def outcome(fn):
    """What a call returned or which exception the Rust side must map (`error` = class name)."""
    try:
        return {"value": fn()}
    except usage.UsageNotApplicable as e:
        return {"error": "UsageNotApplicable", "message": str(e)}
    except usage.TokenExpired:
        return {"error": "TokenExpired"}


# --- oauth_mapping ----------------------------------------------------------------------------

write_config("clock = off\n")
five_at = lambda secs: iso(datetime.datetime.fromtimestamp(NOW + secs, datetime.timezone.utc))  # noqa: E731
mapping_cases = [
    ("max account", FIXTURE),
    ("five_hour locked", body_with(five_hour__locked_reason="session_limit")),
    ("weekly locked", body_with(seven_day__locked_reason="weekly_limit")),
    ("non-normal severity", body_with(limits=[{"kind": "weekly", "severity": "warning"}])),
    ("unknown kind ignored", body_with(limits=[{"kind": "other", "severity": "critical"}])),
    ("severity null ignored", body_with(limits=[{"kind": "session", "severity": None}])),
    ("limits not a list", body_with(limits={"kind": "session", "severity": "warning"})),
    ("no weekly window", dict(FIXTURE, seven_day=None)),
    ("weekly not an object", dict(FIXTURE, seven_day="x")),
    ("clamped, past reset", body_with(five_hour__utilization=130.4, five_hour__resets_at="2020-01-01T00:00:00Z")),
    ("negative utilization", body_with(five_hour__utilization=-3)),
    ("utilization .5 rounds to even (0)", body_with(five_hour__utilization=0.5)),
    ("utilization 1.5 rounds up (2)", body_with(five_hour__utilization=1.5)),
    ("utilization 2.5 rounds down (2)", body_with(five_hour__utilization=2.5)),
    ("utilization 7.5 (8)", body_with(five_hour__utilization=7.5, seven_day__utilization=8.5)),
    ("utilization string", body_with(five_hour__utilization="12.5")),
    ("utilization garbage string", body_with(five_hour__utilization="n/a", seven_day__utilization=[1])),
    ("utilization zero keeps window", body_with(five_hour__utilization=0)),
    ("utilization true", body_with(five_hour__utilization=True)),
    ("minutes 2.5 rounds to even (2)", body_with(five_hour__resets_at=five_at(150))),
    ("minutes 1.5 rounds up (2)", body_with(five_hour__resets_at=five_at(90))),
    ("minutes 0.5 (0)", body_with(five_hour__resets_at=five_at(30))),
    ("minutes just under 1 (1)", body_with(five_hour__resets_at=five_at(59))),
    ("reset in the past", body_with(five_hour__resets_at=five_at(-600))),
    ("reset exactly now", body_with(five_hour__resets_at=five_at(0))),
    ("reset z-suffix", body_with(five_hour__resets_at="2026-10-11T12:00:00Z")),
    ("reset with microseconds", body_with(five_hour__resets_at="2026-10-11T12:00:00.287293+00:00")),
    ("reset with +07:00 offset", body_with(five_hour__resets_at="2026-10-11T19:00:00+07:00")),
    ("reset not a string", body_with(five_hour__resets_at=12345)),
    ("reset garbage", body_with(five_hour__resets_at="soon")),
    ("reset empty", body_with(five_hour__resets_at="")),
    ("reset missing", {"five_hour": {"utilization": 3.0}}),
    ("no body fields", {"five_hour": {"utilization": 3.0}, "seven_day": {}}),
    ("empty object", {}),
    ("list", []),
    ("null", None),
    ("five_hour null", {"five_hour": None}),
    ("five_hour utilization null", {"five_hour": {"utilization": None}}),
    ("five_hour not an object", {"five_hour": 7}),
]
mapping = []
for name, body in mapping_cases:
    mapping.append({"name": name, "body": body,
                    "result": outcome(lambda b=body: usage.usage_from_oauth_usage(b, now=NOW))})

# --- usage_result -----------------------------------------------------------------------------

result_cases = [
    ("401", 401, "{}", "clock = off\n"),
    ("403", 403, "{}", "clock = off\n"),
    ("404", 404, "not found", "clock = off\n"),
    ("400 not applicable", 400, '{"error":{"message":"Usage limits are Not Applicable to API organizations"}}', "clock = off\n"),
    ("400 other", 400, "bad request", "clock = off\n"),
    ("429", 429, "slow down", "clock = off\n"),
    ("500", 500, "boom", "clock = off\n"),
    ("200 html", 200, "<html>", "clock = off\n"),
    ("200 no windows", 200, "{}", "clock = off\n"),
    ("200 ok, clock off", 200, json.dumps(FIXTURE), "clock = off\n"),
    ("200 ok, clock 24", 200, json.dumps(FIXTURE), "clock = 24\n"),
    ("200 ok, clock 12 + chime", 200, json.dumps(FIXTURE), "clock = 12\nchime = on\n"),
    ("200 ok, chime only", 200, json.dumps(FIXTURE), "clock = off\nchime = ON\n"),
]
results = []
for name, status, text, config in result_cases:
    write_config(config)
    res = outcome(lambda s=status, t=text: usage._usage_result(s, t))
    entry = {"name": name, "status": status, "text": text, "config": config, "result": res}
    if res.get("value") and "t" in res["value"]:
        entry["tz_gmtoff"] = time.localtime().tm_gmtoff
    results.append(entry)

# --- probe_headers ----------------------------------------------------------------------------

reset_ent = utc(2026, 10, 15, 12).timestamp()             # period 2026-09-15 12:00 .. 2026-10-15 12:00 UTC (now is inside)
reset_ent_jan = utc(2026, 10, 31, 12).timestamp()         # September has 30 days: the start day clamps to the 30th
PRO = {
    "anthropic-ratelimit-unified-5h-utilization": "0.07",
    "anthropic-ratelimit-unified-5h-reset": str(int(NOW + 8400)),
    "anthropic-ratelimit-unified-7d-utilization": "0.08",
    "anthropic-ratelimit-unified-7d-reset": str(int(NOW + 6 * 86400 + 1200)),
    "anthropic-ratelimit-unified-5h-status": "allowed_warning",
}
probe_cases = [
    ("pro windows", 200, PRO),
    ("pro windows, no status header", 200, {k: v for k, v in PRO.items() if not k.endswith("status")}),
    ("pro rounding .5 to even", 200, dict(PRO, **{"anthropic-ratelimit-unified-5h-utilization": "0.125",
                                                  "anthropic-ratelimit-unified-7d-utilization": "0.135"})),
    ("pro minutes boundary", 200, dict(PRO, **{"anthropic-ratelimit-unified-5h-reset": str(NOW + 150),
                                               "anthropic-ratelimit-unified-7d-reset": str(NOW + 90)})),
    ("pro garbage numbers", 200, dict(PRO, **{"anthropic-ratelimit-unified-5h-utilization": "lots",
                                              "anthropic-ratelimit-unified-5h-reset": "soon"})),
    ("pro reset in the past", 200, dict(PRO, **{"anthropic-ratelimit-unified-5h-reset": str(int(NOW - 5))})),
    ("enterprise spend limit", 200, {
        "anthropic-ratelimit-unified-overage-utilization": "0.426",
        "anthropic-ratelimit-unified-overage-reset": str(int(reset_ent)),
        "anthropic-ratelimit-unified-status": "allowed"}),
    ("enterprise, 31st clamps to a 30-day month", 200, {
        "anthropic-ratelimit-unified-overage-utilization": "0.5",
        "anthropic-ratelimit-unified-overage-reset": str(int(reset_ent_jan)),
        "anthropic-ratelimit-unified-status": "allowed_warning"}),
    ("enterprise, no reset header", 200, {"anthropic-ratelimit-unified-overage-utilization": "0.1"}),
    ("no ratelimit headers at all", 200, {}),
    ("401", 401, {}),
    ("403", 403, {}),
    ("429", 429, {}),
    ("500", 500, {}),
]
real_client = httpx.AsyncClient
probes = []
for name, status, headers in probe_cases:
    for config in ("clock = off\n", "clock = 24\nchime = on\n"):
        if config != "clock = off\n" and name not in ("pro windows", "enterprise spend limit"):
            continue
        write_config(config)

        def handler(request, status=status, headers=headers):
            assert request.headers["authorization"] == "Bearer sk-ant-test-probe"
            return httpx.Response(status, headers=headers, json={"id": "msg_test"})

        usage.httpx.AsyncClient = lambda **kw: real_client(transport=httpx.MockTransport(handler), **kw)
        try:
            res = outcome(lambda: asyncio.run(usage.poll_api("sk-ant-test-probe")))
        finally:
            usage.httpx.AsyncClient = real_client
        entry = {"name": name, "status": status, "headers": headers, "config": config, "result": res}
        if res.get("value") and "t" in res["value"]:
            entry["tz_gmtoff"] = time.localtime().tm_gmtoff
        probes.append(entry)

# --- payload_text -----------------------------------------------------------------------------

ascii_cases = [
    ("Você  aprova\na ação?", 120), ("x" * 200, 120), ("", 120), ("  spaced \t out\r\ntext  ", 120),
    ("Đồng hồ ☃ ﬁne ① ½", 120), ("a" * 10, 10), ("a" * 11, 10), ("ab cd ef gh", 8), ("ab        cd", 6),
    ("\x1c\x1d x \x1f", 120), ("日本語", 120), ("x" * 5, 2), ("hello world", 3),
]
ascii_out = [{"text": t, "limit": n, "result": usage.to_ascii(t, n)} for t, n in ascii_cases]
base = {"s": 4, "ok": True, "b": 1}
encode_cases = [
    ({**base, "m": "hi"}, 180), ({**base, "m": "x" * 200}, 120), ({**base, "m": "x" * 200}, 26),
    ({**base, "m": "x" * 200}, 60), ({**base, "m": "word " * 30}, 90), ({**base}, 180),
    ({**base, "m": "Đồng hồ"}, 180), ({"m": "ä" * 50, "s": 1}, 40),
    ({"s": 5, "m": "a b c d e f g h i j k l m n o p q r s t u v w x y z", "t": 1800000000, "tf": 24}, 70),
    ({"ok": False, "why": "login", "m": "alert", "t": 1800000000, "tf": 12}, 180),
]
encode_out = []
for payload, limit in encode_cases:
    data = usage.encode_payload(payload, limit)
    encode_out.append({"payload": payload, "limit": limit, "bytes": data.decode("ascii")})

# --- attention --------------------------------------------------------------------------------

attention_cases = [
    ("bare touch", "", 0),
    ("alert with card", json.dumps({"m": "Você  aprova\na ação?", "s": "abc"}), 5),
    ("fresh, 59 s", json.dumps({"m": "x", "s": "a"}), 59),
    ("stale, 60 s", json.dumps({"m": "x", "s": "a"}), 60),
    ("stale, 61 s", json.dumps({"m": "x", "s": "a"}), 61),
    ("not json", "needs you", 1),
    ("json list", "[1, 2]", 1),
    ("message number, session missing", json.dumps({"m": 42}), 1),
    ("long message", json.dumps({"m": "word " * 60, "s": "s1"}), 1),
    ("falsy fields", json.dumps({"m": None, "s": 0}), 1),
]
flag = ctx.config_dir / "attention"
attention = []
for name, text, age in attention_cases:
    ctx.config_dir.mkdir(parents=True, exist_ok=True)
    flag.write_text(text)
    mtime = NOW - age
    os.utime(flag, (mtime, mtime))
    res = usage.take_attention_flag()
    attention.append({"name": name, "text": text, "age": age, "now": NOW, "result": res,
                      "consumed": not flag.exists()})

# --- codex ------------------------------------------------------------------------------------

CNOW = 1_783_000_000


def row(rl, ts="2026-07-02T17:50:57.643Z"):
    return json.dumps({"timestamp": ts, "type": "event_msg",
                       "payload": {"type": "token_count", "info": None, "rate_limits": rl}})


LIMITS = {"limit_id": "codex", "primary": {"used_percent": 12.4, "window_minutes": 300, "resets_at": CNOW + 3600},
          "secondary": {"used_percent": 43.0, "window_minutes": 10080, "resets_at": CNOW + 86400 * 2},
          "plan_type": "plus", "rate_limit_reached_type": None}
win = lambda **kw: {"used_percent": 5, "window_minutes": 300, "resets_at": CNOW + 600, **kw}  # noqa: E731
payload_cases = [
    ("windows by length", LIMITS, CNOW, CNOW),
    ("past reset reads zero, limit reached", {**LIMITS, "primary": {**LIMITS["primary"], "resets_at": CNOW - 1},
                                              "rate_limit_reached_type": "primary"}, CNOW, CNOW),
    ("older resets_in_seconds", {"primary": {"used_percent": 5, "window_minutes": 300, "resets_in_seconds": 600}}, CNOW, CNOW + 60),
    ("usage based plan", {**LIMITS, "primary": None, "secondary": None}, CNOW, CNOW),
    ("warning at 90", {**LIMITS, "primary": win(used_percent=90.4)}, CNOW, CNOW),
    ("percent .5 to even", {"primary": win(used_percent=12.5), "secondary": win(used_percent=13.5, window_minutes=10080)}, CNOW, CNOW),
    ("minutes .5 to even", {"primary": win(resets_at=CNOW + 150)}, CNOW, CNOW),
    ("only weekly", {"secondary": win(window_minutes=10080)}, CNOW, CNOW),
    ("both short, first wins", {"primary": win(used_percent=1), "secondary": win(used_percent=2)}, CNOW, CNOW),
    ("no window minutes counts as long", {"primary": win(window_minutes=0)}, CNOW, CNOW),
    ("window minutes string", {"primary": win(window_minutes="300")}, CNOW, CNOW),
    ("bad window skipped", {"primary": win(window_minutes="x"), "secondary": win(window_minutes=10080)}, CNOW, CNOW),
    ("used percent null", {"primary": win(used_percent=None)}, CNOW, CNOW),
    ("no reset time", {"primary": {"used_percent": 7, "window_minutes": 300}}, CNOW, CNOW),
    ("clamped", {"primary": win(used_percent=250)}, CNOW, CNOW),
]
codex_payloads = [{"name": n, "rate_limits": rl, "written": w, "now": now,
                   "result": codex_usage.payload_from(rl, w, now=now)} for n, rl, w, now in payload_cases]

# Session logs: (relative path under sessions/, lines, mtime). Newest folder first, newest file first.
SESSIONS = {
    "newest session wins, null-window and broken rows skipped": [
        ("2026/07/01/rollout-a.jsonl", [row({**LIMITS, "primary": {**LIMITS["primary"], "used_percent": 1}})], CNOW - 900),
        ("2026/07/02/rollout-b.jsonl",
         [row(LIMITS), '{"type":"response_item"}', row({**LIMITS, "primary": None, "secondary": None}), "{broken"],
         CNOW - 100),
    ],
    "falls back to an older session when the newest has no limits": [
        ("2026/07/01/rollout-a.jsonl", [row(LIMITS)], CNOW - 900),
        ("2026/07/02/rollout-b.jsonl", ['{"type":"response_item"}'], CNOW - 100),
    ],
    "other limit id ignored": [
        ("2026/07/02/rollout-a.jsonl", [row({**LIMITS, "limit_id": "other"})], CNOW - 100),
    ],
    "newest file within a day by mtime": [
        ("2026/07/02/rollout-old.jsonl", [row({**LIMITS, "primary": {**LIMITS["primary"], "used_percent": 1}})], CNOW - 500),
        ("2026/07/02/rollout-new.jsonl", [row({**LIMITS, "primary": {**LIMITS["primary"], "used_percent": 2}})], CNOW - 50),
    ],
    "row timestamp drives resets_in_seconds": [
        ("2026/07/02/rollout-a.jsonl",
         [row({"primary": {"used_percent": 3, "window_minutes": 300, "resets_in_seconds": 7200}}, ts="2026-07-02T17:50:57.643Z")],
         CNOW - 100),
    ],
    "no sessions": [],
}
codex_reads = []
for name, files in SESSIONS.items():
    root = ctx.home / ".codex" / "sessions"
    if root.exists():
        for p in sorted(root.rglob("*"), reverse=True):
            p.unlink() if p.is_file() else p.rmdir()
    for rel, lines, mtime in files:
        f = root / rel
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text("\n".join(lines) + "\n")
        os.utime(f, (mtime, mtime))
    codex_reads.append({
        "name": name, "files": [{"path": r, "lines": ln, "mtime": m} for r, ln, m in files], "now": CNOW,
        "result": codex_usage.read(home=ctx.home, env={}, now=CNOW),
        "available": codex_usage.available(home=ctx.home, env={}),
    })

outputs = [
    ("oauth_mapping", {"now": NOW}, mapping),
    ("usage_result", {"now": NOW}, results),
    ("probe_headers", {"now": NOW, "token": "sk-ant-test-probe"}, probes),
    ("payload_text", {}, {"to_ascii": ascii_out, "encode_payload": encode_out}),
    ("attention", {"max_age": usage.ATTENTION_MAX_AGE}, attention),
    ("codex", {"now": CNOW}, {"payload_from": codex_payloads, "read": codex_reads}),
]
for name, inp, out in outputs:
    path = h.write_fixture("usage", name, input=inp, output=out, ctx=ctx)
    print(f"wrote {path.relative_to(h.REPO)}")
