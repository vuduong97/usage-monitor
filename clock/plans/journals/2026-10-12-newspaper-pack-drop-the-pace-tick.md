---
title: "Newspaper pack: drop the pace tick"
date: 2026-10-12
summary: "Removed the orange pace marker from the Newspaper 5-hour bar; pack-only change, installed on the clock"
---

# Newspaper pack: drop the pace tick

## What happened
The user read the orange tick on the Newspaper pack's 5-hour bar (rect 9,130,2,12, dx on `ts`) as a quota warning and asked to remove it. Interviewed down from "warning levels on all usage screens" to exactly this one op.

## Changes
- `firmware/tools/style/packs/paper.js`: one `S.rect` removed. Bar, red >= 80 % number/bar, forecast sentence, weekly bar, grey rule kept.
- `firmware/styles/paper.pak` 8628 -> 8592 B, crc 5fd9d93e -> a7419e64; the other five packs are byte-identical. Commit 5104def on feat/style-packs.
- clawd-desktop bundle and renderer rebuilt from 5104def (git-ignored, fwSeq 5, same firmware.bin sha256).

## Verification
- Host preview `styles: ok`; before/after screenshots show only the tick gone. `variants: FAILED` is pre-existing (no desk.pak at ../build/desk), same on baseline.
- clawd-desktop `CLAWD_GOLDEN=1 pytest`: 360 passed.
- Clock 192.168.1.48: POST /api/style -> crc a7419e64, onStyle true after /api/state {"a":""}, paint 209 ms, minHeap 21392, no reboot.

## Decision
Pack-only change: no CLAWD_FW_SEQ bump, no flash. Clocks with the old paper.pak keep it until the style is saved again from the app.

## Next steps
The running soak (until ~03:09 install phase) now rotates the new paper.pak; the user chose not to wait for it.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
