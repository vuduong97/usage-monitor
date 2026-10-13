"""The firmware feed: clock firmware and packs newer than what the app ships.

A release (firmware.bin, clawd.pak, desk.pak, the style packs) is published by the
firmware repo's tools/release.sh to a private bucket behind a small Worker
(clawd-home feed/README.md). The app asks it, with its app token:

  GET  /v1/latest?channel=stable      the manifest: seq, notes, one row per file
                                      {name, kind, size, enc, sha256, crc}
  POST /v1/grant {"seq": N}           the release key (AES-256-GCM) and signed URLs
                                      that work for a few minutes

Every file is `nonce(12) | ciphertext | tag(16)` with AAD "clawd-fw/<seq>/<name>".
It is decrypted in memory and checked against the manifest's size and SHA-256;
nothing decrypted is written to disk. The result is a `Release`, which the rest
of the app treats like the bundled firmware (`firmware.Bundle`), plus the packs.

The URL and token come from CLAWD_FEED_URL / CLAWD_FEED_TOKEN, the config file
(`feed_url`, `feed_token`, `feed_channel`), or the build (`_feed_build.py`,
written by packaging, git-ignored). Without both, there is no feed.
"""
from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import zlib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

import httpx
from cryptography.exceptions import InvalidTag
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from clawd_desktop import firmware, usage

TIMEOUT = 30.0
DOWNLOAD_TIMEOUT = 180.0
CHECK_EVERY = 6 * 3600.0                    # the tray's background check
KINDS = {"fw", "clawdpak", "deskpak", "styles", "style"}
PACK_KINDS = ("clawdpak", "deskpak")
STYLE_KINDS = frozenset({"styles", "style"})
NAME_RE = re.compile(r"[a-z0-9][a-z0-9._-]{0,47}(/[a-z0-9][a-z0-9._-]{0,47})?")
HEX64 = re.compile(r"[0-9a-f]{64}")
HEX8 = re.compile(r"[0-9a-f]{8}")
MAX_FILE = 4 * 1024 * 1024


class FeedError(Exception):
    """The feed did not answer, refused, or sent something that does not check out."""


@dataclass(frozen=True)
class FeedFile:
    name: str
    kind: str
    size: int
    enc: int
    sha256: str
    crc: str


@dataclass(frozen=True)
class Manifest:
    seq: int
    variant: str
    layouts: int
    style_api: int
    files: tuple[FeedFile, ...]
    date: str = ""
    notes: str = ""
    commit: str = ""

    def file(self, kind: str) -> FeedFile | None:
        return next((f for f in self.files if f.kind == kind), None)


def _int(v, lo: int, hi: int) -> int:
    if not isinstance(v, int) or isinstance(v, bool) or not lo <= v <= hi:
        raise ValueError(f"bad number {v!r}")
    return v


def parse_manifest(d: object) -> Manifest:
    """The manifest, every field checked. ValueError when anything is off."""
    if not isinstance(d, dict) or d.get("v") != 1:
        raise ValueError("not a v1 manifest")
    files, names = [], set()
    for f in d.get("files") or []:
        if not isinstance(f, dict):
            raise ValueError("bad file row")
        name, kind = f.get("name"), f.get("kind")
        if not isinstance(name, str) or not NAME_RE.fullmatch(name) or ".." in name or name in names:
            raise ValueError(f"bad file name {name!r}")
        if kind not in KINDS:
            raise ValueError(f"unknown kind {kind!r}")
        size = _int(f.get("size"), 1, MAX_FILE)
        sha, crc = f.get("sha256"), f.get("crc")
        if not isinstance(sha, str) or not HEX64.fullmatch(sha) or not isinstance(crc, str) or not HEX8.fullmatch(crc):
            raise ValueError(f"bad hash for {name}")
        names.add(name)
        files.append(FeedFile(name, kind, size, _int(f.get("enc"), size + 28, size + 28), sha, crc))
    m = Manifest(seq=_int(d.get("seq"), 1, 1_000_000), variant=str(d.get("variant") or ""),
                 layouts=_int(d.get("layouts", 1), 1, 0xFFFF), style_api=_int(d.get("styleApi", 0), 0, 255),
                 files=tuple(files), date=str(d.get("date") or "")[:40], notes=str(d.get("notes") or "")[:2000],
                 commit=str(d.get("commit") or "")[:40])
    if m.file("fw") is None or sum(f.kind == "fw" for f in files) != 1:
        raise ValueError("a release has exactly one firmware image")
    return m


def decrypt(key: bytes, seq: int, name: str, blob: bytes) -> bytes:
    if len(blob) < 28:
        raise FeedError(f"{name}: too short")
    try:
        return AESGCM(key).decrypt(blob[:12], blob[12:], f"clawd-fw/{seq}/{name}".encode())
    except InvalidTag as e:
        raise FeedError(f"{name}: does not decrypt (wrong key or damaged)") from e


