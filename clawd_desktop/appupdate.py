"""Updates of the app itself: Clawd.app / Clawd.exe, from the project's GitHub Releases.

(The clock's firmware has its own feed, feed.py. Nothing here touches that feed or its token.)

How it works
  1. Ask  GET /repos/<owner>/<name>/releases/latest  (public API, no token). A draft or a
     prerelease is never offered. The tag `vX.Y.Z` is the version.
  2. A newer release carries, per platform, the installer and a `SHA256SUMS` file:
       Clawd-X.Y.Z-mac-arm64.zip | Clawd-X.Y.Z-mac-x86_64.zip   (the notarized Clawd.app)
       Clawd-X.Y.Z-windows-setup.exe                             (the Inno Setup installer)
  3. The file is downloaded in the background and its SHA-256 checked against SHA256SUMS.
     On macOS it is unpacked and its code signature, bundle id and version checked: the new
     app must carry the same Team ID as the running one.
  4. Only the user's "Khởi động lại để cập nhật" applies it. macOS: a detached shell script
     waits for this process to quit, swaps the bundle (keeping the old one until the new one
     is in place) and opens it. Windows: the installer runs silently over the old install
     (same AppId, per-user, no UAC) and starts the app again.

Everything it keeps lives in `<config dir>/app-update/`. A failed swap leaves a `failed`
marker, so the same version is not offered in a loop: the page then points to the releases.
"""
from __future__ import annotations

import contextlib
import hashlib
import json
import os
import platform
import plistlib
import re
import shutil
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable
from urllib.parse import urlparse

import httpx

from clawd_desktop import __version__, paths, usage

REPO = "vuduong97/usage-monitor"
API = "https://api.github.com"
BUNDLE_ID = "vn.clawd.desktop"
CHECK_EVERY = 6 * 3600.0                 # the tray's background check
FIRST_CHECK = 90.0                       # first check after the app starts (s)
RETRY = 1800.0                           # after a failed check (s)
RECHECK = 600.0                          # the settings window asks again after this long (s)
LOCK_STALE = 120.0                       # a download lock not touched for this long is dead (s)
TIMEOUT = 30.0
DOWNLOAD_TIMEOUT = 300.0
MAX_DOWNLOAD = 500 * 1024 * 1024
MAX_JSON = 2 * 1024 * 1024
SUMS_NAME = "SHA256SUMS"
STAGE_DIR = paths.CONFIG_DIR / "app-update"
GITHUB_HOSTS = ("github.com", "api.github.com")
GITHUB_CDN = ".githubusercontent.com"     # release assets redirect here
VER_RE = re.compile(r"v?(\d{1,4})\.(\d{1,4})\.(\d{1,4})")
HEX64 = re.compile(r"[0-9a-f]{64}")
REPO_RE = re.compile(r"[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}")
TEAM_RE = re.compile(r"^TeamIdentifier=(\S+)$", re.M)


class UpdateError(Exception):
    pass


NO_TEAM = ("Bản này chỉ có chữ ký ad-hoc (không có Team ID) nên không tự cập nhật được. "
           "Hãy cài bản chính thức một lần, từ đó app sẽ tự cập nhật.")


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] [app-update] {msg}", flush=True)


# =============================================================================
# Pure helpers (tested)
# =============================================================================
def parse_version(s: object) -> tuple[int, int, int] | None:
    """`v1.2.3` / `1.2.3` -> (1, 2, 3). Anything else (1.2, 1.2.3-rc1, ...) is not a release."""
    m = VER_RE.fullmatch(s.strip()) if isinstance(s, str) else None
    return (int(m[1]), int(m[2]), int(m[3])) if m else None


def is_newer(candidate: str, current: str) -> bool:
    a, b = parse_version(candidate), parse_version(current)
    return a is not None and b is not None and a > b


