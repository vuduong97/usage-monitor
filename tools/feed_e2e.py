#!/usr/bin/env python3
"""End-to-end check of the firmware feed against a running Worker and a fake clock.

  # in clawd-home: publish to a local store, serve it
  sh tools/release.sh --dirty --local /tmp/fwfeed-store --stable
  (cd feed/worker && npx wrangler dev --local --persist-to /tmp/fwfeed-store --port 8799 \\
      --var APP_TOKENS:test-token --var URL_SECRET:dev-secret-123)
  # here:
  uv run python tools/feed_e2e.py --feed http://127.0.0.1:8799 --token test-token \\
      --fw ../clawd-home/firmware/.pio/build/clawd/firmware.bin \\
      --clawd ../clawd-home/firmware/data/clawd.pak --desk ../clawd-home/build/desk/desk.pak

It checks the Worker's refusals (no token, no or a forged signature), then runs the
settings window's update job (SettingsApi + the real ClockApi over HTTP) against a
fake clock on 127.0.0.1 that is one release behind, and compares what the clock
received with the plaintext files the release was built from.
"""
from __future__ import annotations

import argparse
import json
import sys
import tempfile
import threading
import time
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import httpx

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from clawd_desktop import feed, ota, settings_api as sa  # noqa: E402
from clawd_desktop.discovery import ClockStore  # noqa: E402


def multipart_file(body: bytes, ctype: str) -> bytes:
    boundary = ctype.split("boundary=", 1)[1].strip('"').encode()
    part = body.split(b"--" + boundary)[1]
    return part.split(b"\r\n\r\n", 1)[1].rsplit(b"\r\n", 1)[0]


class Clock:
    def __init__(self, seq: int) -> None:
        self.seq = seq
        self.boot = time.monotonic() - 3600     # up for an hour before the update
        self.got: dict[str, bytes] = {}
        self.crc = {"pakCrc": "", "deskCrc": "11111111"}
        self.cfg = {"hostname": "clawd", "brightness": 40, "usage": {"screen": 0, "anim": -1, "layout": 0},
                    "wifi": [{"ssid": "Home", "passSet": True}], "auth": {"enabled": False, "user": "admin",
                                                                           "passSet": False}}

    def heap(self) -> dict:
        return {"variant": "esp8266-clawd", "fwSeq": self.seq, "layouts": 1, "styleApi": 1, "style": "",
                "pak": 17, "desk": 31, "uptime": int(time.monotonic() - self.boot), "version": "2.16.0", **self.crc}