@dataclass(frozen=True)
class Release(firmware.Bundle):
    """A feed release in memory. `read()` is the firmware image (once fetched);
    `pack(kind)` a clawd/desk pack; `styles` the style packs (once fetched)."""
    manifest: Manifest | None = None
    blobs: dict = field(default_factory=dict, repr=False, compare=False)   # name -> plaintext

    @property
    def notes(self) -> str:
        return self.manifest.notes if self.manifest else ""

    @property
    def date(self) -> str:
        return self.manifest.date if self.manifest else ""

    def has(self, kind: str) -> bool:
        m = self.manifest
        return bool(m) and all(f.name in self.blobs for f in m.files if f.kind == kind) and \
            any(f.kind == kind for f in m.files)

    def missing(self, kinds) -> set[str]:
        """Of `kinds`, the ones the release has but not in memory yet."""
        m = self.manifest
        have = {f.kind for f in m.files} if m else set()
        return {k for k in kinds if k in have and not self.has(k)}

    def read(self) -> bytes:
        m = self.manifest
        f = m.file("fw") if m else None
        data = self.blobs.get(f.name) if f else None
        if data is None:
            raise ValueError("the firmware is not downloaded yet")
        if len(data) != self.size or hashlib.sha256(data).hexdigest() != self.sha256:
            raise ValueError("feed firmware does not match its manifest")
        return data

    def pack(self, kind: str) -> tuple[bytes, str] | None:
        """(the pack, its CRC-32 hex) for "clawdpak" / "deskpak", None when the release has none."""
        f = self.manifest.file(kind) if self.manifest else None
        if f is None or f.name not in self.blobs:
            return None
        return self.blobs[f.name], f.crc


def _styles(m: Manifest, blobs: dict) -> tuple[firmware.StylePack, ...]:
    """The style packs of a release, from its index; () when absent or not for its style API."""
    idx = m.file("styles")
    if idx is None or idx.name not in blobs or m.style_api < 1:
        return ()
    try:
        d = json.loads(blobs[idx.name])
        if int(d["api"]) != m.style_api:
            return ()
        base = idx.name.rsplit("/", 1)[0] + "/" if "/" in idx.name else ""
        out = []
        for e in d["styles"]:
            sid, fname = str(e["id"]), str(e["file"])
            data = blobs.get(base + fname)
            if not firmware.STYLE_ID.fullmatch(sid) or data is None or "/" in fname:
                return ()
            out.append(firmware.StylePack(id=sid, name=str(e["name"])[:23], path=Path(fname), size=int(e["size"]),
                                          crc=int(str(e["crc"]), 16), light=bool(e.get("light")), data=data))
        if len({p.id for p in out}) != len(out):
            return ()
        for p in out:
            p.read()                         # every pack checks against the index and its header
        return tuple(out)
    except (ValueError, KeyError, TypeError):
        return ()


def make_release(m: Manifest, blobs: dict | None = None) -> Release:
    blobs = dict(blobs or {})
    fw = m.file("fw")
    assert fw is not None
    return Release(path=Path(fw.name), size=fw.size, sha256=fw.sha256, variant=m.variant, seq=m.seq,
                   layouts=m.layouts, commit=m.commit, style_api=m.style_api, styles=_styles(m, blobs),
                   manifest=m, blobs=blobs)


class Feed:
    def __init__(self, url: str, token: str, channel: str = "stable",
                 transport: httpx.BaseTransport | None = None) -> None:
        self.url = url.rstrip("/")
        self.channel = channel if channel in ("stable", "beta") else "stable"
        self._http = httpx.Client(timeout=TIMEOUT, transport=transport,
                                  headers={"authorization": f"Bearer {token}"}, follow_redirects=False)

    def close(self) -> None:
        self._http.close()

    def _get(self, method: str, path: str, **kw) -> httpx.Response:
        try:
            r = self._http.request(method, self.url + path, **kw)
        except httpx.HTTPError as e:
            raise FeedError(f"feed: {type(e).__name__}: {e}") from e
        if r.status_code >= 400:
            raise FeedError(f"feed: HTTP {r.status_code} for {path.split('?')[0]}")
        return r

    def latest(self) -> Manifest | None:
        """The channel's newest release; None when it has none yet."""
        try:
            r = self._http.get(self.url + "/v1/latest", params={"channel": self.channel})
        except httpx.HTTPError as e:
            raise FeedError(f"feed: {type(e).__name__}: {e}") from e
        if r.status_code == 404:
            return None
        if r.status_code >= 400:
            raise FeedError(f"feed: HTTP {r.status_code} for /v1/latest")
        try:
            return parse_manifest(r.json())
        except ValueError as e:
            raise FeedError(f"feed: bad manifest ({e})") from e

    def fetch(self, rel: Release, kinds, progress: Callable[[str, int, int], None] = lambda *_a: None) -> Release:
        """`rel` with the files of `kinds` downloaded, decrypted and checked (the others kept)."""
        m = rel.manifest
        assert m is not None
        want = [f for f in m.files if f.kind in set(kinds) and f.name not in rel.blobs]
        if not want:
            return rel
        g = self._get("POST", "/v1/grant", json={"seq": m.seq}).json()
        try:
            key = base64.b64decode(g["key"], validate=True)
            urls = g["urls"]
            if len(key) != 32 or not isinstance(urls, dict):
                raise ValueError
        except (KeyError, TypeError, ValueError) as e:
            raise FeedError("feed: bad grant") from e
        blobs = dict(rel.blobs)
        total, done = sum(f.enc for f in want), 0
        for f in want:
            u = urls.get(f.name)
            if not isinstance(u, str) or not u.startswith("/v1/file/"):
                raise FeedError(f"feed: no URL for {f.name}")
            progress(f.kind, done, total)
            blob = self._get("GET", u, timeout=DOWNLOAD_TIMEOUT).content
            if len(blob) != f.enc:
                raise FeedError(f"{f.name}: {len(blob)} B, the manifest says {f.enc}")
            data = decrypt(key, m.seq, f.name, blob)
            if len(data) != f.size or hashlib.sha256(data).hexdigest() != f.sha256 \
                    or f"{zlib.crc32(data):08x}" != f.crc:
                raise FeedError(f"{f.name}: does not match the manifest")
            blobs[f.name] = data
            done += f.enc
        progress("", total, total)
        return make_release(m, blobs)


