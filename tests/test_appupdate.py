"""Updates of the app itself: the release check against GitHub (a fake API), the download and
its checks, the staged update, the macOS bundle checks, and the swap script run for real."""
import hashlib
import io
import os
import plistlib
import re
import subprocess
import sys
import zipfile
from pathlib import Path
from types import SimpleNamespace

import httpx
import pytest

from clawd_desktop import __version__, appupdate as au, paths

ZIP_NAME = "Clawd-0.2.0-mac-arm64.zip"
EXE_NAME = "Clawd-0.2.0-windows-setup.exe"
BASE = "https://github.com/o/r/releases/download/v0.2.0/"
CDN = "https://release-assets.githubusercontent.com/o/"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def app_zip(version="0.2.0", bundle_id=au.BUNDLE_ID) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("Clawd.app/Contents/Info.plist", plistlib.dumps(
            {"CFBundleIdentifier": bundle_id, "CFBundleShortVersionString": version}))
        z.writestr("Clawd.app/Contents/MacOS/Clawd", "#!/bin/sh\n")
    return buf.getvalue()


class GitHub:
    """api.github.com + the asset CDN in memory (httpx.MockTransport)."""

    def __init__(self, files: dict[str, bytes], version="0.2.0", **release):
        self.files, self.calls = files, []
        self.tamper: set[str] = set()
        self.sums_override: str | None = None
        base = f"https://github.com/o/r/releases/download/v{version}/"
        self.release = {"tag_name": f"v{version}", "draft": False, "prerelease": False, "body": "Sửa lỗi xoay màn hình",
                        "html_url": f"https://github.com/o/r/releases/tag/v{version}",
                        "assets": [{"name": n, "size": len(d), "browser_download_url": base + n} for n, d in files.items()
                                   ] + [{"name": au.SUMS_NAME, "size": 100, "browser_download_url": base + au.SUMS_NAME}],
                        **release}
        self.base = base

    def sums(self) -> str:
        return self.sums_override if self.sums_override is not None else "".join(
            f"{sha(d)}  {n}\n" for n, d in self.files.items())

    def handler(self, request: httpx.Request) -> httpx.Response:
        url = str(request.url)
        self.calls.append(url)
        if url == "https://api.github.com/repos/o/r/releases/latest":
            return httpx.Response(200, json=self.release)
        if url == self.base + au.SUMS_NAME:
            return httpx.Response(302, headers={"location": CDN + "sums"})
        if url == CDN + "sums":
            return httpx.Response(200, text=self.sums())
        for name, data in self.files.items():
            if url == self.base + name:
                return httpx.Response(302, headers={"location": CDN + name})
            if url == CDN + name:
                return httpx.Response(200, content=data[:-1] + b"X" if name in self.tamper else data)
        return httpx.Response(404)

    def source(self) -> au.Source:
        return au.Source("o/r", "https://api.github.com", transport=httpx.MockTransport(self.handler))


# ---- pure helpers -----------------------------------------------------------------
def test_versions():
    assert au.parse_version("v1.2.3") == (1, 2, 3) and au.parse_version("0.10.0") == (0, 10, 0)
    for bad in ("1.2", "v1.2.3-rc1", "1.2.3.4", "latest", "", None, 5):
        assert au.parse_version(bad) is None
    assert au.is_newer("v0.10.0", "0.9.9") and not au.is_newer("0.1.0", "0.1.0") and not au.is_newer("0.1.0", "0.2.0")
    assert not au.is_newer("v1.0.0-rc1", "0.1.0")            # a prerelease tag is never newer


def test_asset_names_sums_and_team_id():
    assert au.asset_name("1.2.3", "mac", "arm64") == "Clawd-1.2.3-mac-arm64.zip"
    assert au.asset_name("1.2.3", "mac", "x86_64") == "Clawd-1.2.3-mac-x86_64.zip"
    assert au.asset_name("1.2.3", "windows", "x86_64") == "Clawd-1.2.3-windows-setup.exe"
    assert au.asset_name("1.2.3", "linux", "x86_64") is None and au.asset_name("1.2.3", "mac", "ppc") is None
    h = "a" * 64
    assert au.parse_sums(f"{h}  one.zip\n{h.upper()} *two.exe\nbad  three\n") == {"one.zip": h, "two.exe": h}
    assert au.team_id("Identifier=x\nTeamIdentifier=ABCDE12345\n") == "ABCDE12345"
    assert au.team_id("TeamIdentifier=not set\n") is None and au.team_id("") is None


