# Clawd firmware

The firmware of the Clawd desk clock: the JUZIPi "Smart Weather Clock" / SD Pro
(ESP8266, 240x240 ST7789, WiFi only). Keep the repo private: it embeds
Anthropic's Clawd art and the Styrene/Tiempos fonts.

Build: `cd firmware && pio run -e clawd` (the clock), `-e clawd_loader` (a tiny
OTA trampoline for stock clocks whose updater is too small, e.g. the "ultra").
Only the ESP8266 builds. `board_esp32*.h` and the `Platform.*` branches are kept
so a port stays possible, but nothing compiles them.

## What it shows

- **Usage:** the compact 240 px layout.
  - "Current" and "Weekly" cards. On Enterprise accounts these become "Spending" and "Period" with pace.
  - The app-fed clock as the title.
  - The whimsical spinner status line.
- **Corner Clawd:** rests, then does acts (waving, dancing, pointing) at a pace set by the usage rate: 4 s of rest between acts when idle, 3 / 2 / 1.5 s as the rate goes up (`MAS_STILL_MS`).
- **Alert:** the app's attention message (`m`) scrolls on the status line inside a blinking accent frame, and Clawd alternates waving and pointing. It clears on `x` or after 2 minutes.
- **Idle:** the cloud mini animation with "Listening… / No data…". Until the first payload since boot it says "Waiting…" and shows the clock's IP above the status line, so a clock the desktop app has not reached yet can still be found.
- **Login expired:** `{"ok": false, "why": "login"}` (fwSeq 7) shows the idle view with "Login expired" on the status line, over a style pack and over the splash, until usage comes back. The title clock keeps running from the payload's `t`.
- **Splash:** the 17 official Clawd animations, full screen. They play intro → loop → outro, and the walkers cross the stage. They are picked by usage rate or pinned.
  - On the Auto screen the splash means no fresh data, so it captions the clock's IP in a 24 px band at the bottom: the way to reach a clock the desktop app cannot find. Every animation ends by grid row 47 of 60, so the band never covers one. A splash picked on purpose (Screen: Clawd) stays full height.

- **Style packs:** the usage screen can instead be drawn from a style pack installed by the desktop app (`POST /api/style`), switched at once without a reboot: Game Boy, Synthwave, Newspaper, Blueprint, Pastel, 1-bit. Alerts and Enterprise accounts still use the classic screen, and so does the time before the first payload since boot (the Idle waiting screen with the IP); after that the pack draws live and stale data. Format, storage and endpoints: `docs/STYLE_PACK.md`. The old usage layouts (pace, hero, rings, gauges, pixel) were removed; `usage.layout` is kept in the config but ignored.

The web UI's Clawd tab has two settings:
- **Screen:** Auto (usage while data is live, splash otherwise), Usage, or Clawd.
- **Animation:** by usage rate, or a fixed one.

## Flash layout: why it is 4m3m

The stock firmware keeps its web UI, weather icons, flip-clock digits, fonts and built-in GIFs in a 3 MB LittleFS at `0x100000` (the `4m3m` layout):
- None of it is published (the JUZIPi repo only has the `.bin` files).
- The stock web server cannot download it, apart from `/`, `javascript.js` and `style.css`.

The usual `4m1m` layout overlaps that region and would destroy it. So `clawd` keeps the **stock layout**:

- `board_build.ldscript = eagle.flash.4m3m.ld`, the same as stock.
- The filesystem is **never formatted**: `LittleFSConfig::setAutoFormat(false)`, and there is no format fallback in `settingsBegin()`.
- The image must fit the stock OTA budget. OTA staging uses what is left of the 1 MB sketch area (1,048,576 B minus the running image):

  | OTA path | Max new image | This image |
  |---|---|---|
  | stock (488 KB) → clawd | 552,960 B | 500,896 B |
  | clawd → stock (488,304 B) | 524,288 B | — |
  | clawd → clawd | 524,288 B | 500,896 B |

  **Keep `firmware.bin` ≤ 520 KB**, or in-place self-updates stop fitting. All staging stays below `0x100000`, so no OTA path touches the filesystem.
- What makes the image fit:
  - The animations are a file (`/clawd.pak`, 210 KB), not PROGMEM.
  - There is no TLS, no HTTP client and no GitHub self-update (update with `tools/flash.sh` or the System tab). Usage only arrives by push (`POST /api/usage`); the old pull mode (Usage URL, poll interval) is gone, it blocked `loop()` for seconds.
  - The code carries no ticker, radar, Home Assistant, notify, theme, MQTT or WireGuard.
  - The web UI (`src/webui.html`, one source) and the settings JSON hold only what the clock uses.
