#!/usr/bin/env python3
"""Golden exporter for phase 10 (`clawd-appupdate`): runs the REAL `clawd_desktop.appupdate` and
records `{input, output}` fixtures in `desktop-rs/fixtures/appupdate/`.

Isolation (harness): no real home, no network (GitHub is an in-memory `httpx.MockTransport`), no
real process (`codesign`/`ditto` are scripted fakes, `subprocess.Popen` is replaced by a recorder:
the installer and `swap.sh` are RECORDED, never executed).

The fake release site follows one rule (the Rust tests rebuild it from the same input):
  API_LATEST            -> the `release` JSON of the case
  BASE + "SHA256SUMS"   -> 302 CDN + "sums";  CDN + "sums" -> the sums text
  BASE + <file>         -> 302 CDN + <file>;  CDN + <file>  -> its text (`tamper`: last char -> "X")
  `routes` (explicit)   -> win over all of the above: {url: {status, location?, text?}}
"""
from __future__ import annotations

import dataclasses
import hashlib
import io
import os
import plistlib
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _harness as h

# appupdate.log() prints Vietnamese; the Windows console is cp1252
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", line_buffering=True)
ctx = h.isolate()

import httpx

from clawd_desktop import appupdate as au
from clawd_desktop import paths, usage

API_LATEST = "https://api.github.com/repos/o/r/releases/latest"
BASE = "https://github.com/o/r/releases/download/v0.2.0/"
CDN = "https://release-assets.githubusercontent.com/o/"
EXE = "Clawd-0.2.0-windows-setup.exe"
ZIP = "Clawd-0.2.0-mac-arm64.zip"
WORK = ctx.root / "work"
WORK.mkdir()
_counter = [0]


def fresh_dir(name: str) -> Path:
    _counter[0] += 1
    d = WORK / f"{_counter[0]}-{name}"
    d.mkdir(parents=True)
    return d


def txt(v) -> str:
    """A text in a fixture is a string or {"repeat": s, "count": n, "prefix"?, "suffix"?} (big bodies)."""
    if isinstance(v, str):
        return v
    return v.get("prefix", "") + v["repeat"] * v["count"] + v.get("suffix", "")


def rep(s: str, n: int, prefix: str = "", suffix: str = "") -> dict:
    return {"repeat": s, "count": n, "prefix": prefix, "suffix": suffix}


def sha(text) -> str:
    return hashlib.sha256(txt(text).encode()).hexdigest()


def release_json(files: dict[str, str], version="0.2.0", **change) -> dict:
    base = f"https://github.com/o/r/releases/download/v{version}/"
    return {"tag_name": f"v{version}", "draft": False, "prerelease": False, "body": "Sửa lỗi xoay màn hình",
            "html_url": f"https://github.com/o/r/releases/tag/v{version}",
            "assets": [{"name": n, "size": len(t.encode()), "browser_download_url": base + n} for n, t in files.items()]
                      + [{"name": au.SUMS_NAME, "size": 100, "browser_download_url": base + au.SUMS_NAME}],
            **change}


def sums_of(files: dict[str, str]) -> str:
    return "".join(f"{sha(t)}  {n}\n" for n, t in files.items())


class Site:
    """The fake GitHub: counts calls, answers by the rule in the module docstring."""

    def __init__(self, case: dict):
        self.case, self.calls = case, []
        self.offline = bool(case.get("offline"))

    def handler(self, request: httpx.Request) -> httpx.Response:
        url = str(request.url)
        self.calls.append(url)
        if self.offline:
            raise httpx.ConnectError("no network")
        c = self.case
        extra = c.get("routes", {}).get(url)
        if extra is not None:
            return reply(extra)
        files = c.get("files", {})
        if url == API_LATEST and "release" in c:
            return httpx.Response(200, json=c["release"])
        if url == BASE + au.SUMS_NAME:
            return httpx.Response(302, headers={"location": CDN + "sums"})
        if url == CDN + "sums":
            text = c["sums_override"] if c.get("sums_override") is not None else sums_of(files)
            return httpx.Response(200, text=txt(text))
        for name, text in files.items():
            if url == BASE + name:
                return httpx.Response(302, headers={"location": CDN + name})
            if url == CDN + name:
                data = txt(text).encode()
                return httpx.Response(200, content=data[:-1] + b"X" if name in c.get("tamper", []) else data)
        return httpx.Response(404)

    def source(self, api="https://api.github.com") -> au.Source:
        return au.Source("o/r", api, transport=httpx.MockTransport(self.handler))


