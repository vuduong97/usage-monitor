---
title: "Backlight flicker at 15 %: PWM 25 kHz (fwSeq 9)"
date: 2026-10-18
summary: "desk 'anim flicker' was the 1 kHz backlight PWM at the 15 % quiet-hours cap; 25 kHz reduces it (472da3d), sigma-delta hung the clock"
---

# Backlight flicker at 15 %: PWM 25 kHz (fwSeq 9)

## What happened
The user reported Clawd flickering on the desk view at night. A three-phase test on clock 192.168.1.48 (anim @100 %, still @15 %, anim @15 %) showed the flicker came only from the low brightness, not the animation. The app caps brightness at 15 % in quiet hours (23:00-07:00, `quiet_dim`), and the ESP8266 drove the backlight with `analogWrite` at 1 kHz. The owner wants 15 % kept, so raising `quiet_dim` was ruled out.

## Change
- `Platform.h`: `platformBacklightInit/Write` replace `platformAnalogWriteInit` + the bare `analogWrite` in `Gfx.cpp`. The ESP8266 sets `analogWriteFreq(25000)`; ESP32 is unchanged.
- `CLAWD_FW_SEQ` 9; `clock/docs/FIRMWARE.md` gains a Backlight section.

## Evidence
- Sweep at 15 % through a temporary `/api/debug/pwm` endpoint (removed): 200-8000 Hz flicker, 15/25/40 kHz no visible flicker in 11 s glimpses.
- Pinned 25 kHz: the owner still sees a faint flicker during desk animations, lighter than before.
- Cost (typing desk anim): draw p50 10.9 ms @1 kHz, 12.4 @15k, 13.2 @20k, 13.3 @25k; lateP95 unchanged.
- 5-minute soak, dim 0/15/50/100 %: 60/60 polls answered, no reset, heap ~26 KB.

## Rejected
- Hardware sigma-delta on GPIO5 (no CPU jitter in theory): the probe build hit a hardware watchdog reset, then an exception (epc in SDK code); a clean build without any `analogWrite` still hung the clock about 3 min after boot (animation frozen, off the network). Documented in FIRMWARE.md as not to use.

## Risks / next steps
- The faint flicker at 15 % remains. Untried: the core's phase-locked waveform engine (`enablePhaseLockedWaveform()`, link-time switch), or darkening pixels via the colour gains while the backlight stays higher.
- Once, right after an OTA, the probe firmware stopped answering HTTP while its loop ran (`/style.log` alive lines, `web=` growing). Not reproduced; may be the known network drop.
- The Clawd.app on this Mac was not rebuilt (other uncommitted work sits in the tree); `build/firmware` holds fwSeq 9 for the next build, and CI builds the firmware into every release.