def this_platform() -> tuple[str, str]:
    """("mac" | "windows" | "linux", "arm64" | "x86_64" | ...)."""
    system = {"darwin": "mac", "win32": "windows"}.get(sys.platform, "linux")
    arch = platform.machine().lower()
    return system, {"amd64": "x86_64", "x64": "x86_64", "aarch64": "arm64"}.get(arch, arch)


def asset_name(version: str, system: str, arch: str) -> str | None:
    """The release file for this platform; None where there is no self-update."""
    if system == "mac" and arch in ("arm64", "x86_64"):
        return f"Clawd-{version}-mac-{arch}.zip"
    if system == "windows":
        return f"Clawd-{version}-windows-setup.exe"
    return None


def parse_sums(text: str) -> dict[str, str]:
    """`sha256sum` output: "<64 hex>  <name>" (a `*` before the name marks binary mode)."""
    out: dict[str, str] = {}
    for line in text.splitlines():
        parts = line.strip().split(None, 1)
        if len(parts) == 2 and HEX64.fullmatch(parts[0].lower()):
            out[parts[1].lstrip("*").strip()] = parts[0].lower()
    return out


def team_id(codesign_output: str) -> str | None:
    """The Team ID in `codesign -dv --verbose=2` output; None for an ad-hoc signature."""
    m = TEAM_RE.search(codesign_output)
    return m[1] if m and m[1] != "not" else None


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


@dataclass(frozen=True)
class AppRelease:
    version: str          # "1.2.3"
    tag: str
    notes: str
    page: str             # the release's web page
    name: str             # the asset for this platform
    url: str
    size: int
    sha256: str