def reply(spec: dict) -> httpx.Response:
    headers = {"location": spec["location"]} if "location" in spec else {}
    return httpx.Response(spec["status"], headers=headers, content=txt(spec.get("text", "")).encode())


def out(name: str, input, output) -> None:
    path = h.write_fixture("appupdate", name, input=input, output=output, ctx=ctx)
    print(f"wrote {path.relative_to(h.REPO)}")


def proc(returncode: int = 0, stderr: str = ""):
    """What a scripted `codesign` / `ditto` answers."""
    return au.subprocess.CompletedProcess([], returncode, "", stderr)


def error_of(fn):
    try:
        return {"value": fn()}
    except au.UpdateError as e:
        return {"error": str(e)}


# ---- pure helpers -----------------------------------------------------------------------------
VERSIONS = ["v1.2.3", "0.10.0", "1.2", "v1.2.3-rc1", "1.2.3.4", "latest", "", None, 5, " v0.2.0 ", "V1.2.3",
            "v01.02.03", "12345.1.1", "0.0.0", "v9999.9999.9999", "1. 2.3", "v1.2.3\n"]
NEWER = [["v0.10.0", "0.9.9"], ["0.1.0", "0.1.0"], ["0.1.0", "0.2.0"], ["v1.0.0-rc1", "0.1.0"], ["0.2.0", "0.1.0"],
         ["1.2", "0.1.0"], ["0.2.0", "garbage"], ["v0.2.0", "v0.2.0"]]
out("versions", {"parse": VERSIONS, "newer": NEWER},
    {"parse": [au.parse_version(v) for v in VERSIONS], "newer": [au.is_newer(a, b) for a, b in NEWER]})

ASSETS = [["1.2.3", "mac", "arm64"], ["1.2.3", "mac", "x86_64"], ["1.2.3", "windows", "x86_64"],
          ["1.2.3", "windows", "arm64"], ["1.2.3", "windows", ""], ["1.2.3", "linux", "x86_64"],
          ["1.2.3", "mac", "ppc"], ["1.2.3", "mac", ""], ["0.2.0", "mac", "arm64"]]
out("asset_names", {"cases": ASSETS}, {"names": [au.asset_name(*a) for a in ASSETS]})

H = "a" * 64
SUMS = [f"{H}  one.zip\n{H.upper()} *two.exe\nbad  three\n", "", f"{H}\n", f"{H}  \n", f"  {H}   spaced name.zip  \n",
        f"{'b' * 63}  short.zip\n{'b' * 65}  long.zip\n", f"{H}  dup\n{'c' * 64}  dup\n", f"{H} **star.zip\n",
        f"{H}  a\r\n{'d' * 64}  b\r\n", f"{H}\t tab.zip\n", f"{'g' * 64}  nothex.zip\n", f"{H}  x\x0b{'e' * 64}  y\n"]
out("parse_sums", {"texts": SUMS}, {"tables": [au.parse_sums(t) for t in SUMS]})

TEAMS = ["Identifier=x\nTeamIdentifier=ABCDE12345\n", "TeamIdentifier=not set\n", "", "TeamIdentifier=not\n",
         "TeamIdentifier=ABC\r\n", "x TeamIdentifier=ABC\n", "TeamIdentifier=\n", "  TeamIdentifier=ABC\n",
         "TeamIdentifier=not set\nTeamIdentifier=ZZZZZ99999\n", "TeamIdentifier=A B\nTeamIdentifier=C\n",
         "Authority=Developer ID\nTeamIdentifier=ABCDE12345"]
out("team_id", {"texts": TEAMS}, {"teams": [au.team_id(t) for t in TEAMS]})

