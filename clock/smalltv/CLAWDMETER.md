# smalltv-clawdmeter

Private fork of [giovi321/smalltv-mod](https://github.com/giovi321/smalltv-mod)
(upstream `9201e86`, branch `clawdmeter`) that runs Clawdmeter's screens on the
JUZIPi "Smart Weather Clock" / SD Pro (ESP8266, 240x240 ST7789, WiFi only,
GeekMagic SmallTV class). Keep it private: it embeds Anthropic's Clawd art and
the Styrene/Tiempos fonts.

## What it shows

- **Usage:** Clawdmeter's compact 240 px layout.
  - "Current" and "Weekly" cards. On Enterprise accounts these become "Spending" and "Period" with pace.
  - The daemon-fed clock as the title.
  - The whimsical spinner status line.
- **Corner Clawd:** idles, then does acts (waving, dancing, pointing) at a pace set by the usage rate.
- **Alert:** the daemon's attention message (`m`) scrolls on the status line inside a blinking accent frame, and Clawd alternates waving and pointing. It clears on `x` or after 2 minutes.
- **Idle:** the cloud mini animation with "Listening… / No data…".
- **Splash:** the 17 official Clawd animations, full screen. They play intro → loop → outro, and the walkers cross the stage. They are picked by usage rate or pinned.

The web UI's Usage tab, Clawdmeter card, has two settings:
- **Screen:** Auto (usage while data is live, splash otherwise), Usage, or Clawd.
- **Animation:** by usage rate, or a fixed one.

## Flash layout: why it is 4m3m

The stock firmware keeps its web UI, weather icons, flip-clock digits, fonts and built-in GIFs in a 3 MB LittleFS at `0x100000` (the `4m3m` layout):
- None of it is published (the JUZIPi repo only has the `.bin` files).
- The stock web server cannot download it, apart from `/`, `javascript.js` and `style.css`.

smalltv-mod's usual `4m1m` layout overlaps that region and would destroy it. So `smalltv_clawd` keeps the **stock layout**:

- `board_build.ldscript = eagle.flash.4m3m.ld`, the same as stock.
- The filesystem is **never formatted**: `LittleFSConfig::setAutoFormat(false)`, and there is no format fallback in `settingsBegin()`.
- The image must fit the stock OTA budget. OTA staging uses what is left of the 1 MB sketch area (1,048,576 B minus the running image):

  | OTA path | Max new image | This image |
  |---|---|---|
  | stock (488 KB) → clawd | 552,960 B | 517,904 B |
  | clawd → stock (488,304 B) | 524,288 B | — |
  | clawd → clawd | 524,288 B | 517,904 B |

  **Keep `firmware.bin` ≤ 520 KB**, or in-place self-updates stop fitting. All staging stays below `0x100000`, so no OTA path touches the filesystem.
- What makes the image fit:
  - The animations are a file (`/clawd.pak`, 210 KB), not PROGMEM.
  - There is no TLS and no GitHub self-update.
  - The ticker, radar, HA, notify and theme features are compiled out.
  - The web UI and settings JSON leave out those features' tabs and slices.
- Both firmwares use the same Arduino core: SDK `2.2.2-dev(38a443e)` and LittleFS disk format v2.0. So they read and write the shared filesystem safely.
- The stock firmware keeps its settings in EEPROM, and this firmware never touches EEPROM. This firmware only adds `/config.json`, `/clawd.pak` and transient `*.tmp` files.

## Install (from stock) — done 2026-10-11, device 192.168.1.48 / clawdmeter.local

```sh
H=http://<clock-ip>
pio run -e smalltv_clawd
# 1. the animation pack, through the STOCK firmware's file upload (root dir)
curl -F "file=@data/clawd.pak;filename=clawd.pak" $H/upload
# 2. a /config.json so this firmware joins WiFi on first boot (else it opens an AP)
#    {"hostname":"clawdmeter","wifi":[{"ssid":"…","pass":"…"}],"mode":"usage"}
curl -F "file=@config.json;filename=config.json" $H/upload
# 3. the image; the stock OTA rejects filenames not starting with "SDP"
curl -F "update=@.pio/build/smalltv_clawd/firmware.bin;filename=SDP_clawdmeter.bin" $H/update_ota
```

To update this firmware later, use `curl -F "firmware=@.pio/build/smalltv_clawd/firmware.bin" $H/update`, or the System tab. To replace the pack in place, use `curl -F "file=@data/clawd.pak" $H/api/clawdpak`.

## Back to stock (verified 2026-10-11)

```sh
curl -F "firmware=@SDPro_V1.0.6_20260525_174828.bin" http://<clock-ip>/update
```

Get the image from https://github.com/JUZIPi-tech/SD_PRO. A local copy is in `.stock-backup/` (git-ignored, because it holds secrets).

The round trip was tested on the device: clawd → stock → clawd.
- After the revert, the stock `index.html`, `javascript.js` and `style.css` were byte-identical to the backup.
- The photo/GIF list and the stock EEPROM settings were unchanged.
- The only difference: free space was 216 KB lower, from `clawd.pak` and `config.json`. Both can be deleted later and are harmless to stock.

## Data

The Clawdmeter macOS daemon pushes over HTTP. Add this to `~/.config/claude-usage-monitor/config`:

```ini
http_targets = clawdmeter.local     # or the IP
ble = off                           # if there is no Bluetooth Clawdmeter
```

`POST /api/usage` accepts the Clawdmeter payload `s sr w wr st ok acct tp rd t tf m x` and is unauthenticated, like upstream:
- `ok:false` is accepted (200). The screen goes idle (Auto: splash) once the data is stale.
- `c` and `b` (chime) are ignored.

## Heap watch

`GET /api/heap` needs no auth and is read only. It returns these fields:
- `heap`, `maxblk`, `minHeap`, `minMaxblk`, `frag`
- `uptime`, `reset`, `version`
- `pak` (animations loaded; 0 means `/clawd.pak` is missing), `splash`, `anim`

Measured on the device: `minHeap` stayed at 24,064 B or more through usage, Enterprise and alert payloads.

```sh
while sleep 60; do curl -s http://clawdmeter.local/api/heap; echo; done >> heap.log   # 24 h soak
```

## Code map

| Path | Role |
|---|---|
| `tools/clawd_assets.js` | Builds `data/clawd.pak` (17 animations, 4-bit) and `src/features/clawd/clawd_fonts.h` (subset 4 bpp fonts with kerning, PROGMEM) from Clawdmeter's `firmware/src`. |
| `ClawdPack.*` | Pack reader. LittleFS on the device, stdio in the preview. Loads one frame (≤ 832 B) at a time. |
| `ClawdDraw.*` | No framebuffer. Each region is composed in 4-row bands (1,920 B): background, anti-aliased shapes, then anti-aliased text. One SPI push per band. Pixel-art cells are diffed per grid row. |
| `ClawdAnim.*` | Port of `splash.cpp`: player, splash with walkers and rate groups, corner mascot, mini. |
| `ClawdRate.*` | Port of `usage_rate.cpp`. |
| `ClawdUi.*` | Screens, views, alert and status line. |
| `features/usage/UsageMode.cpp` | Glue: Arduino_GFX surface plus ticking. |
| `tools/clawd_preview/run.sh` | Desktop build of the same renderer. Writes PNGs of every screen and animation to `.pio/clawd_preview/`. |
| `tools/gzip_webui.py` | Writes `webui.h` and `webui_clawd.h`, the latter without the ticker, radar and HA tabs. |

Build (`pio run -e smalltv_clawd`): static RAM 47,836 B (58.4%), image 517,904 B.

The plain `smalltv` env (4m1m, all features) still builds, but it would need `/clawd.pak` on its own filesystem and is not meant for this clock.
