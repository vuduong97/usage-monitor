#!/usr/bin/env python3
"""Golden exporter for the clock crate (phase 08): `desktop-rs/fixtures/clock/*.json`.

Runs the REAL `clockapi`, `discovery` and `bootwatch` modules (no network: an in-memory httpx
`MockTransport`, a fake mDNS browser, a hand-driven clock; the harness denies everything else)
and records what the Rust crate must reproduce:

* `config_diff`        - `clockapi.config_diff` cases
* `manual_clock`       - `discovery.manual_clock` / `valid_host` on good and malformed input
* `stored_clock`       - `discovery.stored_clock` on `clock.json` shapes
* `clock_store`        - the exact `clock.json` text `ClockStore.save` writes, and `unpin`
* `finder_sequences`   - `ClockFinder` event sequences (mdns seen/lost, report ok/fail, typed
                         address, use, retry): targets, describe, current, `clock.json` per step
* `api_messages`       - `ClockApi` results and error texts for scripted clock answers
* `bootwatch_steps`    - `BootWatch.step` sequences (restart / jitter / fresh window / retry)
* `heap_url`           - `bootwatch.heap_url`

`ClockStore` writes through `Path.write_text`, which turns `\\n` into `\\r\\n` on Windows: the
recorded file texts are normalised to `\\n` (the Rust crate always writes `\\n`).
"""
from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _harness as h  # noqa: E402

ctx = h.isolate()
import httpx  # noqa: E402

from clawd_desktop import bootwatch as bw  # noqa: E402
from clawd_desktop import clockapi as ca  # noqa: E402
from clawd_desktop import discovery as dsc  # noqa: E402


def clock_dict(c: dsc.Clock | None) -> dict | None:
    if c is None:
        return None
    return {"host": c.host, "port": c.port, "name": c.name, "version": c.version,
            "source": c.source, "base": c.base, "usage_url": c.usage_url, "label": c.label}


def text_of(path: Path) -> str:
    return path.read_bytes().decode().replace("\r\n", "\n")


# ---- config_diff --------------------------------------------------------------------
DIFF_CASES = [
    ("nested_changed_key",
     {"brightness": 45, "rotation": 0, "usage": {"screen": 0, "layout": 0, "anim": 9}},
     {"brightness": 60, "rotation": 0, "usage": {"screen": 0, "layout": 1, "anim": 9}}),
    ("list_goes_whole", {"wifi": [{"ssid": "A"}, {"ssid": "B"}]}, {"wifi": [{"ssid": "B"}, {"ssid": "A"}]}),
    ("equal", {"wifi": [{"ssid": "A"}], "a": {"b": 1}}, {"wifi": [{"ssid": "A"}], "a": {"b": 1}}),
    ("new_key_goes_missing_stays", {"a": 1, "b": 2}, {"a": 1, "c": 3}),
    ("dict_replaces_scalar", {"usage": 5}, {"usage": {"layout": 1}}),
    ("scalar_replaces_dict", {"usage": {"layout": 1}}, {"usage": 5}),
    ("int_equals_float", {"a": 1, "b": {"c": 2.0}}, {"a": 1.0, "b": {"c": 2}}),
    ("bool_equals_int", {"a": 1, "b": 0}, {"a": True, "b": False}),
    ("string_is_not_number", {"a": 1}, {"a": "1"}),
    ("null_values", {"a": None, "b": 1}, {"a": None, "b": None}),
    ("new_is_list", {"a": 1}, [1, 2]),
    ("new_is_scalar", {"a": 1}, "x"),
    ("old_is_not_dict", [1], {"a": 1}),
    ("old_missing_nested", {}, {"usage": {"layout": 1, "screen": 0}}),
    ("deep", {"a": {"b": {"c": {"d": 1, "e": 2}}}}, {"a": {"b": {"c": {"d": 1, "e": 3}}}}),
    ("order_preserved", {"z": 0}, {"z": 1, "a": 2, "m": {"y": 1, "b": 2}}),
    ("dict_key_order_ignored", {"u": {"a": 1, "b": 2}}, {"u": {"b": 2, "a": 1}}),
    ("unicode", {"name": "x"}, {"name": "Đồng hồ ☃"}),
]
h.write_fixture(
    "clock", "config_diff",
    input={"cases": [{"name": n, "old": o, "new": w} for n, o, w in DIFF_CASES]},
    output={"results": [ca.config_diff(o, w) for _n, o, w in DIFF_CASES]}, ctx=ctx)