# =============================================================================
# The release source (GitHub)
# =============================================================================
class Source:
    def __init__(self, repo: str = REPO, api: str = API,
                 transport: httpx.BaseTransport | None = None) -> None:
        if not REPO_RE.fullmatch(repo):
            raise UpdateError(f"bad repository name: {repo!r}")
        self.repo, self.api = repo, api.rstrip("/")
        self._loopback = urlparse(self.api).hostname in ("127.0.0.1", "localhost")   # tests, dev
        self._http = httpx.Client(
            timeout=TIMEOUT, transport=transport, follow_redirects=False,
            headers={"Accept": "application/vnd.github+json", "User-Agent": f"Clawd/{__version__}"})

    def close(self) -> None:
        self._http.close()

    def trusted(self, url: str) -> bool:
        """Where a file may come from: GitHub and its asset CDN over https (a local test
        server only when the API itself is local)."""
        u = urlparse(url)
        host = u.hostname or ""
        if u.scheme == "https":
            return host in GITHUB_HOSTS or host.endswith(GITHUB_CDN)
        return self._loopback and u.scheme == "http" and host in ("127.0.0.1", "localhost")

    def _get_json(self, path: str) -> dict | None:
        try:
            r = self._http.get(self.api + path)
        except httpx.HTTPError as e:
            raise UpdateError(f"{type(e).__name__}: {e}") from e
        if r.status_code == 404:
            return None
        if r.status_code in (403, 429):
            raise UpdateError("GitHub đang giới hạn số lần hỏi, thử lại sau")
        if r.status_code >= 400:
            raise UpdateError(f"GitHub trả HTTP {r.status_code}")
        if len(r.content) > MAX_JSON:
            raise UpdateError("phản hồi quá lớn")
        try:
            data = r.json()
        except ValueError as e:
            raise UpdateError("phản hồi không phải JSON") from e
        return data if isinstance(data, dict) else None

    def _get_small(self, url: str) -> str:
        """A small text file (SHA256SUMS), following GitHub's redirect to its CDN."""
        for _ in range(4):
            if not self.trusted(url):
                raise UpdateError("địa chỉ tải không đáng tin")
            try:
                r = self._http.get(url)
            except httpx.HTTPError as e:
                raise UpdateError(f"{type(e).__name__}: {e}") from e
            if r.status_code in (301, 302, 303, 307, 308) and r.headers.get("location"):
                url = str(httpx.URL(url).join(r.headers["location"]))
                continue
            if r.status_code >= 400 or len(r.content) > 64 * 1024:
                raise UpdateError(f"không đọc được {SUMS_NAME} (HTTP {r.status_code})")
            return r.text
        raise UpdateError("quá nhiều lần chuyển hướng")

    def latest(self, current: str, system: str | None = None, arch: str | None = None) -> AppRelease | None:
        """The newest published release when it is newer than `current` and has this
        platform's installer and its checksum; None otherwise."""
        sys_, arch_ = (system, arch) if system else this_platform()
        data = self._get_json(f"/repos/{self.repo}/releases/latest")
        if not data or data.get("draft") or data.get("prerelease"):
            return None
        tag = str(data.get("tag_name") or "")
        ver = parse_version(tag)
        if ver is None or not is_newer(tag, current):
            return None
        version = ".".join(map(str, ver))
        name = asset_name(version, sys_, arch_ or "")
        if name is None:
            return None
        assets = {a.get("name"): a for a in data.get("assets") or [] if isinstance(a, dict)}
        a, sums = assets.get(name), assets.get(SUMS_NAME)
        if not a or not sums:
            return None                     # still uploading, or not built for this platform
        url, size = str(a.get("browser_download_url") or ""), a.get("size")
        if not self.trusted(url) or not isinstance(size, int) or isinstance(size, bool) \
                or not 0 < size <= MAX_DOWNLOAD:
            raise UpdateError(f"{name}: địa chỉ hoặc kích thước không hợp lệ")
        digest = parse_sums(self._get_small(str(sums.get("browser_download_url") or ""))).get(name)
        if not digest:
            raise UpdateError(f"{SUMS_NAME} không có {name}")
        return AppRelease(version=version, tag=tag, notes=str(data.get("body") or "").strip()[:2000],
                          page=str(data.get("html_url") or ""), name=name, url=url, size=size, sha256=digest)

    def download(self, rel: AppRelease, dest: Path, progress: Callable[[int, int], None] = lambda *_a: None) -> None:
        """Download `rel` to `dest`, checking size and SHA-256; `dest` appears only when it is right."""
        dest.parent.mkdir(parents=True, exist_ok=True)
        part = dest.with_name(f"{dest.name}.part{os.getpid()}")
        h, got, url = hashlib.sha256(), 0, rel.url
        try:
            for _ in range(4):
                if not self.trusted(url):
                    raise UpdateError("địa chỉ tải không đáng tin")
                with self._http.stream("GET", url, timeout=DOWNLOAD_TIMEOUT) as r:
                    if r.status_code in (301, 302, 303, 307, 308) and r.headers.get("location"):
                        url = str(httpx.URL(url).join(r.headers["location"]))
                        continue
                    if r.status_code != 200:
                        raise UpdateError(f"tải {rel.name}: HTTP {r.status_code}")
                    with open(part, "wb") as f:
                        for chunk in r.iter_bytes(1 << 18):
                            got += len(chunk)
                            if got > rel.size:
                                raise UpdateError(f"{rel.name}: lớn hơn {rel.size} byte đã công bố")
                            h.update(chunk)
                            f.write(chunk)
                            progress(got, rel.size)
                break
            else:
                raise UpdateError("quá nhiều lần chuyển hướng")
            if got != rel.size:
                raise UpdateError(f"{rel.name}: nhận {got} byte, đã công bố {rel.size}")
            if h.hexdigest() != rel.sha256:
                raise UpdateError(f"{rel.name}: SHA-256 không khớp, bỏ file")
            part.replace(dest)
        except httpx.HTTPError as e:
            raise UpdateError(f"{type(e).__name__}: {e}") from e
        finally:
            with contextlib.suppress(OSError):
                part.unlink()


def from_settings(transport: httpx.BaseTransport | None = None) -> Source:
    repo = os.environ.get("CLAWD_UPDATE_REPO") or usage.read_config_value("update_repo") or REPO
    return Source(repo, os.environ.get("CLAWD_UPDATE_API") or API, transport)