# ---- trusted() ----------------------------------------------------------------------------------
URLS = ["https://api.github.com/repos/o/r/releases/latest", "https://github.com/o/r/releases/download/v1/x",
        "https://release-assets.githubusercontent.com/o/x", "https://objects.githubusercontent.com/x",
        "https://raw.githubusercontent.com/o/r/main/x", "https://gist.githubusercontent.com/o/x",
        "https://githubusercontent.com/x", "https://evilgithubusercontent.com/x", "https://github.com.evil.example/x",
        "https://evil.example/github.com", "https://github.com@evil.example/x", "https://API.GITHUB.COM/x",
        "https://github.com:8443/x", "https://github.com./x", "http://github.com/x", "http://api.github.com/x",
        "http://127.0.0.1:8080/x", "http://localhost:1/x", "https://127.0.0.1/x", "http://127.0.0.2/x",
        "http://[::1]:1/x", "http://evil.example/x", "ftp://github.com/x", "file:///etc/passwd",
        "github.com/x", "", "https://", "https:///x", "HTTPS://github.com/x", "https://sub.github.com/x"]
APIS = ["https://api.github.com", "http://127.0.0.1:9999", "http://localhost:9999", "https://example.test/api"]
trusted_cases, trusted_out = [], []
for api in APIS:
    src = au.Source("o/r", api, transport=httpx.MockTransport(lambda r: httpx.Response(404)))
    for url in URLS:
        trusted_cases.append({"api": api, "url": url})
        trusted_out.append(src.trusted(url))
out("trusted", {"cases": trusted_cases}, {"trusted": trusted_out})

# ---- latest() -----------------------------------------------------------------------------------
def exe_files(text="installer"):
    return {EXE: text}


def case(name, files=None, release=None, system="windows", arch="x86_64", current="0.1.0", **more):
    files = exe_files() if files is None else files
    rel = release if release is not None else release_json(files)
    return {"name": name, "files": files, "release": rel, "system": system, "arch": arch, "current": current, **more}


def with_release(files=None, **change):
    files = exe_files() if files is None else files
    return release_json(files, **change)


def no_sums(files=None):
    rel = release_json(exe_files() if files is None else files)
    rel["assets"] = [a for a in rel["assets"] if a["name"] != au.SUMS_NAME]
    return rel


def asset_edit(**change):
    rel = release_json(exe_files())
    rel["assets"][0].update(change)
    return rel