# ---- manual_clock / valid_host ---------------------------------------------------------
RAWS = [
    "192.168.1.48", "clawd.local:8080", "http://10.0.0.5", "http://10.0.0.5/whatever", " 10.0.0.5 ",
    "", "   ", ":80", "h:abc", "h:0", "h:1", "h:65535", "h:65536", "h:-1", "h:+80", "h: 80", "h:8_0",
    "h:80 ", "h:", "10..97.40.207", "10.97.40", "10.97.40.256", "010.1.1.1", ".clawd", "clawd.",
    "a b", "-x.local", "x-", "x" * 63, "x" * 64, "a." + "x" * 63, "clawd-home.local", "clawd",
    "CLAWD.Local", "http://", "https://clawd.local:443/x?y", "user@host", "10.0.0.5:80:90",
    "ftp://10.0.0.5:21", "1.2.3.4.5", "256.1.1.1", "0.0.0.0", "1e3", "1.2.3.4/", "http://1.2.3.4:81/a/b",  # noqa: S104 (input text, not a bind)
    "Đồng.local", "\u0663.\u0663.\u0663.\u0663", "a_b.local", "a..b", "..", ".", "1", "1.2.3",
    "h:99999999999999999999", "h:80.5", "http://a:b@host", "http:///x", "://host", "HTTP://H:81",
    "a" * 254, ("a" * 60 + ".") * 4 + "com",
]
HOSTS = ["", "10.0.0.5", "10.97.40", "256.1.1.1", "clawd.local", "a" * 253, "a" * 254, "-a", "a-",
         "a-b", "1.2.3.4.5", "0x7f.1", "0.0.0.0", "1234", "a.b.c.d.e.f", "\u0663.\u0663.\u0663.\u0663",
         "1.2.3.4 ", "ab cd", "ab\n"]
h.write_fixture(
    "clock", "manual_clock",
    input={"raws": RAWS, "hosts": HOSTS},
    output={"clocks": [clock_dict(dsc.manual_clock(r)) for r in RAWS],
            "valid_hosts": [dsc.valid_host(x) for x in HOSTS]}, ctx=ctx)

# ---- stored_clock ----------------------------------------------------------------------
STORED = [
    {}, {"host": ""}, {"host": "10.0.0.6"}, {"host": "10.0.0.6", "port": 8080, "name": "clawd", "version": "2.16.0"},
    {"manual_host": "10.0.0.5:8080", "host": "10.0.0.6"},
    {"host": "10.0.0.6", "port": "x", "name": "clawd"},
    {"host": "10.0.0.6", "port": "8080"}, {"host": "10.0.0.6", "port": 8080.0},
    {"host": "10.0.0.6", "port": 0}, {"host": "10.0.0.6", "port": None},
    {"manual_host": "10..97.40.207", "host": "10.0.0.6"},
    {"manual_host": "", "host": "10.0.0.6", "name": None, "version": None},
    {"manual_host": "10.0.0.5"}, {"manual_host": 5, "host": "10.0.0.6"},
    {"host": 7, "name": 5, "version": 2},
    {"name": "clawd"},
]
h.write_fixture(
    "clock", "stored_clock",
    input={"cases": STORED},
    output={"clocks": [clock_dict(dsc.stored_clock(d)) for d in STORED]}, ctx=ctx)

