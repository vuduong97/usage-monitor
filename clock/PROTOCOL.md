# Clawd clock protocol, v1

How a computer talks to a Clawd desk clock (ESP8266 clock running the
clawd firmware, 2.x) on the local network. Plain HTTP on port 80, JSON bodies.
This file is kept identical in the desktop app repo and the firmware repo.

## Finding the clock

The clock advertises itself over mDNS (Bonjour):

| Field | Value |
| --- | --- |
| Service type | `_clawd._tcp.local.`, port 80 |
| TXT `id` | the clock's hostname, e.g. `clawd-3fa2` (firmware default `clawd-<chip id>`) |
| TXT `ver` | firmware version, e.g. `2.16.0` |
| TXT `path` | `/api/usage` |

It also answers as `<hostname>.local` and advertises `_http._tcp`.

When mDNS is blocked (some routers, guest networks), probe `GET /api/heap` on
each address of the local /24. A clawd clock answers `200` with JSON that has
`version`, `pak` and `desk` keys.

The address can change (DHCP). Remember the clock by its `id`, not its IP.
The clock shows its IP on screen for a few seconds after it joins WiFi.

## Endpoints the app uses (no login needed)

### `POST /api/usage`: usage bars

Send every ~60 s. The clock goes back to "no data" if nothing arrives for a while.

```json
{"s":42,"sr":166,"w":17,"wr":7626,"st":"allowed","acct":"pro","ok":true,"t":1791798836,"tf":24}
```

| Key | Meaning |
| --- | --- |
| `s` | 5-hour session use, percent (required unless `ok` is false) |
| `sr` | minutes until the session resets |
| `w` | 7-day use, percent |
| `wr` | minutes until the week resets |
| `st` | status text from the usage API (`allowed`, ...) |
| `acct` | `pro` or `ent` (enterprise layout) |
| `tp`, `rd` | enterprise: time-elapsed percent, reset date text |
| `t` | current local time as epoch seconds (the clock has no RTC) |
| `tf` | 12 or 24 hour clock |
| `m`, `x` | alert line to show; `x: true` clears it |
| `src` | whose usage: absent = Claude, `codex` = Codex CLI (the cards read "Codex 5h" / "Codex week"); `off` with `ok: false` = the user shows no usage: the time and Clawd, never "No data" (`fwSeq` 7) |
| `ok` | `false` = "no data" (not logged in / no token); other keys ignored except `t`/`tf` with `src: off` or `why: login`, and the alert line `m`/`x` |
| `why` | with `ok: false`, why there is no usage: `login` = the Claude login expired. The clock shows its classic screen (time, Clawd, IP) with "Login expired" on the status line, even over a style pack or the splash, until usage comes back (`fwSeq` 7). An older clock ignores it and shows "No data" |

Reply: `200 {"ok":true}`, or `400` for a malformed body.

### `POST /api/state`: what Clawd does

```json
{"a":"typing","txt":"Editing","dim":30,"ttl":60}
{"a":["yawn","sleeping","wake"]}
{"a":["react left","react right"],"rep":true,"u":true,"txt":"Which framework?"}
{"a":"typing","v":["building","carrying","typing boss"],"txt":"Working"}
{"a":""}
```

| Key | Meaning |
| --- | --- |
| `a` | animation name, or a list of up to 4 (each plays one pass, the last loops). Empty or missing = back to the usage screens |
| `rep` | cycle the whole list instead of looping the last one |
| `v` | up to 3 same-meaning variants of a single looping `a` (no `rep`): `a` plays 3 passes, then one variant picked at random (never the previous one), then `a` again. Clocks without it ignore `v` and just loop `a` |
| `u` | urgent: switch now instead of at the end of the current pass |
| `txt` | one line of ASCII text under the animation (the font has no accents) |
| `cd` | countdown seconds, shown as `MM:SS` after the text |
| `dim` | brightness cap 0–100 |
| `ttl` | seconds the state lives (1–3600, default 60). Resend every ~20 s while it should stay |

Animation names (the desk.pak's own list, `desk` in `/api/heap` counts them): `thinking`,
`ultrathink`, `typing`, `typing boss`, `building`, `wizard`, `debugger`, `headphones`,
`juggling`, `error`, `dizzy`, `happy`, `notification`, `sweeping`, `carrying`, `reading`,
`idle`, `look`, `bubble`, `coffee hand`, `shy`, `outlaw`, `yawn`, `sleeping`, `wake`,
`low battery`, `double jump`, `heart eyes`, `react left`, `react right`, `annoyed` (31).
The 30-animation pak before it had `doze` and `collapse` instead of `coffee hand`, `shy`
and `outlaw`. A name the pak does not have plays its first animation (`thinking`), so
the app only sends the newer names when every clock reports `desk` ≥ 31.

### `GET /api/heap`: health

`version`, `uptime`, `heap`, `pak` (screens in `/clawd.pak`, 0 = missing),
`desk` (animations in `/desk.pak`, 0 = missing), `anim`, `deskAnim` (the desk
animation on screen now, a variant too; only firmware that understands `v` has it),
`reset`, `crash`, `styleApi` and `style` (the style pack API the clock draws, and
the pack it shows: `""` = the classic screen; `styleApi` is 2 from `fwSeq` 8, whose
packs play the official Clawd animations, and `styleAnim` names the one playing). From `fwSeq` 6: `pakCrc` and
`deskCrc`, the CRC-32 (zlib, 8 hex digits) of `/clawd.pak` and `/desk.pak`. The
value is `""` when the pack is missing or not yet checksummed: the clock checksums
in the background from 30 s after boot, about a minute for `desk.pak`. The app compares
them with a firmware feed release so it only re-sends packs that differ.

The app reads it every 5 s. When `uptime` shows the clock restarted, it
sends the last `/api/usage` payload and the current `/api/state` again at
once, so the clock leaves its "no data" splash without waiting for the next poll.

## Endpoints for setup (login when a password is set)

HTTP digest auth, user/password from the clock's web page. Used by the
provisioning script and the clock's own web page, never by the desktop app.

`GET/POST /api/config` (POST takes partial JSON: `hostname`, `wifi` list,
`apSsid`, `apPass`, `auth`), `POST /api/reboot`, `GET /api/fs/list`,
`GET /api/fs/get?path=`, `POST /api/fs/put`, `POST /api/fs/rm`,
`POST /api/clawdpak`, `POST /api/deskpak`, `POST /update` (firmware). The two pack
uploads answer `{"ok":true,"anims":N,"crc":"<hex>"}`. The `crc` is that of the bytes
received, so the app can check the pack arrived intact. `desk.pak` is deleted before its
upload, because LittleFS has no room for two copies.

The settings window of the desktop app also uses `/api/config`, `/update`, the two
pack uploads (firmware feed, `feed/README.md` in clawd-home) and the
usage screen's style pack: `POST /api/style` (multipart field `file`, a pack from
`firmware/styles/`) shows it at once, `DELETE /api/style` goes back to the classic
screen, `GET /api/style` (no login) says which one is installed. See
`docs/STYLE_PACK.md`.

## Setup mode

With no saved WiFi (or none in reach) the clock opens its own WiFi network
(`apSsid`: firmware default `Clawd-Setup`, open; provisioned clocks use
`Clawd-XXXX`) and shows the network name, password and
`http://192.168.4.1` on screen. Joining it opens a captive portal where the
owner picks the home WiFi. The clock then reboots onto that network.

## Compatibility

- The app checks `ver` / `version`. Major version 2 speaks this protocol.
- New keys may be added; both sides ignore keys they do not know.
- The clock talks plain HTTP only (no TLS), so keep it on a trusted network.