LATEST = [
    case("ok_windows"),
    case("ok_mac", files={ZIP: "zip-bytes"}, system="mac", arch="arm64"),
    case("draft", release=with_release(draft=True)),
    case("prerelease", release=with_release(prerelease=True)),
    case("tag_nightly", release=with_release(tag_name="nightly")),
    case("tag_rc", release=with_release(tag_name="v0.2.0-rc1")),
    case("tag_padded", release=with_release(tag_name=" v0.2.0 ")),
    case("tag_missing", release=with_release(tag_name=None)),
    case("already_current", current="0.2.0"),
    case("running_newer", current="0.3.0"),
    case("no_mac_file", system="mac", arch="arm64"),
    case("linux", system="linux"),
    case("no_arch_given", system="windows", arch=""),
    case("sums_asset_missing", release=no_sums()),
    case("sums_lack_the_file", sums_override=f"{'0' * 64}  something-else.exe\n"),
    case("sums_binary_mode", sums_override=f"{sha('installer').upper()} *{EXE}\n"),
    case("evil_asset_url", release=asset_edit(browser_download_url="https://evil.example/Clawd.exe")),
    case("asset_url_missing", release=asset_edit(browser_download_url=None)),
    case("size_zero", release=asset_edit(size=0)),
    case("size_bool", release=asset_edit(size=True)),
    case("size_float", release=asset_edit(size=9.0)),
    case("size_string", release=asset_edit(size="9")),
    case("size_negative", release=asset_edit(size=-1)),
    case("size_too_big", release=asset_edit(size=500 * 1024 * 1024 + 1)),
    case("size_max", release=asset_edit(size=500 * 1024 * 1024)),
    case("notes_long_and_padded", release=with_release(body="  " + "x" * 2500 + "  ")),
    case("notes_null", release=with_release(body=None)),
    case("notes_vietnamese_cut", release=with_release(body="ế" * 2100)),
    case("html_url_missing", release=with_release(html_url=None)),
    case("sums_redirect_foreign", routes={BASE + au.SUMS_NAME: {"status": 302, "location": "https://evil.example/sums"}}),
    case("sums_redirect_loop", routes={BASE + au.SUMS_NAME: {"status": 302, "location": BASE + au.SUMS_NAME}}),
    case("sums_redirect_relative", routes={BASE + au.SUMS_NAME: {"status": 302, "location": "/o/r/sums-2"},
                                           "https://github.com/o/r/sums-2": {"status": 200, "text": f"{sha('installer')}  {EXE}\n"}}),
    case("sums_404", routes={BASE + au.SUMS_NAME: {"status": 404}}),
    case("sums_500", routes={CDN + "sums": {"status": 500}}),
    case("sums_too_big", routes={CDN + "sums": {"status": 200, "text": rep("a", 64 * 1024 + 1)}}),
    case("sums_exactly_64k", routes={CDN + "sums": {"status": 200, "text": rep("#", 64 * 1024 - 100, prefix=f"{sha('installer')}  {EXE}\n")}}),
    case("api_404", routes={API_LATEST: {"status": 404}}),
    case("api_403", routes={API_LATEST: {"status": 403}}),
    case("api_429", routes={API_LATEST: {"status": 429}}),
    case("api_500", routes={API_LATEST: {"status": 500}}),
    case("api_not_json", routes={API_LATEST: {"status": 200, "text": "<html>"}}),
    case("api_json_list", routes={API_LATEST: {"status": 200, "text": "[1, 2]"}}),
    case("api_empty_object", routes={API_LATEST: {"status": 200, "text": "{}"}}),
    case("api_too_large", routes={API_LATEST: {"status": 200, "text": rep(" ", 2 * 1024 * 1024 + 1)}}),
]
latest_out = []
for c in LATEST:
    site = Site(c)
    try:
        rel = site.source().latest(c["current"], c["system"], c["arch"])
        latest_out.append({"release": dataclasses.asdict(rel) if rel else None})
    except au.UpdateError as e:
        latest_out.append({"error": str(e)})
out("latest", {"cases": LATEST}, {"results": latest_out})