def _build_defaults() -> tuple[str, str]:
    try:
        from clawd_desktop import _feed_build  # type: ignore[attr-defined]  # written by packaging
        return str(getattr(_feed_build, "URL", "")), str(getattr(_feed_build, "TOKEN", ""))
    except ImportError:
        return "", ""


def from_settings() -> Feed | None:
    """The feed this install uses, None when it has no URL or token."""
    bu, bt = _build_defaults()
    url = os.environ.get("CLAWD_FEED_URL") or usage.read_config_value("feed_url") or bu
    token = os.environ.get("CLAWD_FEED_TOKEN") or usage.read_config_value("feed_token") or bt
    if not url or not token or not url.startswith(("https://", "http://127.0.0.1", "http://localhost")):
        return None
    channel = os.environ.get("CLAWD_FEED_CHANNEL") or usage.read_config_value("feed_channel") or "stable"
    return Feed(url, token, channel)


# ---- what a clock needs from a release -----------------------------------------------
HEAP_KEYS = {"clawdpak": ("pak", "pakCrc"), "deskpak": ("desk", "deskCrc")}


def pack_state(heap: dict, kind: str, crc: str) -> str:
    """"ok" (the clock has this pack) | "need" | "wait" (it is still checksumming it) |
    "unknown" (its firmware does not say: decided after the firmware update)."""
    count_key, crc_key = HEAP_KEYS[kind]
    count = heap.get(count_key)
    if isinstance(count, int) and count == 0:
        return "need"                      # missing or unreadable
    if crc_key not in heap:
        return "unknown"
    have = heap.get(crc_key)
    if have == "":
        return "wait"
    return "ok" if have == crc else "need"


def plan(heap: dict, rel: firmware.Bundle | None) -> dict[str, str]:
    """What installing `rel` means for this clock: {"fw": "need"} when its image is newer,
    and per pack of a feed release its `pack_state`. {} when nothing applies."""
    if rel is None or firmware.update_state(heap, rel) not in ("available", "current"):
        return {}
    out = {"fw": "need"} if firmware.update_state(heap, rel) == "available" else {}
    m = rel.manifest if isinstance(rel, Release) else None
    for kind in PACK_KINDS:
        f = m.file(kind) if m else None
        if f is not None:
            out[kind] = pack_state(heap, kind, f.crc)
    return out


def needed(heap: dict, rel: firmware.Bundle | None) -> list[str]:
    """The kinds to download for an update: the firmware if newer, and every pack that
    is not known to match ("need", "wait", "unknown")."""
    return [k for k, s in plan(heap, rel).items() if s != "ok"]


def worth_it(heap: dict, rel: firmware.Bundle | None) -> bool:
    """Something to install now: a newer image, or a pack known to differ."""
    p = plan(heap, rel)
    return p.get("fw") == "need" or any(p.get(k) == "need" for k in PACK_KINDS)


def offer(heap: dict, m: Manifest | None, bundle: firmware.Bundle | None = None) -> int:
    """The release number the tray should offer for this clock, 0 for none. A bundled
    image at least as new as the feed's wins (a private build)."""
    if m is None:
        return 0
    if bundle is not None and bundle.seq >= m.seq:
        return bundle.seq if firmware.update_state(heap, bundle) == "available" else 0
    return m.seq if worth_it(heap, make_release(m)) else 0