def test_the_build_names_match_the_release_workflow():
    """packaging/ and the release workflow name the files; the updater looks for exactly those."""
    root = Path(__file__).resolve().parent.parent
    mac = (root / "packaging" / "build_mac.sh").read_text()
    assert 'Clawd-$VERSION-mac-$ARCH.zip' in mac
    assert "OutputBaseFilename=Clawd-{#AppVersion}-windows-setup" in (root / "packaging" / "windows" / "clawd.iss").read_text()
    assert au.asset_name("1.2.3", "windows", "x86_64") == "Clawd-1.2.3-windows-setup.exe"


def test_versions_agree():
    root = Path(__file__).resolve().parent.parent
    m = re.search(r'^version = "([^"]+)"', (root / "pyproject.toml").read_text(), re.M)
    assert m and m[1] == __version__


# ---- the release source -----------------------------------------------------------------
def test_latest_finds_the_asset_and_its_checksum():
    gh = GitHub({EXE_NAME: b"installer"})
    rel = gh.source().latest("0.1.0", "windows", "x86_64")
    assert rel == au.AppRelease("0.2.0", "v0.2.0", "Sửa lỗi xoay màn hình", gh.release["html_url"], EXE_NAME,
                                BASE + EXE_NAME, 9, sha(b"installer"))


@pytest.mark.parametrize("change", [{"draft": True}, {"prerelease": True}, {"tag_name": "nightly"},
                                    {"tag_name": "v0.2.0-rc1"}])
def test_a_draft_a_prerelease_or_a_odd_tag_is_not_offered(change):
    assert GitHub({EXE_NAME: b"x"}, **change).source().latest("0.1.0", "windows", "x86_64") is None


def test_nothing_newer_nothing_for_this_platform_or_no_checksum_yet():
    gh = GitHub({EXE_NAME: b"x"})
    assert gh.source().latest("0.2.0", "windows", "x86_64") is None        # already current
    assert gh.source().latest("0.3.0", "windows", "x86_64") is None        # running a newer build
    assert gh.source().latest("0.1.0", "mac", "arm64") is None             # no mac file in this release
    assert gh.source().latest("0.1.0", "linux", "x86_64") is None
    gh.release["assets"] = [a for a in gh.release["assets"] if a["name"] != au.SUMS_NAME]
    assert gh.source().latest("0.1.0", "windows", "x86_64") is None        # CI is still uploading


def test_bad_checksum_file_untrusted_url_and_rate_limit_are_errors():
    gh = GitHub({EXE_NAME: b"x"})
    gh.sums_override = f"{'0' * 64}  something-else.exe\n"
    with pytest.raises(au.UpdateError, match="SHA256SUMS"):
        gh.source().latest("0.1.0", "windows", "x86_64")
    gh = GitHub({EXE_NAME: b"x"})
    gh.release["assets"][0]["browser_download_url"] = "https://evil.example/Clawd.exe"
    with pytest.raises(au.UpdateError):
        gh.source().latest("0.1.0", "windows", "x86_64")
    limited = au.Source("o/r", transport=httpx.MockTransport(lambda r: httpx.Response(403)))
    with pytest.raises(au.UpdateError, match="giới hạn"):
        limited.latest("0.1.0", "windows", "x86_64")
    with pytest.raises(au.UpdateError):
        au.Source("not a repo")


def test_download_checks_size_and_hash_and_only_trusted_hosts(tmp_path):
    payload = b"installer-bytes" * 1000
    gh = GitHub({EXE_NAME: payload})
    src = gh.source()
    rel = src.latest("0.1.0", "windows", "x86_64")
    seen = []
    dest = tmp_path / EXE_NAME
    src.download(rel, dest, lambda got, total: seen.append((got, total)))
    assert dest.read_bytes() == payload and seen[-1] == (len(payload), len(payload))
    assert [p.name for p in tmp_path.iterdir()] == [EXE_NAME]                # no .part left over

    gh.tamper.add(EXE_NAME)                                                  # same size, one byte flipped
    bad = tmp_path / "bad.exe"
    with pytest.raises(au.UpdateError, match="SHA-256"):
        src.download(rel, bad)
    assert not bad.exists() and [p.name for p in tmp_path.iterdir()] == [EXE_NAME]

    short = au.AppRelease(**{**rel.__dict__, "size": 10})                    # more bytes than announced
    with pytest.raises(au.UpdateError, match="lớn hơn"):
        src.download(short, tmp_path / "x.exe")
    evil = au.AppRelease(**{**rel.__dict__, "url": "https://evil.example/x"})
    with pytest.raises(au.UpdateError, match="đáng tin"):
        src.download(evil, tmp_path / "y.exe")


