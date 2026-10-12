---
title: send usage again as soon as the clock restarts
date: 2026-10-12
summary: the boot animation is the "no data" splash, which stayed up until the next usage poll (up to 60 s). clawd-desktop now watches /api/heap uptime every 5 s and resends the cached usage payload plus the state at once. Verified against a fake clock (usage 4.1 s, state 4.4 s after it answers again, 0 extra API polls); not yet on the real clock (office WiFi blocks it)
---

# Resend usage after a clock restart

## Why the clock shows an animation at boot
- In auto mode (`screen = 0`), `clawdUiService` in `ClawdUi.cpp` uses `want = fresh ? VIEW_USAGE : VIEW_SPLASH`.
- Right after boot nothing has arrived yet (`g_received = false`), so the clock shows the splash.
- The splash plays the idle group from `clawd.pak` (magnifier, walking, pointing, lurking) and rotates every 20 s.
- The splash ended only at the next scheduled usage push. The poll interval is 60 s, and nothing on the computer knew that the clock had restarted.

## Decisions (ak:advise, then ak:cook --auto)
- The user wants the splash to be shorter. They chose a computer-side fix with fresh numbers only, over the firmware caching old numbers.
- Probe every 5 s.
- Scope moved from the clawd-home LaunchAgent daemon to **clawd-desktop**. The daemon was booted out this morning, and `/Applications/Clawd.app` now pushes to the clock. The clawd-home daemon is unchanged.

## clawd-desktop (08367fc, 78c5d15)
- New `bootwatch.py` (`BootWatch`):
  - It reads `GET /api/heap` every 5 s, with a 12 s timeout because HTTP is slow after boot.
  - It estimates the boot time as now − uptime.
  - Restart rule: the estimate moved more than 30 s, or the clock is new (app start or a new DHCP address) and has been up less than 120 s.
  - If the clock does not take the data, the next probe tries again, but only while the clock has been up less than 120 s. It logs once per restart.
- `Publisher.last_payload` keeps the last delivered payload, without `b`/`m`/`x`.
- `Publisher.resend(url)` pushes that payload again:
  - It refreshes `t` and uses a 15 s timeout.
  - It never calls `poll_active`.
  - With nothing cached it sets `force` instead (one poll).
- `HttpSinks.send` takes a `timeout` argument.
- `engine.py` wires it together: on a restart it sets `pusher.last_key = None`, wakes the brain, then calls `publisher.resend`.
- Tests: 11 new, 240 in total pass. `check_public` is ok.
- E2E against a fake clock: the real `Engine` and an `http.server`, with a reboot simulated by 8 s down and then uptime 0.
  - Usage arrived 4.1 s and state 4.4 s after the clock answered again.
  - `poll_active` was awaited 1 time in total.
- PROTOCOL.md in both repos has a note on the 5 s `/api/heap` watch (clawd-home 9923069).

## Not verified
- Behaviour on the real clock. It is on the office WiFi (10.97.44.122), where nothing on the LAN can reach it (see the replug journal).
- An `/api/reboot` sent to the old 192.168.1.48 reached nothing.
- Still to measure at home or on a hotspot:
  - Reboot the clock and watch `/api/heap` until `splash:false`. Target: ≤ 10 s after the clock's first HTTP reply.
  - Check that the slow HTTP after boot (8–10 s per request) stays under the 12 s probe and 15 s push timeouts.
- Whether 12 GETs per minute make the slow window after boot worse.
