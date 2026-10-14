"""A firmware feed in memory for the tests: releases encrypted the way clawd-home's
feed/release.mjs does, served by a fake Worker (httpx.MockTransport)."""
from __future__ import annotations

import base64
import hashlib
import json
import os
import stat
import sys
import zlib
from pathlib import Path

import httpx
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from tests.test_firmware_ota import make_pack, write_styles

TOOLS = Path(__file__).resolve().parent.parent / "tools"

IMAGE = b"\xe9" + b"\x07" * 2047
CLAWD = b"CLWD\x01\x00\x11\x00" + b"c" * 500
DESK = b"CLDK\x03\x00\x1f\x00" + b"d" * 3000
TOKEN = "tok"  # noqa: S105  (a fixture for the fake Worker, not a secret)


def crc(data: bytes) -> str:
    return f"{zlib.crc32(data):08x}"


def renderer_zip(tmp_path, commit="c000006", body="exit 0", styles=("synth", "paper")) -> bytes:
    """A renderer zip as tools/build_preview.py --zip packs it, around a fake program (`body`,
    a shell script)."""
    if str(TOOLS) not in sys.path:
        sys.path.insert(0, str(TOOLS))
    import build_preview  # pyright: ignore[reportMissingImports]  (tools/ is on sys.path above)
    d = tmp_path / f"renderer-src-{os.urandom(4).hex()}"
    d.mkdir(parents=True)
    write_styles(d / "styles", styles)
    exe = d / "clawd_render"
    exe.write_text("#!/bin/sh\n" + body)
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    (d / "clawd.pak").write_bytes(b"CLWD")
    (d / "preview.json").write_text(json.dumps({"exe": "clawd_render", "pak": "clawd.pak", "commit": commit,
                                                "layouts": 1, "styleApi": 1, "fwSeq": 6}))
    out = d.parent / (d.name + ".zip")
    build_preview.write_zip(d, "clawd_render", out)
    return out.read_bytes()


def release_files(image=IMAGE, clawd=CLAWD, desk=DESK, styles=("synth", "paper"), api=1):
    files = [("firmware.bin", "fw", image), ("clawd.pak", "clawdpak", clawd), ("desk.pak", "deskpak", desk)]
    if styles:
        rows, packs = [], []
        for sid in styles:
            data = make_pack(sid)
            rows.append({"id": sid, "name": sid.title(), "file": f"{sid}.pak", "size": len(data),
                         "crc": crc(data[16:]), "light": sid == "paper"})
            packs.append((f"styles/{sid}.pak", "style", data))
        files.append(("styles/index.json", "styles", json.dumps({"api": api, "styles": rows}).encode()))
        files += packs
    return files


class FakeFeed:
    """The Worker: /v1/latest, /v1/grant, /v1/file. `releases`: seq -> (manifest, key, blobs)."""

    def __init__(self):
        self.releases: dict[int, tuple[dict, bytes, dict]] = {}
        self.channels: dict[str, int] = {}
        self.calls: list[str] = []
        self.tamper: set[str] = set()          # names served with a flipped byte
        self.sign_renderers = True             # False: a Worker from before renderers (grant skips them)

    def publish(self, seq, files=None, channel="stable", notes="", style_api=1, renderers=()):
        """`renderers`: (platform, zip bytes) rows of the manifest's `renderers` list."""
        key = os.urandom(32)
        blobs, rows, rrows = {}, [], []
        extra = [(f"renderer/{p}.zip", "renderer", data, p) for p, data in renderers]
        for name, kind, data, *plat in [*(f + () for f in files or release_files()), *extra]:
            nonce = os.urandom(12)
            blob = nonce + AESGCM(key).encrypt(nonce, data, f"clawd-fw/{seq}/{name}".encode())
            blobs[name] = blob
            row = {"name": name, "kind": kind, "size": len(data), "enc": len(blob),
                   "sha256": hashlib.sha256(data).hexdigest(), "crc": crc(data)}
            if plat:
                rrows.append({**row, "platform": plat[0]})
            else:
                rows.append(row)
        m = {"v": 1, "seq": seq, "date": "2026-10-14T08:00:00Z", "notes": notes, "variant": "esp8266-clawd",
             "layouts": 1, "styleApi": style_api, "commit": f"c{seq:06d}", "files": rows}
        if rrows:
            m["renderers"] = rrows
        self.releases[seq] = (m, key, blobs)
        self.channels[channel] = seq
        return m

    def handler(self, req: httpx.Request) -> httpx.Response:
        path = req.url.path
        self.calls.append(f"{req.method} {path}")
        if path.startswith("/v1/file/"):
            _, _, _, seq, name = path.split("/", 4)
            if req.url.params.get("sig") != "ok":
                return httpx.Response(403)
            blob = self.releases[int(seq)][2][name]
            if name in self.tamper:
                blob = blob[:-1] + bytes([blob[-1] ^ 1])
            return httpx.Response(200, content=blob)
        if req.headers.get("authorization") != f"Bearer {TOKEN}":
            return httpx.Response(401)
        if path == "/v1/latest":
            seq = self.channels.get(req.url.params.get("channel", "stable"))
            return httpx.Response(200, json=self.releases[seq][0]) if seq else httpx.Response(404)
        if path == "/v1/grant":
            seq = json.loads(req.content)["seq"]
            m, key, blobs = self.releases[seq]
            return httpx.Response(200, json={"seq": seq, "key": base64.b64encode(key).decode(), "expires": 0,
                                             "urls": {n: f"/v1/file/{seq}/{n}?exp=1&sig=ok" for n in blobs
                                                      if self.sign_renderers or not n.startswith("renderer/")}})
        return httpx.Response(404)

    def got(self, name: str) -> int:
        """How often the file `name` was downloaded."""
        return sum(c.startswith("GET /v1/file/") and c.endswith("/" + name) for c in self.calls)

    def client(self, channel="stable", cache_dir=None):
        from clawd_desktop import feed
        return feed.Feed("https://fw.test", TOKEN, channel, transport=httpx.MockTransport(self.handler),
                         cache_dir=cache_dir)