def test_a_redirect_to_a_foreign_host_is_refused(tmp_path):
    gh = GitHub({EXE_NAME: b"abc"})
    src = gh.source()
    rel = src.latest("0.1.0", "windows", "x86_64")
    orig = gh.handler

    def hijack(request):
        if str(request.url) == BASE + EXE_NAME:
            return httpx.Response(302, headers={"location": "https://evil.example/Clawd.exe"})
        return orig(request)
    src._http = httpx.Client(transport=httpx.MockTransport(hijack), follow_redirects=False)
    with pytest.raises(au.UpdateError, match="đáng tin"):
        src.download(rel, tmp_path / "z.exe")


# ---- can this install replace itself --------------------------------------------------------
def test_can_apply(tmp_path):
    assert not au.can_apply(frozen=False, system="mac")[0]                         # from source
    assert au.can_apply(frozen=True, system="windows") == (True, "")
    assert not au.can_apply(frozen=True, system="linux")[0]
    apps = tmp_path / "Applications"
    apps.mkdir()
    exe = str(apps / "Clawd.app" / "Contents" / "MacOS" / "Clawd")
    assert au.can_apply(frozen=True, system="mac", executable=exe) == (True, "")
    assert "Applications" in au.can_apply(frozen=True, system="mac", executable="/Volumes/Clawd/Clawd.app/Contents/MacOS/Clawd")[1]
    tr = "/private/var/folders/x/AppTranslocation/ABC/d/Clawd.app/Contents/MacOS/Clawd"
    assert not au.can_apply(frozen=True, system="mac", executable=tr)[0]
    assert "Clawd.app" in au.can_apply(frozen=True, system="mac", executable="/usr/bin/python3")[1]
    if os.name == "posix" and os.geteuid() != 0:
        apps.chmod(0o500)
        try:
            assert not au.can_apply(frozen=True, system="mac", executable=exe)[0]
        finally:
            apps.chmod(0o700)


# ---- the updater: Windows -----------------------------------------------------------------
class Clock:
    def __init__(self):
        self.t = 1000.0

    def __call__(self):
        return self.t


def windows_updater(tmp_path, gh, current="0.1.0", clock=None):
    return au.Updater(gh.source(), tmp_path / "app-update", current, system="windows", arch="x86_64",
                      frozen=True, now=clock or Clock())


def test_check_downloads_verifies_and_stages(tmp_path):
    gh = GitHub({EXE_NAME: b"installer" * 100})
    up = windows_updater(tmp_path, gh)
    assert up.state()["status"] == "idle"
    st = up.check()
    assert st["status"] == "ready" and st["version"] == "0.2.0" and st["canApply"] and st["current"] == "0.1.0"
    assert "xoay" in st["notes"] and st["page"].endswith("v0.2.0")
    s = up.staged()
    assert s and s.path.name == EXE_NAME and au.sha256_file(s.path) == s.sha256
    # a second process (the tray, the settings window) sees the same without asking GitHub
    other = au.Updater(None, tmp_path / "app-update", "0.1.0", system="windows", frozen=True)
    assert other.state()["status"] == "ready" and other.state()["version"] == "0.2.0"


def test_a_failed_download_keeps_the_app_as_it_is(tmp_path):
    gh = GitHub({EXE_NAME: b"installer" * 100})
    gh.tamper.add(EXE_NAME)
    up = windows_updater(tmp_path, gh)
    st = up.check()
    assert st["status"] == "error" and "SHA-256" in st["error"] and up.staged() is None
    assert not list((tmp_path / "app-update").glob("*.exe*"))


def test_current_when_there_is_nothing_newer(tmp_path):
    up = windows_updater(tmp_path, GitHub({EXE_NAME: b"x"}), current="0.2.0")
    assert up.check()["status"] == "current"