- Both firmwares use the same Arduino core: SDK `2.2.2-dev(38a443e)` and LittleFS disk format v2.0. So they read and write the shared filesystem safely.
- The stock firmware keeps its settings in EEPROM, and this firmware never touches EEPROM. This firmware only adds `/config.json`, `/clawd.pak`, `/desk.pak` and transient `*.tmp` files.

## Stock files moved to the Mac (2026-10-12)

`/desk.pak` (2.8 MB as CLDK v2, 31 full-screen animations) did not fit next to the stock files (1.5 MB), so they were backed up and deleted from the clock with `tools/stock_fs.sh` (repo root). It talks to auth-gated endpoints this firmware has for that (ESP8266 only):

| Endpoint | |
|---|---|
| `GET /api/fs/list` | `size<TAB>path` per file |
| `GET /api/fs/get?path=` | one file |
| `GET /api/fs/raw` | the whole LittleFS partition, raw |
| `POST /api/fs/put?path=` | multipart upload; directories are created |
| `POST /api/fs/rm?path=` | delete; empty directories go too |

```sh
tools/stock_fs.sh backup            # -> .stock-backup/fs-<stamp>/ (files, list.tsv, sha256.tsv, littlefs.bin)
tools/stock_fs.sh verify <dir>      # unpacks littlefs.bin with mklittlefs, compares; writes VERIFIED
tools/stock_fs.sh delete <dir>      # only after VERIFIED; keeps config.json, clawd.pak, desk.pak
tools/stock_fs.sh restore <dir>     # deletes desk.pak, puts the stock files back
tools/stock_fs.sh check <dir>       # what is on the clock vs the backup
```

`/api/fs/get` and `/api/fs/raw` can read `/config.json` (WiFi password): keep a web password set (`CLAWD_AUTH=user:pass` for the script).

The backup in use is `.stock-backup/fs-20261012-000754/` (72 files, git-ignored: `config.json` holds the WiFi password). **Run `restore` before going back to stock**, or the stock firmware boots without its web UI, icons and fonts.

## Install (from stock)

`tools/provision.sh <clock-ip>` does all of this and more (see README). By hand:

```sh
H=http://<clock-ip>
pio run -e clawd
# 1. the animation pack, through the STOCK firmware's file upload (root dir)
curl -F "file=@data/clawd.pak;filename=clawd.pak" $H/upload
# 2. a /config.json so this firmware joins WiFi on first boot (else it opens an AP)
#    {"hostname":"clawd","wifi":[{"ssid":"…","pass":"…"}]}
curl -F "file=@config.json;filename=config.json" $H/upload
# 3. the image; the stock OTA rejects filenames not starting with "SDP"
curl -F "update=@.pio/build/clawd/firmware.bin;filename=SDP_clawd.bin" $H/update_ota
```

To update this firmware later, use `curl -F "firmware=@.pio/build/clawd/firmware.bin" $H/update`, or the System tab. For other people's clocks, publish a release to the firmware feed (`tools/release.sh`, `feed/README.md`); Clawd desktop then offers it. To replace the pack in place, use `curl -F "file=@data/clawd.pak" $H/api/clawdpak`.

## Back to stock (verified 2026-10-11)

```sh
tools/stock_fs.sh restore .stock-backup/fs-20261012-000754   # the stock files first
curl -F "firmware=@SDPro_V1.0.6_20260525_174828.bin" http://<clock-ip>/update
```

Get the image from https://github.com/JUZIPi-tech/SD_PRO. A local copy is in `.stock-backup/` (git-ignored, because it holds secrets).

The round trip was tested on the device: clawd → stock → clawd.
- After the revert, the stock `index.html`, `javascript.js` and `style.css` were byte-identical to the backup.
- The photo/GIF list and the stock EEPROM settings were unchanged.
- The only difference: free space was 216 KB lower, from `clawd.pak` and `config.json`. Both can be deleted later and are harmless to stock.

## Data

The Clawd desktop app finds the clock over mDNS (`_clawd._tcp`, TXT `id`, `ver`, `path`) or a /24 scan and pushes over HTTP; the wire format is `PROTOCOL.md`. To pin a clock, add `http_targets = clawd-3fa2.local` (or the IP) to `~/.config/claude-usage-monitor/config`.

