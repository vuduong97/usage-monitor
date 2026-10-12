# Show only the IP on screen, caption it under the waiting splash

2026-10-12 · commit 950c9f9 · firmware smalltv_clawd 514,784 B

## Why
The CONNECTED boot screen showed both the IP and `http://<host>.local`. The
`.local` name needs multicast on the network and mDNS on the phone, so it
often fails where the IP works, and users read it as "the clock is broken".
The bigger gap: the IP was only on screen for 3.5 s at boot, then nowhere.

## What changed
- `gfxStaInfo(ssid, ip)`: the `.local` line and the `host` parameter are gone.
  mDNS and `_clawdmeter._tcp` stay on (Clawd.app discovery).
- Splash bottom band: `ClawdSplash::setBottomBand()` renders only the top
  54 of 60 grid rows; ClawdUi draws the IP (Styrene 14, `CLAWD_DIM`) in the
  24 px band. Only when Screen = Auto and the view is splash (= no fresh data,
  at boot or stale). Screen = Clawd stays full height.
- `UsageMode::service` feeds `clawdUiSetAddress(netIP())` every 2 s (a String
  per call, so not every loop); "" when not connected, which blanks the band.
- Docs: README, setup, quick-start, troubleshooting, CLAWDMETER.md.

## Evidence
- Scanned all 17 animations in `data/clawd.pak`: the last lit row is 47 for
  every one, so rows 48-59 are always empty and a 6-row band is safe.
- Preview `checkAddressBand`: IP drawn, unchanged after 45 s (two rotations),
  redrawn on a new IP, absent for screen = splash and for an empty IP.
- Builds: smalltv_clawd (+256 B vs before), smalltv, smalltv_lean OK.

## Open
- ESP32 envs (smalltv_c2, smalltv_esp32) were not built yet: the first build
  is downloading the ESP32 toolchain (~350 KB/s). Only shared touch point is
  the `gfxStaInfo` signature (one caller).
- Not flashed: the clock is unreachable from this network. At home:
  `sh tools/flash.sh <ip>`, reboot, check the CONNECTED screen has no `.local`
  and the IP shows under the splash until data arrives.