def test_the_check_is_rate_limited_but_force_asks_again(tmp_path):
    gh = GitHub({EXE_NAME: b"x"})
    clock = Clock()
    up = windows_updater(tmp_path, gh, current="0.2.0", clock=clock)
    up.check()
    n = len(gh.calls)
    up.check()                                       # the window opening: too soon
    assert len(gh.calls) == n
    up.check(force=True)                             # the button
    assert len(gh.calls) > n
    n = len(gh.calls)
    clock.t += au.RECHECK + 1
    up.check()
    assert len(gh.calls) > n


def test_an_install_that_cannot_replace_itself_only_announces(tmp_path):
    gh = GitHub({EXE_NAME: b"x"})
    up = au.Updater(gh.source(), tmp_path / "s", "0.1.0", system="windows", arch="x86_64", frozen=False)
    st = up.check()
    assert st["status"] == "available" and st["version"] == "0.2.0" and not st["canApply"] and "mã nguồn" in st["why"]
    assert not (tmp_path / "s").exists() and sum(c.endswith(EXE_NAME) for c in gh.calls) == 0   # nothing downloaded
    assert up.apply() == (False, st["why"])


def test_github_unreachable_is_an_error_not_a_crash(tmp_path):
    def down(_r):
        raise httpx.ConnectError("no network")
    up = au.Updater(au.Source("o/r", transport=httpx.MockTransport(down)), tmp_path / "s", "0.1.0",
                    system="windows", arch="x86_64", frozen=True)
    st = up.check()
    assert st["status"] == "error" and "GitHub" in st["error"]


def test_a_staged_update_that_is_now_current_is_cleaned_up(tmp_path):
    gh = GitHub({EXE_NAME: b"x"})
    up = windows_updater(tmp_path, gh)
    up.check()
    assert up.staged()
    after = au.Updater(None, tmp_path / "app-update", "0.2.0", system="windows", frozen=True)   # the new version runs
    assert after.staged() is None and not (tmp_path / "app-update").exists()


def test_apply_on_windows_runs_the_installer_silently(tmp_path, monkeypatch):
    gh = GitHub({EXE_NAME: b"installer" * 100})
    up = windows_updater(tmp_path, gh)
    up.check()
    ran = []
    monkeypatch.setattr(au.subprocess, "Popen", lambda cmd, **kw: ran.append(cmd))
    monkeypatch.setattr(paths, "ota_busy", lambda *a, **k: False)
    assert up.apply(1234) == (True, "")
    assert ran[0][0].endswith(EXE_NAME) and "/SILENT" in ran[0] and "/NORESTART" in ran[0]


def test_apply_refuses_while_firmware_is_installing_and_a_changed_file(tmp_path, monkeypatch):
    gh = GitHub({EXE_NAME: b"installer" * 100})
    up = windows_updater(tmp_path, gh)
    up.check()
    ran = []
    monkeypatch.setattr(au.subprocess, "Popen", lambda cmd, **kw: ran.append(cmd))
    monkeypatch.setattr(paths, "ota_busy", lambda *a, **k: True)
    ok, why = up.apply()
    assert not ok and "firmware" in why and not ran
    monkeypatch.setattr(paths, "ota_busy", lambda *a, **k: False)
    up.staged().path.write_bytes(b"swapped after the download")
    ok, why = up.apply()
    assert not ok and "bị đổi" in why and not ran and up.staged() is None
    assert au.Updater(None, tmp_path / "empty", "0.1.0", system="windows", frozen=True).apply()[0] is False


def test_the_apply_lock_keeps_two_processes_from_downloading(tmp_path):
    gh = GitHub({EXE_NAME: b"installer"})
    clock = Clock()
    a = windows_updater(tmp_path, gh, clock=clock)
    b = windows_updater(tmp_path, gh, clock=clock)
    assert a._acquire()
    # b sees a live lock held by another pid: it does not call GitHub and reports a download in progress
    other = (tmp_path / "app-update.lock")
    other.write_text("1")                      # not our pid
    st = b.check()
    assert st["status"] == "downloading" and not gh.calls
    clock.t += au.LOCK_STALE + 1               # a dead downloader's lock expires
    os.utime(other, (clock.t - au.LOCK_STALE - 5, clock.t - au.LOCK_STALE - 5))
    assert b.check()["status"] == "ready"