# ---- download() ---------------------------------------------------------------------------------
def dl_rel(text, **change):
    data = txt(text).encode()
    base = {"version": "0.2.0", "tag": "v0.2.0", "notes": "", "page": "", "name": EXE, "url": BASE + EXE,
            "size": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    base.update(change)
    return base


PAYLOAD = rep("installer-bytes", 1000)
TAMPERED = rep("installer-bytes", 999, suffix="installer-byteX")
GOOD = {BASE + EXE: {"status": 302, "location": CDN + EXE}, CDN + EXE: {"status": 200, "text": PAYLOAD}}
DOWNLOADS = [
    {"name": "ok", "rel": dl_rel(PAYLOAD), "routes": GOOD},
    {"name": "tampered_same_size", "rel": dl_rel(PAYLOAD), "routes": {**GOOD, CDN + EXE: {"status": 200, "text": TAMPERED}}},
    {"name": "bigger_than_announced", "rel": dl_rel(PAYLOAD, size=10), "routes": GOOD},
    {"name": "smaller_than_announced", "rel": dl_rel(PAYLOAD, size=len(txt(PAYLOAD)) + 5), "routes": GOOD},
    {"name": "foreign_first_url", "rel": dl_rel(PAYLOAD, url="https://evil.example/x"), "routes": GOOD},
    {"name": "foreign_redirect", "rel": dl_rel(PAYLOAD), "routes": {**GOOD, BASE + EXE: {"status": 302, "location": "https://evil.example/Clawd.exe"}}},
    {"name": "http_redirect_non_loopback_api", "rel": dl_rel(PAYLOAD), "routes": {**GOOD, BASE + EXE: {"status": 302, "location": "http://127.0.0.1/x"}}},
    {"name": "redirect_loop", "rel": dl_rel(PAYLOAD), "routes": {**GOOD, BASE + EXE: {"status": 302, "location": BASE + EXE}}},
    {"name": "three_hops_then_file", "rel": dl_rel(PAYLOAD), "routes": {
        BASE + EXE: {"status": 302, "location": CDN + "a"}, CDN + "a": {"status": 301, "location": CDN + "b"},
        CDN + "b": {"status": 307, "location": CDN + EXE}, CDN + EXE: {"status": 200, "text": PAYLOAD}}},
    {"name": "four_redirects_is_too_many", "rel": dl_rel(PAYLOAD), "routes": {
        BASE + EXE: {"status": 302, "location": CDN + "a"}, CDN + "a": {"status": 302, "location": CDN + "b"},
        CDN + "b": {"status": 302, "location": CDN + "c"}, CDN + "c": {"status": 302, "location": CDN + EXE},
        CDN + EXE: {"status": 200, "text": PAYLOAD}}},
    {"name": "http_404", "rel": dl_rel(PAYLOAD), "routes": {**GOOD, CDN + EXE: {"status": 404}}},
    {"name": "http_500", "rel": dl_rel(PAYLOAD), "routes": {**GOOD, CDN + EXE: {"status": 500}}},
    {"name": "redirect_without_location", "rel": dl_rel(PAYLOAD), "routes": {**GOOD, BASE + EXE: {"status": 302}}},
    {"name": "http_204", "rel": dl_rel(PAYLOAD), "routes": {**GOOD, CDN + EXE: {"status": 204}}},
    {"name": "empty_body_zero_size_announced_one", "rel": dl_rel("x"), "routes": {**GOOD, CDN + EXE: {"status": 200, "text": ""}}},
]
def run_download(c: dict) -> dict:
    d = fresh_dir("download")
    dest = d / EXE
    seen: list = []
    site = Site({"routes": c["routes"]})
    rel = au.AppRelease(**c["rel"])
    res = error_of(lambda: site.source().download(rel, dest, lambda got, total: seen.append([got, total])))
    return {
        "error": res.get("error"),
        "dest_sha256": h.sha256_of(dest.read_bytes()) if dest.exists() else None,
        "dir": sorted(p.name for p in d.iterdir()),
        "last_progress": seen[-1] if seen else None,
        "asked": site.calls,
    }


out("download", {"cases": DOWNLOADS}, {"results": [run_download(c) for c in DOWNLOADS]})


# ---- check_bundle() / bundle_team() -------------------------------------------------------------
def plist_bytes(bundle_id, version):
    return plistlib.dumps({"CFBundleIdentifier": bundle_id, "CFBundleShortVersionString": version})


BUNDLES = [
    {"name": "ok"},
    {"name": "other_team", "new_team": "OTHERTEAM01"},
    {"name": "new_is_ad_hoc", "new_team": None},
    {"name": "verify_fails", "verify_rc": 1},
    {"name": "other_version", "bundle_version": "0.1.9"},
    {"name": "version_missing", "bundle_version": None},
    {"name": "other_bundle_id", "bundle_id": "com.evil.app"},
    {"name": "running_ad_hoc", "running_team": None},
    {"name": "running_absent", "running": "absent"},
    {"name": "plist_missing", "plist": "missing"},
    {"name": "plist_garbage", "plist": "garbage"},
    {"name": "codesign_missing", "codesign": "oserror"},
]
def run_bundle(c: dict) -> dict:
    d = fresh_dir("bundle")
    new, running = d / "x" / "Clawd.app", d / "Applications" / "Clawd.app"
    (new / "Contents").mkdir(parents=True)
    running.mkdir(parents=True)
    kind = c.get("plist", "ok")
    plist_path = new / "Contents" / "Info.plist"
    if kind == "ok":
        if c.get("bundle_version", "0.2.0") is None:
            plist_path.write_bytes(plistlib.dumps({"CFBundleIdentifier": au.BUNDLE_ID}))
        else:
            plist_path.write_bytes(plist_bytes(c.get("bundle_id", au.BUNDLE_ID), c.get("bundle_version", "0.2.0")))
    elif kind == "garbage":
        plist_path.write_bytes(b"garbage")
    calls: list = []

    def run(cmd, **kw):
        calls.append(cmd)
        if c.get("codesign") == "oserror":
            raise FileNotFoundError(2, "No such file or directory: 'codesign'")
        if cmd[1] == "--verify":
            return proc(c.get("verify_rc", 0))
        team = c.get("running_team", "ABCDE12345") if Path(cmd[-1]) == running else c.get("new_team", "ABCDE12345")
        return proc(0, f"Identifier=vn.clawd.desktop\nTeamIdentifier={team or 'not set'}\n")
    res = error_of(lambda: au.check_bundle(new, "0.2.0", None if c.get("running") == "absent" else running, run))
    err = res.get("error")
    if err and err.startswith(("Info.plist:", "codesign:")):
        err = err.split(":")[0] + ": *"          # the OS text after it differs per platform
    shown = [[str(a).replace(str(new), "<new>").replace(str(running), "<running>") for a in cmd] for cmd in calls]
    return {"error": err, "calls": shown}


out("check_bundle", {"cases": BUNDLES, "version": "0.2.0"}, {"results": [run_bundle(c) for c in BUNDLES]})

# ---- can_apply() --------------------------------------------------------------------------------
apps = fresh_dir("apps") / "Applications"
apps.mkdir()
exe = str(apps / "Clawd.app" / "Contents" / "MacOS" / "Clawd")
APPLY_CASES = [
    {"frozen": False, "system": "mac", "executable": None},
    {"frozen": False, "system": "windows", "executable": None},
    {"frozen": True, "system": "windows", "executable": None},
    {"frozen": True, "system": "linux", "executable": None},
    {"frozen": True, "system": "freebsd", "executable": None},
    {"frozen": True, "system": "mac", "executable": "<apps>/Clawd.app/Contents/MacOS/Clawd"},
    {"frozen": True, "system": "mac", "executable": "<apps>/Other.app/Contents/MacOS/Clawd"},
]
apply_out = []
for c in APPLY_CASES:
    ex = c["executable"].replace("<apps>", str(apps)) if c["executable"] else None
    ok, why = au.can_apply(frozen=c["frozen"], system=c["system"], executable=ex)
    apply_out.append([ok, why])
out("can_apply", {"cases": APPLY_CASES}, {"results": apply_out})

# ---- the swap script ----------------------------------------------------------------------------
out("swap_script", {}, {"text": au.MAC_SWAP, "sha256": h.sha256_of(au.MAC_SWAP.encode()), "no_team": au.NO_TEAM})

# ---- Updater flows (Windows) ---------------------------------------------------------------------
class Clock:
    def __init__(self):
        self.t = 1000.0

    def __call__(self):
        return self.t


def flow_case(name, steps, **kw):
    c = case(name, **kw)
    c["steps"] = steps
    return c


CHECK, FORCE = {"check": False}, {"check": True}
FLOWS = [
    flow_case("ok", [{"state": True}, CHECK, {"state": True}]),
    flow_case("tampered", [CHECK], tamper=[EXE]),
    flow_case("already_current", [CHECK], current="0.2.0"),
    flow_case("rate_limit", [CHECK, CHECK, FORCE, {"advance": au.RECHECK + 1}, CHECK], current="0.2.0"),
    flow_case("cannot_apply_from_source", [CHECK], frozen=False),
    flow_case("unreachable", [CHECK], offline=True),
    flow_case("failed_marker", [CHECK, {"fail_marker": "0.2.0"}, {"state": True}, {"restart": True}, FORCE]),
    flow_case("rate_limit_counts_failures", [CHECK, CHECK, {"advance": au.RECHECK + 1}, CHECK], offline=True),
    flow_case("api_error", [CHECK], routes={API_LATEST: {"status": 500}}),
    flow_case("sums_lack_the_file", [CHECK], sums_override=f"{'0' * 64}  other.exe\n"),
    flow_case("newer_then_stale_stage", [CHECK, {"restart": True, "current": "0.2.0"}, {"state": True}]),
]
def run_flow(c: dict) -> dict:
    d = fresh_dir("flow")
    clock = Clock()
    site = Site(c)
    stage = d / "app-update"
    frozen = c.get("frozen", True)

    def make(current, source):
        return au.Updater(source, stage, current, system=c["system"], arch=c["arch"], frozen=frozen, now=clock)
    up = make(c["current"], site.source())
    states: list = []
    for step in c["steps"]:
        if "check" in step:
            n = len(site.calls)
            st = up.check(force=step["check"])
            states.append({"state": st, "api_asks": sum(1 for u in site.calls[n:] if u == API_LATEST)})
        elif "state" in step:
            states.append({"state": up.state()})
        elif "advance" in step:
            clock.t += step["advance"]
        elif "fail_marker" in step:
            (stage / "failed").write_text(step["fail_marker"])
        elif "restart" in step:
            cur = step.get("current", c["current"])
            up = make(cur, site.source())
            states.append({"restart_state": up.state()})
    return {"states": states}


out("updater_flows", {"cases": FLOWS}, {"results": [run_flow(c) for c in FLOWS]})

# ---- apply(): recorded, never executed ------------------------------------------------------------
popen_calls = []
au.subprocess.Popen = lambda cmd, **kw: popen_calls.append((cmd, kw))          # type: ignore[assignment]
busy = [False]
paths.ota_busy = lambda *a, **k: busy[0]                                        # type: ignore[assignment]


def win_updater(d, current="0.1.0"):
    files = exe_files("installer" * 100)
    c = {"files": files, "release": release_json(files)}
    return au.Updater(Site(c).source(), d / "app-update", current, system="windows", arch="x86_64", frozen=True, now=Clock())


APPLY = []
d = fresh_dir("apply")
up = win_updater(d)
up.check()
popen_calls.clear()
ok = up.apply(1234)
cmd, kw = popen_calls[0]
APPLY.append({"name": "windows_ok", "result": list(ok),
              "popen": {"exe": Path(cmd[0]).name, "args": cmd[1:], "flags": 0x208 if sys.platform == "win32" and kw["creationflags"] == 0x208 else None,
                        "detached_stdio": kw["stdin"] == au.subprocess.DEVNULL and kw["close_fds"] is True}})
popen_calls.clear()
busy[0] = True
APPLY.append({"name": "windows_ota_busy", "result": list(up.apply()), "popen": len(popen_calls)})
busy[0] = False
staged = up.staged()
assert staged is not None
staged.path.write_bytes(b"swapped after the download")
APPLY.append({"name": "windows_changed_file", "result": list(up.apply()), "popen": len(popen_calls), "staged_after": up.staged() is not None})
APPLY.append({"name": "windows_nothing_staged",
              "result": list(au.Updater(None, d / "empty", "0.1.0", system="windows", frozen=True).apply()), "popen": len(popen_calls)})
d2 = fresh_dir("apply-src")
up2 = au.Updater(None, d2 / "s", "0.1.0", system="windows", arch="x86_64", frozen=False)
APPLY.append({"name": "windows_from_source", "result": list(up2.apply()), "popen": len(popen_calls)})

# macOS: scripted ditto/codesign, as tests/test_appupdate.py::mac_setup
d3 = fresh_dir("apply-mac")
files = {ZIP: "zip-of-the-app"}
macsite = Site({"files": files, "release": release_json(files)})
running = (d3 / "Applications" / "Clawd.app").resolve() if (d3 / "Applications" / "Clawd.app").exists() else None
(d3 / "Applications" / "Clawd.app" / "Contents" / "MacOS").mkdir(parents=True)
running = (d3 / "Applications" / "Clawd.app").resolve()
mac_calls = []


def mac_run(cmd, **kw):
    mac_calls.append(cmd)
    if cmd[0] == "ditto":
        dest = Path(cmd[4]) / "Clawd.app" / "Contents"
        (dest / "MacOS").mkdir(parents=True)
        (dest / "Info.plist").write_bytes(plist_bytes(au.BUNDLE_ID, "0.2.0"))
        (dest / "MacOS" / "Clawd").write_text("#!/bin/sh\n")
        return proc(0)
    if cmd[1] == "--verify":
        return proc(0)
    return proc(0, "Identifier=vn.clawd.desktop\nTeamIdentifier=ABCDE12345\n")


mstage = d3 / "stage"
mup = au.Updater(macsite.source(), mstage, "0.1.0", run=mac_run, system="mac", arch="arm64", frozen=True,
                 executable=str(running / "Contents" / "MacOS" / "Clawd"), now=Clock())
mstate = mup.check()
(mstage / "failed").write_text("0.1.9")
popen_calls.clear()
res = mup.apply(4321)
mcmd, mkw = popen_calls[0]


def shorten(text: str) -> str:
    text = text.replace(str(mstage), "<stage>").replace(str(running), "<running>").replace(str(d3), "<tmp>")
    return text.replace("\\", "/")           # the same fixture on every OS


APPLY.append({"name": "mac_ok", "state_status": mstate["status"], "result": list(res),
              "popen": {"argv": [shorten(a) for a in mcmd], "start_new_session": mkw.get("start_new_session"),
                        "script_is_swap": (mstage / "swap.sh").read_text() == au.MAC_SWAP,
                        "failed_marker_removed": not (mstage / "failed").exists()},
              "runner_calls": [[shorten(str(a)) for a in c] for c in mac_calls]})
out("apply", {"scenarios": [a["name"] for a in APPLY]}, {"results": APPLY})

# ---- from_settings() / auto_enabled() -------------------------------------------------------------
SETTINGS = [
    {"name": "defaults", "env": {}, "config": None},
    {"name": "env_repo", "env": {"CLAWD_UPDATE_REPO": "fork/usage-monitor"}, "config": None},
    {"name": "config_repo", "env": {}, "config": "update_repo = someone/else  # mine\n"},
    {"name": "env_beats_config", "env": {"CLAWD_UPDATE_REPO": "env/repo"}, "config": "update_repo = cfg/repo\n"},
    {"name": "config_key_is_case_insensitive", "env": {}, "config": "UPDATE_REPO = cfg/Repo\n"},
    {"name": "empty_env_is_unset", "env": {"CLAWD_UPDATE_REPO": ""}, "config": "update_repo = cfg/repo\n"},
    {"name": "empty_config_value", "env": {}, "config": "update_repo =\n"},
    {"name": "api_override_https", "env": {"CLAWD_UPDATE_API": "https://example.test/api/"}, "config": None},
    {"name": "api_override_loopback", "env": {"CLAWD_UPDATE_API": "http://127.0.0.1:9999"}, "config": None},
    {"name": "bad_repo", "env": {"CLAWD_UPDATE_REPO": "not a repo"}, "config": None},
    {"name": "bad_repo_quote", "env": {"CLAWD_UPDATE_REPO": "it's/x y"}, "config": None},
    {"name": "bad_repo_in_config", "env": {}, "config": "update_repo = a/b/c\n"},
]
AUTO = ["", "auto_update = off\n", "auto_update = OFF\n", "auto_update = false\n", "auto_update = 0\n", "auto_update = no\n",
        "auto_update = on\n", "auto_update = yes\n", "auto_update =\n", "auto_update = off # comment\n", "other = off\n"]

settings_out = []
for c in SETTINGS:
    for k in [k for k in os.environ if k.startswith("CLAWD_")]:
        del os.environ[k]
    os.environ.update(c["env"])
    paths.CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    usage.CONFIG_FILE.unlink(missing_ok=True)
    if c["config"] is not None:
        usage.CONFIG_FILE.write_text(c["config"])
    try:
        src = au.from_settings(httpx.MockTransport(lambda r: httpx.Response(404)))   # no env-proxy/registry lookup
        settings_out.append({"repo": src.repo, "api": src.api})
    except au.UpdateError as e:
        settings_out.append({"error": str(e)})
for k in [k for k in os.environ if k.startswith("CLAWD_")]:
    del os.environ[k]
auto_out = []
for text in AUTO:
    usage.CONFIG_FILE.unlink(missing_ok=True)
    if text:
        usage.CONFIG_FILE.write_text(text)
    auto_out.append(au.auto_enabled())
usage.CONFIG_FILE.unlink(missing_ok=True)
out("settings", {"cases": SETTINGS, "auto_configs": AUTO}, {"sources": settings_out, "auto": auto_out})
