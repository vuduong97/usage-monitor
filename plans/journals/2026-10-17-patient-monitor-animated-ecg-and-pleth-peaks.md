---
title: "Patient monitor: animated ECG and pleth peaks"
date: 2026-10-17
summary: "monitor pack's trace peaks rise and fall at three heights via shared vis tracks (038c921)"
---

# Patient monitor: animated ECG and pleth peaks

## What happened
The user asked for up/down motion on the Patient monitor style pack. Chosen option: the ECG and WK pleth traces pulse (not a Clawd bob, since the clock replaces Clawd's masks with the official animations).

## Change
- `clock/firmware/tools/style/packs/monitor.js`: `liveWave()` draws the segments touching the moving points (ECG R tip; pleth pulse + notch) at 3 heights. Each height is a `vis` track of 250 ms steps (tall, mid, low, mid), shared by both traces. Each beat's `phase` is a step behind the previous one, so the peaks roll right. Still segments are merged into flat runs as before.
- Rebuilt `styles/monitor.pak` (14248 -> 15616 B) and `styles/index.json`.

## Evidence
- build.js is deterministic (all other packs byte-identical).
- Host preview: `styles: ok`, `style anims: ok`, paint ~clock 248 ms (was 212), anim host avg 10 us (was 6).
- clawd_render --ms frames show the rolling peaks; --clip: 41 frames / 6 s loop, largest dirty region 15.7k px.
- publish_packs.py stages 46 packs.

## Risks / next steps
- 59 of the firmware's 64 animated ops (STYLE_MAX_ANIMATED) are used; the compiler does not check this cap.
- Not measured on the real clock (per-step frame time).
- Commit is local on feat/monorepo, not pushed.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