# ---- clock_store ------------------------------------------------------------------------
SAVES = [
    {}, {"name": "clawd", "host": "10.0.0.7", "port": 80, "version": "2.16.0"},
    {"name": "Đồng hồ ☃ \U0001F600", "host": "10.0.0.7", "port": 80, "version": ""},
    {"manual_host": "10.97.40.207", "extra": [1, 2.5, None, True, {"k": []}], "empty": {}},
]
UNPINS = [
    None,
    {"manual_host": "10.97.40.207", "name": "old", "host": "1.2.3.4"},
    {"manual_host": "clawd.local:8080"},
    {"manual_host": "10..97.40.207", "host": "10.0.0.6", "name": "n"},
    {"name": "clawd", "host": "10.0.0.7", "port": 80, "version": "2.16.0"},
    {"host": "10.0.0.7", "port": "x"},
]
store_dir = ctx.home / "store-cases"
save_texts = []
for i, data in enumerate(SAVES):
    p = store_dir / f"save{i}" / "clock.json"
    dsc.ClockStore(p).save(data)
    save_texts.append(text_of(p))
unpin_out = []
for i, data in enumerate(UNPINS):
    p = store_dir / f"unpin{i}" / "clock.json"
    s = dsc.ClockStore(p)
    if data is not None:
        s.save(data)
    clock = s.unpin()
    unpin_out.append({"clock": clock_dict(clock), "file": (text_of(p) if p.exists() else None)})
h.write_fixture("clock", "clock_store", input={"saves": SAVES, "unpins": UNPINS},
                output={"save_texts": save_texts, "unpins": unpin_out}, ctx=ctx)


# ---- finder_sequences ------------------------------------------------------------------
class FakeBrowser:
    def __init__(self, clocks=()):
        self.clocks = {f"{c.name}._clawd._tcp.local.": c for c in clocks}
        self.changed = 0

    def set(self, *clocks):
        self.clocks = {f"{c.name}._clawd._tcp.local.": c for c in clocks}
        self.changed += 1

    def start(self):
        return True

    def stop(self):
        pass

    def names(self):
        return sorted(self.clocks)

    def resolve(self, name):
        return self.clocks.get(name)


class Now:
    def __init__(self):
        self.t = 1000.0

    def __call__(self):
        return self.t


def mk(d: dict) -> dsc.Clock:
    return dsc.Clock(**d)


A = {"host": "10.0.0.7", "port": 80, "name": "clawd", "version": "2.16.0", "source": "mdns"}
OTHER = {"host": "10.0.0.9", "port": 80, "name": "aaa-neighbour", "version": "", "source": "mdns"}
URL_A = "http://10.0.0.7/api/usage"


def fails(url: str, n: int = 3) -> list[dict]:
    return [{"op": "report", "url": url, "ok": False} for _ in range(n)]