def test_a_failed_swap_is_not_offered_again(tmp_path):
    gh = GitHub({EXE_NAME: b"installer"})
    up = windows_updater(tmp_path, gh)
    up.check()
    (tmp_path / "app-update" / "failed").write_text("0.2.0")
    st = up.state()
    assert st["status"] == "error" and "trang phát hành" in st["error"] and not st["canApply"]
    gh.calls.clear()
    st = windows_updater(tmp_path, gh).check(force=True)
    assert st["status"] == "error" and not any(c.endswith(EXE_NAME) or c.endswith(EXE_NAME.replace("-0.2.0", "")) for c in gh.calls[1:])


# ---- the updater: macOS (codesign and ditto replaced) -----------------------------------------
def mac_setup(tmp_path, *, version="0.2.0", bundle_id=au.BUNDLE_ID, new_team="ABCDE12345", running_team="ABCDE12345",
              verify_rc=0):
    data = app_zip(version, bundle_id)
    gh = GitHub({ZIP_NAME: data})
    apps = tmp_path / "Applications"
    (apps / "Clawd.app" / "Contents" / "MacOS").mkdir(parents=True)
    running = (apps / "Clawd.app").resolve()
    calls = []

    def run(cmd, **kw):
        calls.append(cmd[:2])
        if cmd[0] == "ditto":
            zipfile.ZipFile(cmd[3]).extractall(cmd[4])
            return SimpleNamespace(returncode=0, stdout="", stderr="")
        if cmd[1] == "--verify":
            return SimpleNamespace(returncode=verify_rc, stdout="", stderr="")
        team = running_team if Path(cmd[-1]).resolve() == running else new_team
        return SimpleNamespace(returncode=0, stdout="", stderr=f"Identifier=vn.clawd.desktop\nTeamIdentifier={team or 'not set'}\n")
    up = au.Updater(gh.source(), tmp_path / "stage", "0.1.0", run=run, system="mac", arch="arm64", frozen=True,
                    executable=str(running / "Contents" / "MacOS" / "Clawd"))
    return up, gh, running, calls


def test_mac_download_unpack_and_checks(tmp_path):
    up, gh, running, calls = mac_setup(tmp_path)
    st = up.check()
    assert st["status"] == "ready", st
    s = up.staged()
    assert s.path == tmp_path / "stage" / "Clawd.app" and (s.path / "Contents" / "Info.plist").is_file()
    assert not (tmp_path / "stage" / ZIP_NAME).exists() and not (tmp_path / "stage" / "unpack").exists()
    assert ["codesign", "--verify"] in calls


@pytest.mark.parametrize("kw, why", [
    ({"new_team": "OTHERTEAM01"}, "Team ID"),
    ({"new_team": None}, "Team ID"),                            # ad-hoc build replacing a Developer ID one
    ({"verify_rc": 1}, "chữ ký"),
    ({"version": "0.1.9"}, "0.1.9"),                            # the zip holds another version than announced
    ({"bundle_id": "com.evil.app"}, "không phải app Clawd"),
])
def test_mac_refuses_a_bad_bundle(tmp_path, kw, why):
    up, _gh, _r, _c = mac_setup(tmp_path, **kw)
    st = up.check()
    assert st["status"] == "error" and why in st["error"] and up.staged() is None


def test_mac_an_ad_hoc_running_app_has_no_team_to_match(tmp_path):
    up, *_ = mac_setup(tmp_path, running_team=None, new_team="ABCDE12345")
    assert up.check()["status"] == "ready"


def test_mac_apply_starts_the_swap_script(tmp_path, monkeypatch):
    up, _gh, running, _c = mac_setup(tmp_path)
    up.check()
    started = []
    monkeypatch.setattr(au.subprocess, "Popen", lambda cmd, **kw: started.append((cmd, kw)))
    monkeypatch.setattr(paths, "ota_busy", lambda *a, **k: False)
    assert up.apply(4321) == (True, "")
    cmd, kw = started[0]
    assert cmd[:3] == ["/bin/sh", str(tmp_path / "stage" / "swap.sh"), "4321"]
    assert cmd[3] == str(tmp_path / "stage" / "Clawd.app") and cmd[4] == str(running) and cmd[6] == "0.2.0"
    assert kw["start_new_session"] and (tmp_path / "stage" / "swap.sh").read_text() == au.MAC_SWAP


# ---- the swap script, for real ----------------------------------------------------------------
posix = pytest.mark.skipif(sys.platform == "win32", reason="sh script")