`POST /api/usage` accepts the payload `s sr w wr st ok acct tp rd t tf m x` and is unauthenticated, because the app cannot send a password and the endpoint only changes what the screen shows:
- `ok:false` is accepted (200). The screen goes idle (Auto: splash) once the data is stale.
- `c` and `b` (chime) are ignored.

## Heap watch

`GET /api/heap` needs no auth and is read only. It returns these fields:
- `heap`, `maxblk`, `minHeap`, `minMaxblk`, `frag`
- `uptime`, `reset`, `version`, `variant` (`esp8266-clawd`, which the app matches on)
- `fwSeq` (`CLAWD_FW_SEQ` in `config.h`): bump it by hand for every image the desktop app should install; the app only pushes its bundled image to a clawd clock with a lower number (missing = 0), so it never downgrades
- `layouts`: bitmask of the compiled-in usage layouts (`CLAWD_LAYOUTS`); only classic (1) since style packs replaced the others
- `styleApi`, `style`, `onStyle`, `styleFrameUs`, `styleFrameMaxUs`, `stylePaintUs`, `stylePaintMaxUs`: the style pack API this image draws, the pack installed (`""` = classic), whether it is on screen now, and its animation and full-screen paint times (`docs/STYLE_PACK.md`)
- `loopMaxMs`: the longest `loop()` pass over the last 1–2 minutes; `loopPartsMs` splits it into `[net, web, screen]`. A style install shows ~1–2 s in web: the upload, the slot copy and the LittleFS rename all run inside the request handler
- `pak` (animations loaded; 0 means `/clawd.pak` is missing), `splash`, `anim`
- `desk` (animations in `/desk.pak`), `onDesk`, `fsTotal`, `fsUsed`
- `pakCrc`, `deskCrc`: the CRC-32 of `/clawd.pak` and `/desk.pak`, or `""` while unknown (`src/PakCrc.cpp`).
  - Each is kept in a sidecar `<pack>.crc` (`"<hex> <size>"`) so it survives a reboot.
  - It is set by the pack upload handlers while they stream.
  - It is dropped by `fs/put` and `fs/rm` on the pack.
  - It is recomputed in 3 ms slices from `loop()` starting 30 s after boot whenever the sidecar is missing or its size is stale. `desk.pak` takes about a minute.
  - The firmware feed (`feed/README.md`) uses these to send only the packs that changed.
- `contFree`: the least free `loop()` stack seen so far (of 4 KB)
- `crash` (only after one): the last crash since power on, from `custom_crash_callback` in RTC memory (`src/CrashLog.cpp`): count, reason, exccause, epc, excvaddr and the code addresses found on the stack (feed them to `addr2line`). On this core a panic/abort/stack overflow restarts as "Software/System restart", like `ESP.restart()`; with the record, `reset` then reads `Crash (panic/abort/stack)`.
- `deskStats`: over the last 128 desk frames, `drawP50`/`drawP95`/`drawMax` (µs to decode and push one frame) and `lateP95` (µs a frame started after its time)

Measured on the device: `minHeap` stayed at 24,064 B or more through usage, Enterprise and alert payloads. With the desk view (2026-10-12, after a reboot): `minHeap` ≥ 22,568 B; draw p95 11 ms (`typing`) to 45 ms (`building`, the busiest), `lateP95` 5 ms.

After the YCbCr trim, the pull-mode removal and the v3 desk pack (2026-10-12, 17 min soak with app pushes): `heap` ≈ 30,000 B, `minHeap` 28,168 B (was 21,480), `fsUsed` 2,105,344 of 3,121,152 B (was 3,096,576). Draw p95 unchanged from v2 within noise: `debugger` 17.5 ms, `typing boss` 19.5, `juggling` 16.1, `wizard` 26.7, `happy` 31.0, `building` 44 ms; `lateP95` 4–5 ms. Compressing those six too saves another 360 KB but costs ~6 µs per raw byte (`debugger` p95 24 ms, `typing boss` 28 ms), so `RAW_ANIMS` in `tools/desk/desk_pak.js` keeps them stored. IRAM for the inflater measured slower (cache pressure) and was dropped.

```sh
while sleep 60; do curl -s http://clawd-3fa2.local/api/heap; echo; done >> heap.log   # 24 h soak
```

## Backlight (fwSeq 9, 2026-10-18)

The backlight (GPIO5, active low) is driven by `analogWrite` at 25 kHz (`platformBacklightInit` in `src/Platform.h`), not the core's 1 kHz. At the app's quiet-hours cap of 15 %, 1–8 kHz flickers visibly with or without an animation; 15–40 kHz mostly does not. A faint flicker remains at 15 % during desk animations: `analogWrite` is a timer-interrupt PWM, and WiFi and flash reads delay the interrupt.