SCENARIOS = [
    {"name": "first_mdns_clock", "mdns": [A], "steps": [{"op": "step"}]},
    {"name": "saved_follows_new_address",
     "saved": {"name": "clawd", "host": "192.168.1.48", "port": 80},
     "steps": [{"op": "mdns", "clocks": [dict(A, host="192.168.1.60")]}, {"op": "step"}]},
    {"name": "remembered_wins", "mdns": [OTHER, A],
     "saved": {"name": "clawd", "host": "1.2.3.4"}, "steps": [{"op": "step"}]},
    {"name": "config_targets_bypass", "mdns": [A], "manual": ["http://192.168.0.5/api/usage"],
     "steps": [{"op": "step"}]},
    {"name": "silent_clock", "saved": {"host": "10.97.40.207"},
     "steps": fails("http://10.97.40.207/api/usage", 2) + fails("http://10.97.40.207/api/usage", 1)
     + [{"op": "advance", "secs": 30.0}, {"op": "step"}, {"op": "advance", "secs": 30.0}, {"op": "step"},
        {"op": "advance", "secs": 1.0}, {"op": "step"},
        {"op": "report", "url": "http://10.97.40.207/api/usage", "ok": True}]},
    {"name": "typed_address_wins", "mdns": [A],
     "steps": [{"op": "set_manual", "raw": "not a host:99999"}, {"op": "set_manual", "raw": " 192.168.1.80 "},
               {"op": "step"}] + fails("http://192.168.1.80/api/usage", 3)
     + [{"op": "step"}]},
    {"name": "retry_keeps_address", "saved": {"name": "clawd", "host": "10.97.40.207", "port": 80},
     "steps": fails("http://10.97.40.207/api/usage") + [{"op": "step"}, {"op": "retry"},
                                                        {"op": "advance", "secs": 1.0}, {"op": "step"}]},
    {"name": "retry_unpins_typed", "mdns": [A],
     "steps": [{"op": "set_manual", "raw": "10.97.40.207"}, {"op": "retry"}, {"op": "step"}]
     + fails("http://10.97.40.207/api/usage") + [{"op": "step"}]},
    {"name": "retry_nothing_saved",
     "steps": [{"op": "retry"}, {"op": "mdns", "clocks": [A]}, {"op": "step"}]},
    {"name": "old_scan_keys_dropped",
     "saved": {"name": "", "host": "10.97.40.207", "port": 80, "version": "2.16.0", "subnets": ["10.97.40.0/24"]},
     "steps": [{"op": "report", "url": "http://10.97.40.207/api/usage", "ok": True}]},
    {"name": "use_clock", "saved": {"manual_host": "10.0.0.1"},
     "steps": [{"op": "use", "clock": A}, {"op": "step"}]},
    {"name": "unnamed_saved_takes_mdns_name", "mdns": [A], "saved": {"host": "10.0.0.7", "port": 80},
     "steps": [{"op": "step"}]},
    {"name": "no_saved_two_clocks_first_by_name", "mdns": [dict(OTHER, name="zzz", host="10.0.0.3"), OTHER],
     "steps": [{"op": "step"}]},
    {"name": "mdns_port_and_version", "mdns": [dict(A, port=8080)], "steps": [{"op": "step"}]},
    {"name": "saved_string_port_is_rewritten", "saved": {"name": "clawd", "host": "10.0.0.6", "port": "8080"},
     "steps": [{"op": "report", "url": "http://10.0.0.6:8080/api/usage", "ok": True}]},
    {"name": "unnamed_dropped_then_mdns", "mdns": [],
     "saved": {"host": "10.0.0.6"},
     "steps": fails("http://10.0.0.6/api/usage") + [{"op": "mdns", "clocks": [A]}, {"op": "step"}]},
    {"name": "mdns_clock_lost", "mdns": [A],
     "steps": [{"op": "step"}, {"op": "mdns", "clocks": []}, {"op": "step"},
               {"op": "report", "url": URL_A, "ok": True}]},
    {"name": "report_other_url_stamps_only_last_ok", "mdns": [A],
     "steps": [{"op": "step"}, {"op": "report", "url": "http://10.9.9.9/api/usage", "ok": False},
               {"op": "report", "url": "http://10.9.9.9/api/usage", "ok": True}]},
    {"name": "saved_reported_ok_is_remembered_keeping_name",
     "saved": {"name": "clawd", "host": "10.0.0.6", "port": 80, "version": ""},
     "steps": [{"op": "report", "url": "http://10.0.0.6/api/usage", "ok": True}]},
    {"name": "manual_host_blocks_remember",
     "saved": {"manual_host": "10.0.0.6", "host": "10.0.0.2"},
     "steps": [{"op": "report", "url": "http://10.0.0.6/api/usage", "ok": True}]},
    {"name": "hold_cleared_by_fresh_mdns_answer", "mdns": [A],
     "steps": [{"op": "step"}] + fails(URL_A) + [{"op": "step"}]},
]


def snapshot(f: dsc.ClockFinder, store: dsc.ClockStore, f_now: Now) -> dict:
    d = f.describe()
    return {
        "targets": f.targets(),
        "current": clock_dict(f.current),
        "describe": {"manual_config": d["manual_config"], "clock": clock_dict(d["clock"]),
                     "config_targets": d["config_targets"], "last_ok_set": bool(d["last_ok"]),
                     "failing": d["failing"]},
        "searching": f.searching,
        "clock_json": store.load(),
    }