def auto_enabled() -> bool:
    """`auto_update = off` in the config file stops the background check (the button still works)."""
    return (usage.read_config_value("auto_update") or "").lower() not in ("off", "false", "0", "no")


# =============================================================================
# macOS bundle checks and the swap script
# =============================================================================
Runner = Callable[..., "subprocess.CompletedProcess[str]"]


def _codesign(args: list[str], run: Runner) -> subprocess.CompletedProcess:
    try:
        return run(["codesign", *args], capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.SubprocessError) as e:
        raise UpdateError(f"codesign: {e}") from e


def bundle_team(app: Path, run: Runner = subprocess.run) -> str | None:
    r = _codesign(["-dv", "--verbose=2", str(app)], run)
    return team_id((r.stderr or "") + (r.stdout or ""))


def check_bundle(new: Path, version: str, running: Path | None, run: Runner = subprocess.run) -> None:
    """The unpacked app is the version announced, validly signed and by the same team as
    the one running (when that one is signed by a team; an ad-hoc dev build has none to match)."""
    try:
        info = plistlib.loads((new / "Contents" / "Info.plist").read_bytes())
    except (OSError, ValueError, plistlib.InvalidFileException) as e:
        raise UpdateError(f"Info.plist: {e}") from e
    if info.get("CFBundleIdentifier") != BUNDLE_ID:
        raise UpdateError("không phải app Clawd")
    if info.get("CFBundleShortVersionString") != version:
        raise UpdateError(f"app bên trong là {info.get('CFBundleShortVersionString')}, không phải {version}")
    r = _codesign(["--verify", "--deep", "--strict", str(new)], run)
    if r.returncode != 0:
        raise UpdateError("chữ ký của bản mới không hợp lệ")
    want = bundle_team(running, run) if running is not None else None
    if not want:
        raise UpdateError(NO_TEAM)               # nothing to match the new signature against
    if bundle_team(new, run) != want:
        raise UpdateError("bản mới không cùng Team ID với app đang chạy")


MAC_SWAP = r"""#!/bin/sh
# Written by clawd_desktop/appupdate.py. Waits for Clawd to quit, swaps the bundle (the old
# one stays until the new one is in place) and opens it again.
PID="$1"; NEW="$2"; APP="$3"; STAGE="$4"; VER="$5"
log() { printf '%s %s\n' "$(date '+%H:%M:%S')" "$*" >> "$STAGE/apply.log"; }
[ -n "$APP" ] && [ -d "$NEW" ] || { log "bad arguments"; exit 2; }
i=0
while kill -0 "$PID" 2>/dev/null; do
  i=$((i + 1))
  if [ "$i" -gt 150 ]; then log "Clawd did not quit"; echo "$VER" > "$STAGE/failed"; exit 1; fi
  sleep 0.2
done
TMP="$APP.update-$$"; OLD="$APP.old-$$"
fail() { log "failed: $1"; echo "$VER" > "$STAGE/failed"; rm -rf "$TMP"; open "$APP"; exit 1; }
rm -rf "$TMP"
ditto "$NEW" "$TMP" || fail "copy"
mv "$APP" "$OLD" || fail "move the old app"
if ! mv "$TMP" "$APP"; then mv "$OLD" "$APP"; fail "move the new app"; fi
rm -rf "$OLD" "$NEW" "$STAGE/ready.json"
log "updated to $VER"
open "$APP"
"""


# =============================================================================
# The updater: state, staging, applying
# =============================================================================
@dataclass(frozen=True)
class Staged:
    version: str
    path: Path            # Clawd.app (mac) or the installer (Windows)
    sha256: str           # of the installer / the zip it came from
    notes: str
    page: str


def running_bundle(executable: str | None = None) -> Path | None:
    """`/Applications/Clawd.app` for the frozen macOS app, else None."""
    exe = Path(executable or sys.executable).resolve()
    if len(exe.parents) > 3 and exe.parents[2].suffix == ".app" and exe.parents[1].name == "Contents":
        return exe.parents[2]
    return None


