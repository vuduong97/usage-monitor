# Clawd clock protocol, v1

How a computer talks to a Clawd desk clock (ESP8266 "SmallTV" running the
clawd firmware, 2.x) on the local network. Plain HTTP on port 80, JSON bodies.
This file is kept identical in the desktop app repo and the firmware repo.

## Finding the clock

The clock advertises itself over mDNS (Bonjour):

| Field | Value |
| --- | --- |
| Service type | `_clawdmeter._tcp.local.`, port 80 |
| TXT `id` | the clock's hostname, e.g. `clawd-3fa2` (firmware default `smalltv-<chip id>`) |
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
| `ok` | `false` = "no data" (not logged in / no token); other keys ignored |

Reply: `200 {"ok":true}`, or `400` for a malformed body.

### `POST /api/state`: what Clawd does

```json
{"a":"typing","txt":"Editing","dim":30,"ttl":60}
{"a":["yawn","doze","collapse","sleeping"]}
{"a":["react left","react right"],"rep":true,"u":true,"txt":"Which framework?"}
{"a":"typing","v":["building","carrying","typing boss"],"txt":"Running"}
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

Animation names (firmware 2.16): `thinking`, `ultrathink`, `typing`, `typing boss`,
`building`, `wizard`, `debugger`, `headphones`, `juggling`, `error`, `dizzy`,
`happy`, `notification`, `sweeping`, `carrying`, `idle`, `look`, `bubble`,
`reading`, `yawn`, `doze`, `collapse`, `sleeping`, `wake`, `low battery`,
`double jump`, `heart eyes`, `react left`, `react right`, `annoyed`.
An unknown name is ignored by the clock.

### `GET /api/heap`: health

`version`, `uptime`, `heap`, `pak` (screens in `/clawd.pak`, 0 = missing),
`desk` (animations in `/desk.pak`, 0 = missing), `anim`, `deskAnim` (the desk
animation on screen now, a variant too; only firmware that understands `v` has it),
`reset`, `crash`.

The app reads it every 5 s. When `uptime` shows the clock restarted, it
sends the last `/api/usage` payload and the current `/api/state` again at
once, so the clock leaves its "no data" splash without waiting for the next poll.

## Endpoints for setup (login when a password is set)

HTTP digest auth, user/password from the clock's web page. Used by the
provisioning script and the clock's own web page, never by the desktop app.

`GET/POST /api/config` (POST takes partial JSON: `hostname`, `wifi` list,
`apSsid`, `apPass`, `auth`), `POST /api/reboot`, `GET /api/fs/list`,
`GET /api/fs/get?path=`, `POST /api/fs/put`, `POST /api/fs/rm`,
`POST /api/clawdpak`, `POST /api/deskpak`, `POST /update` (firmware).

## Setup mode

With no saved WiFi (or none in reach) the clock opens its own WiFi network
(`apSsid`: firmware default `SmallTV-Setup`, open; provisioned clocks use
`Clawd-XXXX`) and shows the network name, password and
`http://192.168.4.1` on screen. Joining it opens a captive portal where the
owner picks the home WiFi. The clock then reboots onto that network.

## Compatibility

- The app checks `ver` / `version`. Major version 2 speaks this protocol.
- New keys may be added; both sides ignore keys they do not know.
- The clock talks plain HTTP only (no TLS), so keep it on a trusted network.