def run_scenario(sc: dict) -> list[dict]:
    store = dsc.ClockStore(ctx.home / "finder" / sc["name"] / "clock.json")
    if sc.get("saved") is not None:
        store.save(sc["saved"])
    now = Now()
    browser = FakeBrowser([mk(c) for c in sc.get("mdns", [])])
    manual = list(sc.get("manual", []))
    f = dsc.ClockFinder(store, manual=lambda: list(manual), browser=browser, clock=now)
    out = [snapshot(f, store, now)]
    for op in sc["steps"]:
        kind = op["op"]
        extra = {}
        if kind == "step":
            asyncio.run(f.step())
        elif kind == "report":
            f.report(op["url"], op["ok"])
        elif kind == "set_manual":
            extra["returned"] = clock_dict(f.set_manual(op["raw"]))
        elif kind == "use":
            f.use(mk(op["clock"]))
        elif kind == "retry":
            f.retry()
        elif kind == "advance":
            now.t += op["secs"]
        elif kind == "mdns":
            browser.set(*[mk(c) for c in op["clocks"]])
        else:
            raise ValueError(kind)
        out.append({**snapshot(f, store, now), **extra})
    return out


h.write_fixture("clock", "finder_sequences",
                input={"scenarios": SCENARIOS},
                output={"runs": [run_scenario(sc) for sc in SCENARIOS]}, ctx=ctx)


