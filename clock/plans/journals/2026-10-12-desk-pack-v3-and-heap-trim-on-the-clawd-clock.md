---
title: Desk pack v3 and heap trim on the clawd clock
date: 2026-10-12
summary: "CLDK v3 per-delta deflate (hybrid), YCbCr tables dropped, pull mode removed: fs 99%->67%, minHeap 21.5->28.2 KB"
---

# Desk pack v3 and heap trim on the clawd clock

## What happened
- Goal: free memory on the ESP8266 clock without touching animation quality.
- YCbCr: Arduino_GFX 1.6.9 linked 6,804 B of DRAM tables through the virtual `writeYCbCrPixels`. `firmware/gfx_trim.py` compiles it out (CLAWD_NO_YCBCR) and `gfx_check.py` guards with nm. GFX is pinned.
- Pull mode removed (usageUrl/pollSec/httpTimeout, HTTP client/TLS helpers, web UI fields). Static RAM 48,220 -> 43,412 B, image 510,099 -> 500,896 B. fwSeq 4.
- CLDK v3 (`tools/desk/desk_pak.js`): raw DEFLATE per delta with a 512 B window, dedupe, uzlib vendored in `features/clawd/uzlib/`. CRC is identical on 3,331/3,331 frames and the 82 preview PNGs are pixel-identical.

## Surprise
- Fully compressed v3 (1.50 MB) is correct, but inflate costs ~6 us per raw byte: debugger draw p95 went 17.6 -> 24.2 ms and typing boss 19.0 -> 27.6 ms. lateP95 stayed 5 ms.
- uzlib in IRAM was slower (cache pressure), and Z_FIXED was no faster. Both were reverted.

## Decision
- Hybrid, chosen by the user: RAW_ANIMS (building, debugger, typing boss, wizard, happy, juggling) stay stored. The pack is 1.86 MB.
- Measured: fsUsed 2,105,344 / 3,121,152, minHeap 28,168 over a 17 min soak, p95 at v2 levels.

## Next steps
- Phase 2 (deferred): compress clawd.pak (desktop preview.js reads CLWD v1), move strings to PROGMEM, gzip OTA.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