- Cost at 25 kHz on the `typing` desk animation: draw p50 13.3 ms (was 10.9 at 1 kHz), no late frames added. 5-minute soak at 0/15/50/100 %: no reset, heap steady around 26 KB.
- The hardware sigma-delta generator (`sigmaDeltaAttachPin`) was tried on this pin and hung the clock within minutes (animation frozen, off the network, once a hardware watchdog reset). Don't use it here.

## Code map

| Path | Role |
|---|---|
| `tools/clawd_assets.js` | Builds `data/clawd.pak` (17 animations, 4-bit) and `src/features/clawd/clawd_fonts.h` (subset 4 bpp fonts with kerning, PROGMEM) from the reference Clawd ESP32 firmware's `src` (not in this repo; the generated files are committed). |
| `ClawdPack.*` | Pack reader. LittleFS on the device, stdio in the preview. Loads one frame (≤ 832 B) at a time. |
| `ClawdDraw.*` | No framebuffer. Each region is composed in 4-row bands (1,920 B): background, anti-aliased shapes, then anti-aliased text. One SPI push per band. Pixel-art cells are diffed per grid row. |
| `ClawdAnim.*` | Splash player: player, splash with walkers and rate groups, corner mascot, mini. |
| `ClawdRate.*` | Usage-rate groups (idle, normal, active, heavy). |
| `ClawdUi.*` | Screens, views, alert and status line; the desk player (sequences, pass-end switching, timing stats). |
| `StylePack.*` | Style pack interpreter: validates a pack, reads it in place through the flash cache, paints it and repaints only what its animation tracks and its Clawd animation (frames from `/clawd.pak`, recoloured) change (`docs/STYLE_PACK.md`). |
| `StyleStore.*` | `/style.pak` on LittleFS and its mirror in the free sketch flash after the image (device only); re-mirrored at boot after an OTA. |
| `tools/style/` | Style pack compiler (`build.js`, `style_pack.js`) and the six pack sources (`packs/*.js`); writes `styles/*.pak` + `index.json`. |
| `DeskPack.*` | `/desk.pak` reader, CLDK v2 and v3. 1:1 240 px frames as row deltas (skip / literal / run ops, 16 colours per animation), streamed in 128 B reads and pushed as row spans. v3 stores each delta as raw DEFLATE (512 B window), inflated through static uzlib state and a 512 B ring (`uzlib/`, vendored from the Arduino core, zlib licence); identical deltas are stored once. Ops in `tools/desk/desk_codec.js`, container in `tools/desk/desk_pak.js`. |
| `tools/desk_check/run.sh [pak]` | Host build of `DeskPack.cpp`: decodes every frame of `build/desk/desk.pak` (v2 or v3) and checks it against the encoder's `desk.crc.json`. |
| `gfx_trim.py`, `gfx_check.py` | Pre/post build scripts. Arduino_GFX 1.6.9 (pinned) links `writeYCbCrPixels` through the vtable, and with it ~6.8 KB of YCbCr tables in DRAM that nothing here uses. `gfx_trim.py` compiles that body out under `CLAWD_NO_YCBCR`; `gfx_check.py` fails the build if the tables are back in the ELF (for example after a library bump). |
| `features/usage/UsageMode.cpp` | Glue: Arduino_GFX surface plus ticking. |
| `tools/clawd_preview/run.sh` | Desktop build of the same renderer. Writes PNGs of every screen and animation to `.pio/clawd_preview/`. |
| `tools/gzip_webui.py` | Writes `src/webui.h` (gzip) from `src/webui.html`, the single web UI source. |
| `tests/host/run.sh` | Host test: applies a config with dead keys through `Settings.cpp` and the save/load path, and compares with `fixtures/expected.json`. |

Build (`pio run -e clawd`, fwSeq 9, backlight PWM 25 kHz): 504,784 B image, RAM 46,028 B. Before (fwSeq 8, style API 2: the packs play the official Clawd animations from `/clawd.pak`): 504,736 B image, RAM 46,028 B (the player keeps one 832 B frame). Before (fwSeq 7, this machine's toolchain): 502,240 B image, RAM 45,032 B. Before (usage `src`, fwSeq 7): 501,984 B image, RAM 44,988 B. Before (pack CRCs, fwSeq 6): 501,520 B image. Before (style packs): 499,664 B image (OTA budget 524,288 B), RAM 44,352 B of 81,920 (54.1%).