# ---- api_messages --------------------------------------------------------------------------
LONG = "e" * 300
REPR_JSON = {"ok": False, "n": None, "f": 1.5, "s": "it's", "l": [1, 'a"b', True], "u": "Đ", "d": {"x": "y"}}
API_CASES = [
    # name, method, args, (http method, path), response
    ("heap_ok", "heap", [], "/api/heap", {"json": {"uptime": 5}}),
    ("heap_odd", "heap", [], "/api/heap", {"json": [1]}),
    ("heap_not_json", "heap", [], "/api/heap", {"text": "<html>"}),
    ("config_odd", "config", [], "/api/config", {"json": "x"}),
    ("status_null_json", "status", [], "/api/status", {"text": "null"}),
    ("status_empty_body", "status", [], "/api/status", {"text": ""}),
    ("status_http_500_long", "status", [], "/api/status", {"status": 500, "text": LONG}),
    ("status_404", "status", [], "/api/status", {"status": 404, "text": "nope"}),
    ("status_401", "status", [], "/api/status", {"status": 401, "text": "login"}),
    ("export_odd", "export", [], "/api/export", {"json": [1]}),
    ("scan_list", "scan", [], "/api/scan", {"json": [{"ssid": "Home", "rssi": -50, "enc": True}]}),
    ("scan_object", "scan", [], "/api/scan", {"json": {"x": 1}}),
    ("save_empty", "save", [{}], None, None),
    ("save_ok", "save", [{"a": 1}], "/api/config", {"json": {"ok": True, "reboot": False}}),
    ("save_refused_repr", "save", [{"a": 1}], "/api/config", {"json": REPR_JSON}),
    ("save_refused_list", "save", [{"a": 1}], "/api/config", {"json": [1, "a"]}),
    ("save_ok_zero", "save", [{"a": 1}], "/api/config", {"json": {"ok": 0}}),
    ("save_not_json", "save", [{"a": 1}], "/api/config", {"text": "boom"}),
    ("reboot_ok", "reboot", [], "/api/reboot", {"text": "anything"}),
    ("reboot_500", "reboot", [], "/api/reboot", {"status": 500, "text": "bad"}),
    ("firmware_empty", "upload_firmware", [0], None, None),
    ("firmware_too_big", "upload_firmware", [ca.OTA_MAX + 1], None, None),
    ("firmware_ok", "upload_firmware", [1000], "/update", {"text": " OK\n"}),
    ("firmware_refused", "upload_firmware", [10], "/update", {"text": "FAIL: bad image"}),
    ("firmware_refused_long", "upload_firmware", [10], "/update", {"text": LONG}),
    ("pack_unknown", "upload_pack", ["nopak", 5], None, None),
    ("pack_unknown_quote", "upload_pack", ["it's", 5], None, None),
    ("pack_empty", "upload_pack", ["clawdpak", 0], None, None),
    ("pack_too_big", "upload_pack", ["deskpak", ca.PACK_MAX + 1], None, None),
    ("pack_ok", "upload_pack", ["clawdpak", 5], "/api/clawdpak", {"json": {"ok": True, "anims": 12, "crc": 7}}),
    ("pack_no_anims", "upload_pack", ["deskpak", 5], "/api/deskpak", {"json": {"ok": True, "anims": 0}}),
    ("pack_not_ok", "upload_pack", ["deskpak", 5], "/api/deskpak", {"json": REPR_JSON}),
    ("pack_not_json", "upload_pack", ["clawdpak", 5], "/api/clawdpak", {"text": "OK"}),
    ("pack_http_413", "upload_pack", ["clawdpak", 5], "/api/clawdpak", {"status": 413, "text": "too big"}),
    ("style_ok", "style", [], "/api/style", {"json": {"api": 1, "active": False, "slot": 32768}}),
    ("style_odd", "style", [], "/api/style", {"json": [1]}),
    ("install_empty", "install_style", [0], None, None),
    ("install_too_big", "install_style", [ca.STYLE_MAX + 1], None, None),
    ("install_ok", "install_style", [5], "/api/style", {"json": {"ok": True, "style": {"active": True, "id": "synth"}}}),
    ("install_ok_no_style", "install_style", [5], "/api/style", {"json": {"ok": True}}),
    ("install_ok_empty_style", "install_style", [5], "/api/style", {"json": {"ok": True, "style": {}}}),
    ("install_ok_not_json_type", "install_style", [5], "/api/style", {"text": "fine"}),
    ("install_not_ok", "install_style", [5], "/api/style", {"json": {"ok": False, "x": "it's"}}),
    ("install_http_400", "install_style", [5], "/api/style",
     {"status": 400, "json": {"ok": False, "error": "magic", "style": {"active": False}}}),
    ("install_401", "install_style", [5], "/api/style", {"status": 401, "text": "login"}),
    ("clear_ok", "clear_style", [], "/api/style", {"json": {"ok": True, "style": {"active": False}}}),
    ("clear_ok_no_style", "clear_style", [], "/api/style", {"json": {"ok": 1}}),
    ("clear_refused", "clear_style", [], "/api/style", {"json": {"ok": False}}),
    ("clear_odd", "clear_style", [], "/api/style", {"json": [True]}),
    ("file_ok", "get_file", ["/a.bin", 10], "/api/fs/get", {"text": "12345"}),
    ("file_too_big", "get_file", ["/a.bin", 3], "/api/fs/get", {"text": "12345"}),
    ("file_404", "get_file", ["/nope", 10], "/api/fs/get", {"status": 404, "text": "no such file"}),
]


def api_run(method: str, args: list, path: str | None, response: dict | None):
    def handler(req: httpx.Request) -> httpx.Response:
        if path is None or req.url.path != path:
            return httpx.Response(404, text="unexpected " + req.url.path)
        status = response.get("status", 200)
        if "json" in response:
            return httpx.Response(status, json=response["json"])
        return httpx.Response(status, text=response["text"])
    c = ca.ClockApi("http://10.0.0.5", transport=httpx.MockTransport(handler), sleep=lambda _s: None)
    if method in ("upload_firmware", "install_style"):
        args = [b"x" * args[0]]
    elif method == "upload_pack":
        args = [args[0], b"x" * args[1]]
    try:
        value = getattr(c, method)(*args)
    except ca.AuthNeeded as e:
        return {"error": "AuthNeeded", "text": str(e)}
    except ca.ClockError as e:
        return {"error": "ClockError", "text": str(e)}
    if isinstance(value, bytes):
        value = {"bytes": len(value)}
    return {"ok": value}