def serve(clock: Clock, new_seq: int) -> ThreadingHTTPServer:
    class H(BaseHTTPRequestHandler):
        def log_message(self, *_a) -> None:
            pass

        def _json(self, d: dict, code: int = 200) -> None:
            b = json.dumps(d).encode()
            self.send_response(code)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(b)))
            self.end_headers()
            self.wfile.write(b)

        def do_GET(self) -> None:
            if self.path.startswith("/api/heap"):
                self._json(clock.heap())
            elif self.path.startswith("/api/config"):
                self._json(clock.cfg)
            elif self.path.startswith("/api/status"):
                self._json({"mode": "sta", "connected": True, "ssid": "Home", "rssi": -50})
            elif self.path.startswith("/api/style"):
                self._json({"api": 1, "active": False})
            else:
                self._json({"error": "nf"}, 404)

        def do_POST(self) -> None:
            body = self.rfile.read(int(self.headers.get("content-length") or 0))
            if self.path == "/update":
                clock.got["fw"] = multipart_file(body, self.headers["content-type"])
                self.send_response(200)
                self.send_header("content-length", "2")
                self.end_headers()
                self.wfile.write(b"OK")

                def reboot() -> None:
                    time.sleep(1.0)
                    clock.seq, clock.boot = new_seq, time.monotonic()
                    clock.crc = {"pakCrc": "", "deskCrc": ""}       # checksummed in the background...
                    threading.Timer(4.0, lambda: clock.crc.update(pakCrc="22222222", deskCrc="33333333")).start()
                threading.Thread(target=reboot, daemon=True).start()
            elif self.path in ("/api/clawdpak", "/api/deskpak"):
                kind = self.path.rsplit("/", 1)[1]
                data = multipart_file(body, self.headers["content-type"])
                clock.got[kind] = data
                clock.crc["pakCrc" if kind == "clawdpak" else "deskCrc"] = f"{zlib.crc32(data):08x}"
                self._json({"ok": True, "anims": 17, "crc": f"{zlib.crc32(data):08x}"})
            elif self.path == "/api/config":
                clock.cfg.update(json.loads(body))
                self._json({"ok": True, "reboot": False})
            else:
                self._json({"error": "nf"}, 404)

    srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--feed", required=True)
    ap.add_argument("--token", required=True)
    ap.add_argument("--channel", default="stable")
    ap.add_argument("--fw", type=Path, required=True)
    ap.add_argument("--clawd", type=Path, required=True)
    ap.add_argument("--desk", type=Path, required=True)
    a = ap.parse_args()
    bad = 0

    def check(ok: bool, what: str) -> None:
        nonlocal bad
        print(("ok   " if ok else "FAIL ") + what)
        bad += not ok

    # the Worker's refusals
    h = httpx.Client(timeout=20)
    check(h.get(a.feed + "/v1/latest").status_code == 401, "latest without a token: 401")
    tok = {"authorization": f"Bearer {a.token}"}
    m = h.get(a.feed + "/v1/latest", params={"channel": a.channel}, headers=tok).json()
    seq = m["seq"]
    check(h.get(f"{a.feed}/v1/file/{seq}/firmware.bin").status_code == 403, "file without a signature: 403")
    g = h.post(a.feed + "/v1/grant", json={"seq": seq}, headers=tok).json()
    u = g["urls"]["firmware.bin"]
    forged = u.replace("firmware.bin", "desk.pak")
    check(h.get(a.feed + forged).status_code == 403, "signature moved to another file: 403")
    check(h.get(f"{a.feed}/rel/{seq}/firmware.bin.enc").status_code == 404, "raw bucket path: 404")
    blob = h.get(a.feed + u).content
    plain = a.fw.read_bytes()
    check(plain not in blob and plain[:64] not in blob, "the served firmware is ciphertext")

    # the update job, as the settings window runs it
    clock = Clock(seq - 1)
    srv = serve(clock, seq)
    base = f"http://127.0.0.1:{srv.server_address[1]}"
    ota.SETTLE = 0
    src = feed.Feed(a.feed, a.token, a.channel)
    with tempfile.TemporaryDirectory() as tmp:
        class NoKeys:
            def get(self, _key):
                return None
        b = sa.SettingsApi(base, "clawd", feed=src, store=ClockStore(Path(tmp) / "clock.json"), keychain=NoKeys(),
                           payload_source=lambda: None, remember_screen=lambda _s: None)
        s = b.state()
        fw = s.get("fw") or {}
        check(s.get("ok") and fw.get("update") == "available" and fw["bundle"]["source"] == "feed",
              f"window offers release {fw.get('bundle', {}).get('seq')} from the feed")
        check(len(fw["styles"]["packs"]) > 0, f"gallery has {len(fw['styles']['packs'])} style packs from the feed")
        t0 = time.monotonic()
        check(b.update(None, None)["ok"], "update started")
        while b.job()["running"] and time.monotonic() - t0 < 300:
            time.sleep(0.5)
        j = b.job()
        check(j.get("done") and not j.get("error"), f"update finished in {time.monotonic() - t0:.0f} s {j}")
    check(clock.got.get("fw") == plain, f"clock got the firmware ({len(clock.got.get('fw', b''))} B)")
    check(clock.got.get("clawdpak") == a.clawd.read_bytes(), "clock got clawd.pak")
    check(clock.got.get("deskpak") == a.desk.read_bytes(), "clock got desk.pak")
    check(clock.seq == seq, f"clock now runs #{clock.seq}")
    srv.shutdown()
    print("feed e2e:", "FAILED" if bad else "ok")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
