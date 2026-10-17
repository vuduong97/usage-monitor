---
title: "Waiting screen with IP before first data, even with a style pack"
date: 2026-10-13
---

# Waiting screen with IP before first data, even with a style pack

## What changed
Before the first payload since boot, the usage screen (screen = Usage) used to hand over to an installed style pack, which drew its own no-data screen and gave no IP. Now `clawdUiService` only lets the pack take over once `g_received` is true. Until then the classic idle waiting screen shows: corner Clawd, the cloud, the clock's IP (styrene_14, dim, `IDLE_ADDR_Y = STATUS_Y - 24`) and "Waiting…". The IP line clears for good after the first ok payload, and stale data afterwards still goes to the pack's own stale screen (user's choice). The Auto screen is unchanged: it shows the splash with the IP band.

## Verification
- `tools/clawd_preview/run.sh` adds the new `checkWaiting`, which covers: IP drawn, IP redrawn when it changes, no pack before data, pack after data, pack still on when stale, IP gone after first data. All checks pass, including styles, address band and variants.
- `pio run -e clawd` builds a 499,808 B image. The host settings test passes.

## Limits
- Not verified on the clock: it was unreachable from the office network.
- An `ok:false` payload alone, where the app is connected but has no usage yet, also keeps the waiting screen.

> Historical work record — not durable authority. Prefer docs/specs/ADRs for current decisions.
