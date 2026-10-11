---
title: "Clawdmeter on ESP8266 SmallTV: HTTP push + smalltv-mod fork"
date: 2026-10-11
summary: "Daemon pushes usage over HTTP; private smalltv-mod fork ports Clawdmeter UI (usage, Enterprise, clock, alert, mascot, 17 Clawd animations) to the ESP8266 clock; builds pass, hardware unverified"
---

# Clawdmeter on ESP8266 SmallTV: HTTP push + smalltv-mod fork

## What happened
- User's JUZIPi "Smart Weather Clock" is an ESP8266 (GeekMagic SmallTV-Ultra class: 240x240 ST7789, WiFi only, no buttons). Clawdmeter firmware (LVGL 9, ESP32-S3/C6, BLE) cannot run on it.
- Daemon (this repo, branch `feat/daemon-http-push`, commit ebf1b58): `http_targets` POSTs the same JSON payload to `/api/usage`; `Publisher` owns one poll loop shared by BLE + HTTP sinks; `ble = off` for WiFi-only. 12 new tests; suite 136 passed / 2 skipped.
- Firmware: private fork `~/Downloads/code/smalltv-clawdmeter` (upstream giovi321/smalltv-mod 9201e86, branch `clawdmeter`, commit f2ccda5, push URL disabled).
  - `features/clawd`: assets generated from Clawdmeter's `splash_animations.h` + LVGL fonts (17 anims 4-bit = 209 KB, subset 4bpp fonts with kerning, PROGMEM).
  - Renderer composes 4-row bands (1,920 B) with AA rounded shapes and AA text, one SPI push per band: no framebuffer, no flicker. Pixel art diffed per grid row.
  - Ports of splash.cpp (intro/loop/outro, walkers, rate groups), usage_rate.cpp, corner mascot, idle cloud, alert (marquee + blinking frame + mascot attention).
  - Web UI: screen Auto/Usage/Splash + animation picker; `/api/heap` (unauth, read-only) for 24 h soak.
  - Desktop preview (`tools/clawd_preview/run.sh`) renders PNGs of every screen with the same code; used for visual QA.

## Numbers
- `smalltv_clawd`: static RAM 46,904 B (upstream full build 61,824 B), flash 838,533 B, firmware.bin 842,688 B.
- Moving the 90 "whimsical" words to a PROGMEM blob saved 1,260 B DRAM; halving the band buffer saved 1,920 B.

## Decisions
- Assets in firmware PROGMEM, not LittleFS (OTA does not need to preserve FS; fits in 1 MB app).
- GitHub self-updater compiled to an error in `SMALLTV_CLAWD`: upstream releases would wipe the Clawd UI. `/update` is the only update path.
- Full `smalltv` env still builds (81% RAM, 94% flash) but is not recommended.

## Not verified (needs the device)
- Whether the stock JUZIPi firmware accepts an 843 KB OTA (else use `smalltv_loader` first); panel pinout/colour; runtime heap and 24 h stability; OTA round trip from the new image. ESP32 envs were not rebuilt.

## Next steps
- User reaches the clock web UI, flashes loader/firmware via `/update`, sets `http_targets` in the daemon config, logs `/api/heap` for 24 h.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