def swap_env(tmp_path, ditto_ok=True):
    """A fake machine: stub `ditto` (cp -R) and `open` (records), an old app and a staged new one."""
    bin_ = tmp_path / "bin"
    bin_.mkdir()
    (bin_ / "ditto").write_text('#!/bin/sh\n' + ('cp -R "$1" "$2"\n' if ditto_ok else 'exit 1\n'))
    (bin_ / "open").write_text(f'#!/bin/sh\necho "$@" >> "{tmp_path}/opened"\n')
    for f in bin_.iterdir():
        f.chmod(0o755)
    apps, stage = tmp_path / "Applications", tmp_path / "stage"
    (apps / "Clawd.app").mkdir(parents=True)
    (apps / "Clawd.app" / "version").write_text("old")
    (stage / "Clawd.app").mkdir(parents=True)
    (stage / "Clawd.app" / "version").write_text("new")
    (stage / "ready.json").write_text("{}")
    (stage / "swap.sh").write_text(au.MAC_SWAP)
    return apps / "Clawd.app", stage, {**os.environ, "PATH": f"{bin_}:{os.environ['PATH']}"}


def run_swap(stage, app, env, pid):
    return subprocess.run(["/bin/sh", str(stage / "swap.sh"), str(pid), str(stage / "Clawd.app"), str(app), str(stage), "0.2.0"],
                          env=env, capture_output=True, text=True, timeout=60)


def dead_pid() -> int:
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


@posix
def test_swap_script_replaces_the_app_and_opens_it(tmp_path):
    app, stage, env = swap_env(tmp_path)
    r = run_swap(stage, app, env, dead_pid())
    assert r.returncode == 0, r.stderr
    assert (app / "version").read_text() == "new"
    assert [p.name for p in app.parent.iterdir()] == ["Clawd.app"]               # no .old / .update left behind
    assert (tmp_path / "opened").read_text().strip() == str(app)
    assert not (stage / "ready.json").exists() and not (stage / "Clawd.app").exists()
    assert "updated to 0.2.0" in (stage / "apply.log").read_text()
    assert au.Updater(None, stage, "0.2.0", system="mac", frozen=True).staged() is None     # next start: tidied
    assert not stage.exists()


@posix
def test_swap_script_waits_for_the_app_to_quit(tmp_path):
    app, stage, env = swap_env(tmp_path)
    import time
    sleeper = subprocess.Popen(["sleep", "1.5"])
    t0 = time.time()
    script = subprocess.Popen(["/bin/sh", str(stage / "swap.sh"), str(sleeper.pid), str(stage / "Clawd.app"),
                               str(app), str(stage), "0.2.0"], env=env)
    time.sleep(0.5)
    assert (app / "version").read_text() == "old"                  # still waiting: the app has not quit
    sleeper.wait()                                                 # reaped, like launchd does for the real app
    assert script.wait(timeout=30) == 0 and time.time() - t0 >= 1.0 and (app / "version").read_text() == "new"


@posix
def test_swap_script_leaves_the_old_app_when_the_copy_fails(tmp_path):
    app, stage, env = swap_env(tmp_path, ditto_ok=False)
    r = run_swap(stage, app, env, dead_pid())
    assert r.returncode == 1
    assert (app / "version").read_text() == "old" and [p.name for p in app.parent.iterdir()] == ["Clawd.app"]
    assert (stage / "failed").read_text().strip() == "0.2.0" and (stage / "ready.json").exists()
    assert (tmp_path / "opened").read_text().strip() == str(app)                  # the old app starts again


# ---- the firmware-install marker -----------------------------------------------------------------
def test_ota_busy_marker(tmp_path):
    f = tmp_path / "ota.busy"
    assert not paths.ota_busy(f)
    paths.set_ota_busy(True, f)
    assert paths.ota_busy(f)
    assert not paths.ota_busy(f, now=f.stat().st_mtime + paths.OTA_BUSY_STALE + 1)      # a crashed install
    paths.set_ota_busy(False, f)
    assert not paths.ota_busy(f)
    paths.set_ota_busy(False, f)                                                          # idempotent


def test_config_can_turn_the_background_check_off(monkeypatch):
    monkeypatch.setattr(au.usage, "read_config_value", lambda k: "off" if k == "auto_update" else None)
    assert not au.auto_enabled()
    monkeypatch.setattr(au.usage, "read_config_value", lambda k: None)
    assert au.auto_enabled()
