# clock/ (formerly clawd-home)

Firmware and clock tooling for the Clawd desk clock (JUZIPi SD Pro, ESP8266,
240×240 ST7789). The clock shows Claude usage and plays Clawd animations that
react to what Claude Code and pi are doing. The host side is the rest of this repo
(tray app, hooks, pi extension, `clawd` CLI); every app release ships the firmware built
from this folder (`sh tools/build_clock.sh` at the repo root, job `firmware` of the
Release workflow).

This folder was the private repo clawd-home until 0.3.0; its history is kept (the font
sources, `.otf`, were dropped from it). It holds `clawd_fonts.h` (generated from Anthropic's
fonts), `firmware/data/clawd.pak` and `firmware/data/desk.pak` (rendered from clawd-on-desk
art, All Rights Reserved): see `../NOTICE`. The clawd-on-desk SVGs and the font files are
never committed.

Changing code under `firmware/src` needs a higher `CLAWD_FW_SEQ` (`firmware/src/config.h`):
CI fails otherwise. Style packs (`firmware/styles`) do not: they can also reach installed
apps between releases (`../tools/publish_packs.sh`, see the root README).

```
firmware/        PlatformIO project (envs: clawd, clawd_loader), src/, tests/, tools/
tools/desk/      clawd-on-desk SVG -> build/desk/desk.pak (gitignored)
tools/flash.sh   build + size check + OTA a clock
tools/release.sh LEGACY: publish to the old encrypted feed that apps <= 0.2.x read (feed/README.md)
feed/            LEGACY: the old feed (Cloudflare Worker over a private R2 bucket), until 0.2.x is gone
tools/provision.sh  stock clock -> clawd clock ready to gift
tools/stock_fs.sh   back up / delete / restore the stock firmware's files on the clock
../PROTOCOL.md   HTTP + mDNS contract between the app and the clock (one copy, at the repo root)
docs/FIRMWARE.md flash layout, OTA budget, endpoints, /api/heap, code map
docs/BEHAVIOUR.md what the clock shows under the app (layers, holds, pet moments)
docs/STYLE_PACK.md usage screen style packs: format, storage, /api/style, writing one
```

## Build

```sh
cd firmware
pio run -e clawd                  # firmware.bin; must stay <= 524,288 B (OTA budget)
pio run -e clawd_loader           # OTA trampoline for stock clocks with a tiny updater
sh tests/host/run.sh              # Settings.cpp host test (needs one pio run for ArduinoJson)
sh tools/desk_check/run.sh        # desk.pak decoder vs the encoder's frame CRCs
sh tools/clawd_preview/run.sh     # desktop render of the screens -> .pio/clawd_preview/
python3 tools/gzip_webui.py       # after editing src/webui.html
```

## Animations

```sh
cd tools/desk && npm install && cd ../..          # needs the clawd-on-desk checkout + Google Chrome
CLAWD_ON_DESK_DIR=/tmp/clawd-on-desk node tools/desk/desk_to_pak.js --verify   # -> build/desk/desk.pak (CLDK v3)
#   --only NAME,...  --sheet (contact sheets + worst-frame PNGs)  --probe (timings only)
curl -u admin:<pass> -F "file=@build/desk/desk.pak" http://<clock>/api/deskpak   # ~45 s
node tools/desk/desk_repack.js     # an old v2 pack -> v3 in place (keeps desk.v2.pak), pixels unchanged
```

`desk.pak` (1.86 MB: 25 animations deflated per frame, the 6 busiest stored raw
for draw speed; 2.85 MB as the old v2) only fits because the stock firmware's own files (web UI,
icons, fonts, GIFs: 1.5 MB) were moved off the clock with `tools/stock_fs.sh`.
Restore them before flashing the stock firmware (see `docs/FIRMWARE.md`).

## Clocks for friends

Friends get a clock flashed here and the desktop app from this repo's GitHub Releases
(tray app for Mac and Windows, finds the clock over mDNS `_clawd._tcp`, installs the
Claude Code hooks, the pi extension and the `clawd` CLI itself, and keeps the clock's
firmware at the version it ships). Both sides follow `../PROTOCOL.md`.

Per clock (stock JUZIPi SD Pro, firmware SDPro 1.0.x):

```sh
cd firmware && pio run -e clawd && cd ..   # firmware.bin (<= 520 KB)
# join the stock clock to the bench WiFi by hand, read its IP off the screen, then:
CLAWD_BENCH_SSID=... CLAWD_BENCH_PASS=... tools/provision.sh <clock-ip>
tools/provision.sh <clock-ip> --dry-run           # what it would do, changes nothing
```

It flashes clawd through the stock OTA page, moves the stock files off with
`stock_fs.sh` (backup per clock in `.stock-backup/provision/`), uploads `desk.pak`,
names the clock `clawd-<chip id>`, sets a random setup-WiFi password and web login,
forgets the bench WiFi and prints the card for the box (also in
`.stock-backup/provision/cards.tsv`). Rerun after a failure: it skips finished steps.
It stops if the stock OTA page is not the one SDPro 1.0.6 has.

## clawd

```sh
clawd status                 # what the clock shows + live sessions
clawd wave | clawd poke | clawd say "drink water" | clawd play "double jump"
clawd focus 25 | clawd focus stop
clawd anims
```

## Flash a clock

```sh
tools/flash.sh 10.97.44.122              # build + size check + OTA to /update + wait for the reboot (~35 s)
tools/flash.sh --no-build 10.97.44.122   # re-flash the last build
#  clock: the IP argument, else CLAWD_CLOCK, else http_targets in
#  ~/.config/claude-usage-monitor/config, else 192.168.1.48
#  web password (if enabled): CLAWD_AUTH=user:pass

# by hand:
cd firmware && pio run -e clawd                       # image must stay <= 524,288 B
curl -F "firmware=@.pio/build/clawd/firmware.bin" http://192.168.1.48/update
```

The flash layout stays stock 4m3m (1 MB sketch + 3 MB LittleFS, never formatted;
the stock files are backed up on the Mac, see above).
`/update` accepts the stock image for rollback; see `docs/FIRMWARE.md`.
The `clawd` env drops newlib's float printf/scanf (`firmware/no_float_io.py`):
`%f` prints nothing in this build.

