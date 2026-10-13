# clawd-home

Firmware and clock tooling for the Clawd desk clock (JUZIPi SD Pro, ESP8266,
240×240 ST7789). The clock shows Claude usage and plays Clawd animations that
react to what Claude Code and pi are doing. The host side is a separate repo,
**clawd-desktop** (tray app, hooks, pi extension, `clawd` CLI).

**Private.** Never pushed anywhere. The animation pack `desk.pak` is rendered from
clawd-on-desk art (All Rights Reserved, personal use only) and the fonts are
Anthropic's: generated on this Mac and never committed. Origins: `NOTICE`.

```
firmware/        PlatformIO project (envs: clawd, clawd_loader), src/, tests/, tools/
tools/desk/      clawd-on-desk SVG -> build/desk/desk.pak (gitignored)
tools/flash.sh   build + size check + OTA a clock
tools/release.sh build + test + encrypt + publish a release to the firmware feed (feed/README.md)
feed/            the feed: Cloudflare Worker over a private R2 bucket, release packer
tools/provision.sh  stock clock -> clawd clock ready to gift
tools/stock_fs.sh   back up / delete / restore the stock firmware's files on the clock
PROTOCOL.md      HTTP + mDNS contract with the desktop app (identical copy in both repos)
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

Friends do not get this repo. They get a clock flashed here and the desktop app
from the public repo **clawd-desktop** (`../clawd-desktop`: tray app for Mac and
Windows, finds the clock over mDNS `_clawd._tcp` or a LAN scan, installs the
Claude Code hooks, the pi extension and the `clawd` CLI itself). Both sides follow
`PROTOCOL.md` (identical copy in both repos).

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