def can_apply(frozen: bool | None = None, system: str | None = None, executable: str | None = None) -> tuple[bool, str]:
    """(ok, why not): only an installed, packaged app replaces itself."""
    system = system or this_platform()[0]
    if not (paths.frozen() if frozen is None else frozen):
        return False, "Bản chạy từ mã nguồn không tự cập nhật."
    if system == "windows":
        return True, ""
    if system != "mac":
        return False, "Hệ điều hành này chưa hỗ trợ tự cập nhật."
    app = running_bundle(executable)
    if app is None:
        return False, "Không tìm thấy Clawd.app đang chạy."
    if "/AppTranslocation/" in str(app) or str(app).startswith("/Volumes/"):
        return False, "Hãy kéo Clawd vào thư mục Applications rồi mở lại để app tự cập nhật."
    if not os.access(app.parent, os.W_OK):
        return False, f"Không có quyền ghi vào {app.parent}; hãy cài bản mới bằng tay."
    return True, ""


class Updater:
    """One per process (the tray and the settings window each have their own); they
    meet through the files in `stage`."""

    def __init__(self, source: Source | None = None, stage: Path | None = None, current: str = __version__,
                 run: Runner = subprocess.run, now: Callable[[], float] = time.time,
                 system: str | None = None, arch: str | None = None, frozen: bool | None = None,
                 executable: str | None = None) -> None:
        self.source = source
        self.stage = stage or STAGE_DIR
        self.current = current
        self._run, self._now = run, now
        self._system, self._arch = (system, arch) if system else this_platform()
        self._frozen, self._exe = frozen, executable
        self._team: str | None = None          # the running app's Team ID ("": none), read once
        self._tried = 0.0                      # when this process last asked GitHub
        self._lock = threading.Lock()          # the state below
        self._work = threading.Lock()          # one check/download at a time
        self._st = {"status": "idle", "version": "", "notes": "", "page": "", "progress": 0.0,
                    "error": "", "checked": 0.0}

    # ---- what the page and the tray read ---------------------------------------
    def _can(self) -> tuple[bool, str]:
        ok, why = can_apply(self._frozen, self._system, self._exe)
        if ok and self._system == "mac":
            if self._team is None:                 # asked once: codesign is not free
                try:
                    self._team = bundle_team(running_bundle(self._exe), self._run) or ""   # type: ignore[arg-type]
                except UpdateError:
                    self._team = ""
            if not self._team:
                return False, NO_TEAM
        return ok, why

    def state(self) -> dict:
        ok, why = self._can()
        with self._lock:
            st = dict(self._st)
        if st["status"] not in ("checking", "downloading"):
            s = self.staged()
            if s is None and self._held_elsewhere():
                st.update(status="downloading", progress=0.0)       # the other process is on it
            if s is not None:
                if self._failed() == s.version:
                    st.update(status="error", version=s.version, notes=s.notes, page=s.page, error=(
                        "Lần thay app trước không thành công. Hãy tải bản mới từ trang phát hành và cài đè."))
                    ok = False
                else:
                    st.update(status="ready", version=s.version, notes=s.notes, page=s.page, error="")
        return {**st, "current": self.current, "canApply": ok, "why": why}

    def _set(self, **kw) -> None:
        with self._lock:
            self._st.update(kw)

    # ---- the staging directory ---------------------------------------------------
    def _failed(self) -> str:
        with contextlib.suppress(OSError):
            return (self.stage / "failed").read_text().strip()
        return ""

    def staged(self) -> Staged | None:
        """The verified update waiting to be applied, None when there is none (or it is stale)."""
        try:
            d = json.loads((self.stage / "ready.json").read_text())
            version, sha, notes, page = str(d["version"]), str(d["sha256"]), str(d.get("notes", "")), str(d.get("page", ""))
            path = self.stage / str(d["path"])
        except (OSError, ValueError, KeyError, TypeError):
            if self.stage.exists() and not self._lockfile.exists():
                self.clean()                       # the swap script removed ready.json: its leftovers go too
            return None
        if not is_newer(version, self.current) or not path.exists():
            if not self._held_elsewhere():
                self.clean()                       # applied (or older than what runs): tidy up
            return None
        return Staged(version, path, sha, notes, page)

    def clean(self) -> None:
        shutil.rmtree(self.stage, ignore_errors=True)

    # ---- one downloader at a time across the tray and the settings window ---------
    @property
    def _lockfile(self) -> Path:
        return self.stage.with_name(self.stage.name + ".lock")

    def _held_elsewhere(self) -> bool:
        """Another process is downloading: its lock was touched in the last LOCK_STALE s.
        (No pid check: os.kill(pid, 0) is not a probe on Windows.)"""
        try:
            held = self._now() - self._lockfile.stat().st_mtime < LOCK_STALE
            return held and self._lockfile.read_text().strip() != str(os.getpid())
        except (OSError, ValueError):
            return False

    def _acquire(self) -> bool:
        lock = self._lockfile
        lock.parent.mkdir(parents=True, exist_ok=True)
        for _ in range(2):
            try:
                fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            except FileExistsError:
                if self._held_elsewhere() or self._now() - lock.stat().st_mtime < LOCK_STALE:
                    return False
                lock.unlink(missing_ok=True)           # a dead downloader's
                continue
            except OSError:
                return False
            with os.fdopen(fd, "w") as f:
                f.write(str(os.getpid()))
            return True
        return False

    def _release(self) -> None:
        self._lockfile.unlink(missing_ok=True)

    def _stage(self, rel: AppRelease, file: Path) -> Staged:
        """The downloaded file becomes the staged update."""
        if self._system == "mac":
            tmp = self.stage / "unpack"
            shutil.rmtree(tmp, ignore_errors=True)
            tmp.mkdir(parents=True)
            r = self._run(["ditto", "-x", "-k", str(file), str(tmp)], capture_output=True, text=True, timeout=300)
            apps = [p for p in tmp.iterdir() if p.suffix == ".app"]
            if r.returncode != 0 or len(apps) != 1:
                raise UpdateError("không giải nén được bản mới")
            check_bundle(apps[0], rel.version, running_bundle(self._exe), self._run)
            final = self.stage / "Clawd.app"
            shutil.rmtree(final, ignore_errors=True)
            apps[0].rename(final)
            shutil.rmtree(tmp, ignore_errors=True)
            file.unlink(missing_ok=True)
            path = final
        else:
            path = file
        meta = {"version": rel.version, "sha256": rel.sha256, "path": path.name, "notes": rel.notes, "page": rel.page}
        tmp_json = self.stage / "ready.json.tmp"
        tmp_json.write_text(json.dumps(meta))
        tmp_json.replace(self.stage / "ready.json")
        return Staged(rel.version, path, rel.sha256, rel.notes, rel.page)

    # ---- checking and downloading (blocking: run them in a thread) ---------------
    def check(self, force: bool = False) -> dict:
        """Ask GitHub; when there is a newer release and this install can apply it, download it
        and stage it. Returns the state. A second call while one runs returns at once."""
        if self.source is None:
            return self.state()
        if not self._work.acquire(blocking=False):
            return self.state()
        if not self._acquire():                      # the other process is checking or downloading
            self._work.release()
            return self.state()
        try:
            if not force and self._tried and self._now() - self._tried < RECHECK:
                return self.state()            # failures count too: GitHub allows 60 anonymous asks an hour
            self._tried = self._now()
            self._set(status="checking", error="", progress=0.0)
            try:
                rel = self.source.latest(self.current, self._system, self._arch)
            except UpdateError as e:
                self._set(status="error", error=f"Không hỏi được GitHub: {e}")
                log(f"check failed: {e}")
                return self.state()
            self._set(checked=self._now())
            if rel is None:
                self._set(status="current", version="", notes="", page="")
                return self.state()
            s = self.staged()
            if s is not None and s.version == rel.version:
                self._set(status="idle")
                return self.state()
            self._set(version=rel.version, notes=rel.notes, page=rel.page)
            ok, why = self._can()
            if not ok:
                log(f"{rel.version} is out; not applied here: {why}")
                self._set(status="available")
                return self.state()
            if self._failed() == rel.version:
                self._set(status="error", error="Lần thay app trước không thành công. Hãy tải bản mới từ trang phát hành.")
                return self.state()
            try:
                self._fetch(rel)
            except (UpdateError, OSError, subprocess.SubprocessError) as e:
                self._set(status="error", error=f"Không tải được bản {rel.version}: {e}")
                log(f"{rel.version}: {e}")
                shutil.rmtree(self.stage / "unpack", ignore_errors=True)
                return self.state()
            self._set(status="idle")
            log(f"{rel.version} downloaded and verified")
            return self.state()
        finally:
            self._release()
            self._work.release()

    def _fetch(self, rel: AppRelease) -> None:
        assert self.source is not None
        self.clean()                                 # an older staged download is of no use now
        self.stage.mkdir(parents=True, exist_ok=True)
        self._set(status="downloading", progress=0.0)
        file = self.stage / rel.name
        beat = [self._now()]

        def progress(got: int, total: int) -> None:
            self._set(progress=round(got / max(total, 1), 3))
            if self._now() - beat[0] > 5:                # tell the other process we are alive
                beat[0] = self._now()
                with contextlib.suppress(OSError):
                    os.utime(self._lockfile)
        self.source.download(rel, file, progress)
        self._stage(rel, file)

    def check_async(self, force: bool = False) -> None:
        threading.Thread(target=self.check, args=(force,), name="clawd-appupdate", daemon=True).start()

    # ---- applying ----------------------------------------------------------------
    def apply(self, pid: int | None = None) -> tuple[bool, str]:
        """Start the replacement. On True the caller must quit the app now: the installer
        (Windows) or the swap script (macOS) takes over once this process is gone."""
        ok, why = self._can()
        if not ok:
            return False, why
        s = self.staged()
        if s is None:
            return False, "Chưa có bản nào được tải xong."
        if paths.ota_busy():
            return False, "Đang cập nhật firmware cho đồng hồ; hãy đợi xong rồi khởi động lại."
        pid = os.getpid() if pid is None else pid
        try:
            if self._system == "windows":
                if sha256_file(s.path) != s.sha256:
                    self.clean()
                    return False, "File cài đặt đã bị đổi, đã bỏ. Hãy kiểm tra lại bản mới."
                flags = 0
                if sys.platform == "win32":
                    flags = (getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0))
                subprocess.Popen([str(s.path), "/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART"], close_fds=True,
                                 stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                 creationflags=flags)
            else:
                app = running_bundle(self._exe)
                assert app is not None
                check_bundle(s.path, s.version, app, self._run)      # again: it sat on disk since the download
                (self.stage / "failed").unlink(missing_ok=True)
                script = self.stage / "swap.sh"
                script.write_text(MAC_SWAP)
                script.chmod(0o700)
                subprocess.Popen(["/bin/sh", str(script), str(pid), str(s.path), str(app), str(self.stage), s.version],
                                 start_new_session=True, close_fds=True,
                                 stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except (OSError, UpdateError, subprocess.SubprocessError) as e:
            log(f"apply failed: {e}")
            return False, f"Không bắt đầu được cập nhật: {e}"
        log(f"applying {s.version}")
        return True, ""


def updater(transport: httpx.BaseTransport | None = None) -> Updater:
    """The updater this install uses (the repo from the environment or the config file)."""
    try:
        return Updater(from_settings(transport))
    except UpdateError as e:
        log(str(e))
        return Updater(None)
