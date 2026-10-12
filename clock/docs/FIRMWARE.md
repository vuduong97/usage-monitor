# Clawd firmware

The firmware of the Clawd desk clock: the JUZIPi "Smart Weather Clock" / SD Pro
(ESP8266, 240x240 ST7789, WiFi only). Keep the repo private: it embeds
Anthropic's Clawd art and the Styrene/Tiempos fonts. Origins are in `NOTICE`.

Build: `cd firmware && pio run -e clawd` (the clock), `-e clawd_loader` (a tiny
OTA trampoline for stock clocks whose updater is too small, e.g. the "ultra").
Only the ESP8266 builds. `board_esp32*.h` and the `Platform.*` branches are kept
so a port stays possible, but nothing compiles them.

## What it shows

- **Usage:** the compact 240 px layout.
  - "Current" and "Weekly" cards. On Enterprise accounts these become "Spending" and "Period" with pace.
  - The app-fed clock as the title.
  - The whimsical spinner status line.
- **Corner Clawd:** idles, then does acts (waving, dancing, pointing) at a pace set by the usage rate.
- **Alert:** the app's attention message (`m`) scrolls on the status line inside a blinking accent frame, and Clawd alternates waving and pointing. It clears on `x` or after 2 minutes.
- **Idle:** the cloud mini animation with "Listening… / No data…".
- **Splash:** the 17 official Clawd animations, full screen. They play intro → loop → outro, and the walkers cross the stage. They are picked by usage rate or pinned.
  - On the Auto screen the splash means no fresh data, so it captions the clock's IP in a 24 px band at the bottom: the way to reach a clock the desktop app cannot find. Every animation ends by grid row 47 of 60, so the band never covers one. A splash picked on purpose (Screen: Clawd) stays full height.

- **Usage layouts** (`usage.layout`): 0 classic (above); 1 pace: two taller cards with a pace pill (Under/On/Over pace, Just reset), a white time marker on each bar and a forecast line ("Out at ~14:41", "Lasts until reset"); the status line shows only an alert. Enterprise accounts always draw classic. A value this build cannot draw is stored as sent and draws classic, so a later image can pick it up.

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
  | stock (488 KB) → clawd | 552,960 B | 499,760 B |
  | clawd → stock (488,304 B) | 524,288 B | — |
  | clawd → clawd | 524,288 B | 499,760 B |

  **Keep `firmware.bin` ≤ 520 KB**, or in-place self-updates stop fitting. All staging stays below `0x100000`, so no OTA path touches the filesystem.
- What makes the image fit:
  - The animations are a file (`/clawd.pak`, 210 KB), not PROGMEM.
  - There is no TLS and no GitHub self-update (update with `tools/flash.sh` or the System tab).
  - The code carries no ticker, radar, Home Assistant, notify, theme, MQTT or WireGuard.
  - The web UI (`src/webui.html`, one source) and the settings JSON hold only what the clock uses.
- Both firmwares use the same Arduino core: SDK `2.2.2-dev(38a443e)` and LittleFS disk format v2.0. So they read and write the shared filesystem safely.
- The stock firmware keeps its settings in EEPROM, and this firmware never touches EEPROM. This firmware only adds `/config.json`, `/clawd.pak`, `/desk.pak` and transient `*.tmp` files.

## Stock files moved to the Mac (2026-10-12)

`/desk.pak` (2.8 MB, 30 full-screen animations) does not fit next to the stock files (1.5 MB), so they were backed up and deleted from the clock with `tools/stock_fs.sh` (repo root). It talks to auth-gated endpoints this firmware has for that (ESP8266 only):

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

To update this firmware later, use `curl -F "firmware=@.pio/build/clawd/firmware.bin" $H/update`, or the System tab. To replace the pack in place, use `curl -F "file=@data/clawd.pak" $H/api/clawdpak`.

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
- `layouts`: bitmask of the usage layouts this image draws (`CLAWD_LAYOUTS`, bit 0 classic, bit 1 pace); the desktop app offers a layout the clock lacks only when its bundled image has the bit
- `pak` (animations loaded; 0 means `/clawd.pak` is missing), `splash`, `anim`
- `desk` (animations in `/desk.pak`), `onDesk`, `fsTotal`, `fsUsed`
- `contFree`: the least free `loop()` stack seen so far (of 4 KB)
- `crash` (only after one): the last crash since power on, from `custom_crash_callback` in RTC memory (`src/CrashLog.cpp`): count, reason, exccause, epc, excvaddr and the code addresses found on the stack (feed them to `addr2line`). On this core a panic/abort/stack overflow restarts as "Software/System restart", like `ESP.restart()`; with the record, `reset` then reads `Crash (panic/abort/stack)`.
- `deskStats`: over the last 128 desk frames, `drawP50`/`drawP95`/`drawMax` (µs to decode and push one frame) and `lateP95` (µs a frame started after its time)

Measured on the device: `minHeap` stayed at 24,064 B or more through usage, Enterprise and alert payloads. With the desk view (2026-10-12, after a reboot): `minHeap` ≥ 22,568 B; draw p95 11 ms (`typing`) to 45 ms (`building`, the busiest), `lateP95` 5 ms.

```sh
while sleep 60; do curl -s http://clawd-3fa2.local/api/heap; echo; done >> heap.log   # 24 h soak
```

## Code map

| Path | Role |
|---|---|
| `tools/clawd_assets.js` | Builds `data/clawd.pak` (17 animations, 4-bit) and `src/features/clawd/clawd_fonts.h` (subset 4 bpp fonts with kerning, PROGMEM) from the reference Clawd ESP32 firmware's `src` (not in this repo; the generated files are committed). |
| `ClawdPack.*` | Pack reader. LittleFS on the device, stdio in the preview. Loads one frame (≤ 832 B) at a time. |
| `ClawdDraw.*` | No framebuffer. Each region is composed in 4-row bands (1,920 B): background, anti-aliased shapes, then anti-aliased text. One SPI push per band. Pixel-art cells are diffed per grid row. |
| `ClawdAnim.*` | Splash player: player, splash with walkers and rate groups, corner mascot, mini. |
| `ClawdRate.*` | Usage-rate groups (idle, normal, active, heavy). |
| `ClawdUi.*` | Screens, views, alert and status line; the desk player (sequences, pass-end switching, timing stats). |
| `DeskPack.*` | `/desk.pak` (CLDK v2) reader. 1:1 240 px frames as row deltas (skip / literal / run ops, 16 colours per animation), streamed in 128 B reads and pushed as row spans. Format notes in `tools/desk/desk_codec.js`. |
| `tools/desk_check/run.sh` | Host build of `DeskPack.cpp`: decodes every frame of `build/desk/desk.pak` and checks it against the encoder's `desk.crc.json`. |
| `features/usage/UsageMode.cpp` | Glue: Arduino_GFX surface plus ticking. |
| `tools/clawd_preview/run.sh` | Desktop build of the same renderer. Writes PNGs of every screen and animation to `.pio/clawd_preview/`. |
| `tools/gzip_webui.py` | Writes `src/webui.h` (gzip) from `src/webui.html`, the single web UI source. |
| `tests/host/run.sh` | Host test: applies a config with dead keys through `Settings.cpp` and the save/load path, and compares with `fixtures/expected.json`. |

Build (`pio run -e clawd`): static RAM 47,864 B (58.4%), image 499,760 B. That leaves about 24 KB of the 524,288 B clawd → clawd OTA budget: check the size before adding features (`tools/flash.sh` refuses larger images).
