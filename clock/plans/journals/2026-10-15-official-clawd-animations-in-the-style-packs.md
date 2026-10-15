---
title: "Official Clawd animations in the style packs (style API 2, fwSeq 8)"
date: 2026-10-15
---

# Official Clawd animations in the style packs (style API 2, fwSeq 8)

## What changed
With data, every one of the 46 usage-screen style packs now plays the official Clawd animations from `/clawd.pak`, the 17 the splash uses (ported from Clawdmeter's `research/clawd-official`). Each pack uses its own colours and its own set. Clawdmeter's `assets/`, the folder the request named, only holds fonts and battery icons.

- **Firmware (`StylePack.cpp`):**
  - New op `K_ANIM = 6` and condition bit `anim` (10), set when every name in the pack resolves in the pak. It is rechecked whenever `clawdPackGeneration()` changes.
  - One player plays the first anim op shown: it stands on frame 0 for the op's idle time, plays one pass, then moves to the next name.
  - Frames come one at a time into an 832 B buffer and are drawn with role colours. `roleOf` maps the official palette by RGB to body, shade, ink, grey, light and accent.
  - `styleAnimate` repaints the frame boxes, old and new.
  - `STYLE_API` is 2, but API 1 packs still draw (`STYLE_API_MIN`), so the pack installed before the OTA stays up.
  - `/api/heap` gains `styleAnim`, and `CLAWD_FW_SEQ` is 8.
- **Compiler:**
  - `Style.animateClawd` turns every with-data group of Clawd masks into one anim op (`ok anim`). The masks stay behind as the `!anim` fallback, and the no-data Clawd is left alone.
  - Per-pack config lives in `tools/style/anims.js`: set, cell, `at` nudge and colours.
  - The build checks the names against `data/clawd.pak`, allows at most 8 anim ops, and refuses two that could show at once.
- **Preview (`checkStyleAnims`):** per pack it checks:
  - 2 or more animations play in 45 s;
  - hot and fresh animate;
  - without data, Clawd is still and drawn;
  - without `/clawd.pak`, the static Clawd comes back where the animation played (both compared against the pack with its Clawd ops hidden);
  - Clawd resumes when the pak returns.

  It also writes footprint shots (`40-<id>-footprint`), which were used to move Clawd off text (racinghud, weather, scope).

## Decisions
- **Firmware op instead of 1-bit flipbooks.** Flipbooks would have broken the 20 KB pack cap and lost the official timing. The op costs about 36 B per Clawd in a pack and about 2.3 KB of image.
- **Colours by role, not palette index.** Palette order differs between animations. Shade is darkened only for Clawd orange; in other packs it stays flat, so monochrome packs keep two tones.
- **Outlined Clawds** (hifivfd, mac84, pfd, blueprint): the body colour is the fill, unless the fill is close to the background; then the outline colour is used.
- **API 1 acceptance** came from the code review. Without it, the OTA would have dropped every installed pack to the classic screen.

## Verification
- `pio run -e clawd`: 504,720 B image (+2,480 B) and 46,028 B RAM (+996 B). The fwSeq 7 baseline on this machine was 502,240 B and 45,032 B.
- All 46 packs fit: the largest is tufte at 18,892 B. A rebuild is byte-identical.
- The full `run.sh` passes everything: `styles: ok` and `style anims: ok (46 packs)`.
- An independent test pass ran:
  - K_ANIM field fuzzing: every malformed op refused, 9 ops refused;
  - byte fuzzing: everything was either refused or drew within the screen;
  - bad name lists fall back to the mask;
  - a 600 s soak across states on all 46 packs: zero off-screen pushes.
- Harness: the HEAD API-1 pack validates, API 2 relabelled as API 1 is refused (`op anim`), and API 3 is refused.
- Review fixes:
  - API 1 is accepted again;
  - the player retries after a failed read and falls back when a descriptor is unusable;
  - `clawdPackAnim` requires `w*h ≤ 2·frameBytes`;
  - the erase box is the one actually drawn, so a Clawd moved by usage leaves no 1 px strip;
  - `idle` above 32767 ms is written correctly;
  - unreachable `!ok` copies are dropped (36–72 B per pack).

## On the clock (10.97.40.207, same day)
- OTA from fwSeq 7 with the API 1 `monitor` pack installed: after the reboot, `styleApi` was 2 and the old pack was still active (same CRC). The API 1 fallback works on the device.
- Slowest animation repaint (`styleFrameMaxUs`): pfd 24 ms, weather 27 ms, monitor 32 ms, racinghud 66 ms (racing car), all under 100 ms. Full paint 185–328 ms. `minHeap` 23,208–24,304 B, in line with earlier style-pack figures (≥ 23,040 B).
- Every pack played its own set (monitor waving/dancing, weather cloud/waving, racinghud racing car/crab/walking), in normal and hot.
- Found and fixed: `/api/heap` `styleAnim` kept the last name after the clock left the pack (Auto screen with stale data goes to the splash). It now reports only while the pack is on screen. Image 504,736 B.

## Limits
- No 24 h soak yet. The `minHeap` target of 24,000 B in the plan was set too tight; ~23 KB is what style packs ran at before this change too.
- The no-data (sleeping) Clawd on a pack shows only with screen = Usage; this clock is on Auto, which shows the splash instead. The host preview covers it.
- The desktop app (clawd-desktop, on the Mac) must ship these API 2 packs and only send them to clocks reporting `styleApi` 2. Its copy of `tools/style` must take `anims.js` and the `clawd.pak` dependency.
- `run.sh` on a Windows host leaves `.ppm` files unconverted, because the preview prints CRLF and the `*.ppm` case misses. This happened before this change and only affects screenshots.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