h.write_fixture(
    "clock", "api_messages",
    input={"cases": [{"name": n, "call": m, "args": a, "path": p, "response": r}
                     for n, m, a, p, r in API_CASES]},
    output={"results": [api_run(m, a, p, r) for _n, m, a, p, r in API_CASES]}, ctx=ctx)

# ---- bootwatch ------------------------------------------------------------------------------
HEAP_URLS = [
    "http://10.0.0.5/api/usage", "http://clawd.local:8080/custom", "http://10.0.0.5/api/usage?x=1#frag",
    "https://h/", "http://[::1]:8080/api/usage", "http://10.0.0.5:80/api/usage", "http://H.Local/x",
    "http://10.0.0.5", "10.0.0.5/api/usage", "http://u:p@10.0.0.5/api/usage",
]
h.write_fixture("clock", "heap_url", input={"urls": HEAP_URLS},
                output={"heap_urls": [bw.heap_url(u) for u in HEAP_URLS]}, ctx=ctx)

BOOT = [
    ("restart_reported_once", [(0, 5000, True), (5, 5005, True), (40, 12, True), (5, 17, True)]),
    ("slow_answers_are_not_restarts", [(0, 5000, True), (20, 5008, True)]),
    ("unreachable_then_back_after_reboot", [(0, 5000, True), (5, None, True), (30, 9, True)]),
    ("new_address_that_just_booted", [(0, 30, True)]),
    ("fresh_boundary_119", [(0, 119, True)]),
    ("fresh_boundary_120", [(0, 120, True)]),
    ("jitter_boundary_exactly_30", [(0, 5000, True), (40, 5010, True)]),
    ("jitter_boundary_just_over", [(0, 5000, True), (40, 5009, True)]),
    ("failed_delivery_retries_while_fresh",
     [(0, 5000, False), (40, 10, False), (5, 15, True), (5, 20, True)]),
    ("failed_delivery_twice_then_ok",
     [(0, 10, False), (5, 15, False), (5, 20, True), (5, 25, True)]),
    ("retries_stop_after_fresh_window", [(0, 100, False), (30, 130, False)]),
    ("retry_survives_a_silent_probe", [(0, 5000, True), (40, 10, False), (5, None, True), (5, 20, True)]),
    ("uptime_zero_counts_as_fresh", [(0, 0, True)]),
    ("two_restarts", [(0, 5000, True), (40, 3, True), (200, 4, True), (60, 2, True)]),
]


def boot_run(steps: list) -> list[dict]:
    url = "http://10.0.0.5/api/usage"
    state = {"uptime": 5000, "delivered": True}
    now = [1000.0]
    hits: list[int] = []

    def handler(req: httpx.Request) -> httpx.Response:
        assert req.url.path == "/api/heap"
        if state["uptime"] is None:
            raise httpx.ConnectTimeout("down", request=req)
        return httpx.Response(200, json={"uptime": state["uptime"], "version": "2.16.0"})

    async def on_restart(u, up):
        hits.append(up)
        return state["delivered"]

    w = bw.BootWatch(lambda: [url], on_restart, transport=httpx.MockTransport(handler), clock=lambda: now[0])
    out = []
    for dt, uptime, delivered in steps:
        now[0] += dt
        state["uptime"], state["delivered"] = uptime, delivered
        restarted = asyncio.run(w.step())
        out.append({"restarted": restarted == [url], "hits": list(hits)})
    return out


h.write_fixture(
    "clock", "bootwatch_steps",
    input={"scenarios": [{"name": n, "steps": [{"dt": dt, "uptime": up, "delivered": ok} for dt, up, ok in s]}
                         for n, s in BOOT]},
    output={"runs": [boot_run(s) for _n, s in BOOT]}, ctx=ctx)

print("wrote desktop-rs/fixtures/clock/*.json")
