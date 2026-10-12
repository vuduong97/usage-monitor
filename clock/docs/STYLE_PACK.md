# Style packs

A style pack is one usage screen, drawn by the clock from a small binary file
instead of compiled-in code. The desktop app carries every pack and sends only the
one in use. The clock switches to it at once, with no reboot, and keeps it across
reboots and firmware updates. With no pack installed, the clock draws the classic
screen.

Sixteen packs ship with the firmware source:
- The first six: `gameboy`, `synth`, `paper`, `blueprint`, `pastel` and `onebit`.
- Batch A, retro computers and devices: `ambercrt`, `vt100tui`, `mac84`, `win95`,
  `biossetup`, `c64`, `zxspec`, `casiolcd`, `calc` and `hifivfd`.

Adding a pack is data only. It needs no firmware change, no `CLAWD_FW_SEQ` bump
and no OTA.

## Behaviour

- **Usage and idle:** while a pack is installed, the usage view and the "no data"
  idle view both draw the pack. Each pack has its own no-data state.
- **Alerts:** an alert (`m` in the payload) shows the classic screen with its
  blinking frame. When the alert clears, the screen goes back to the pack.
- **Enterprise:** Enterprise accounts (`acct: "ent"`) always draw the classic
  screen, because they have no 5-hour/weekly windows.
- **Desk and splash:** the desk animations and the Clawd splash are not affected.
- **Old setting:** the old `usage.layout` setting is still read and saved, but it
  is ignored. Values 1–5 (pace, hero, rings, gauges, pixel) were removed and now
  draw classic.

## Endpoints

| Request | Auth | Effect |
|---|---|---|
| `GET /api/style` | none | `{api, active, slot, id, name, size, crc, error}`. `id`/`name`/`size`/`crc` only when active. `error` says why the last boot or install left no pack. |
| `POST /api/style` (multipart, field `file`) | login | Install and show the pack. Returns `{ok, error?, style:{…}}`; a refused pack is a 400 with the reason, and the previous pack stays. |
| `DELETE /api/style` | login | Delete the pack and go back to the classic screen. |

`/api/heap` adds these fields:
- `styleApi`: the pack API this image draws (1).
- `style`: the id of the pack shown, or `""` for classic.
- `styleFrameUs` and `styleFrameMaxUs`: the last animation repaint and the slowest
  since the pack was attached, in µs. This covers decode and SPI.
- `stylePaintUs` and `stylePaintMaxUs`: the same for a full-screen paint (on a new
  payload, a new minute, or coming back from the desk view).
- `onStyle`: the usage screen on the panel is the pack's (false on the desk view,
  during an alert, for Enterprise, or with no pack).

Measured on the clock (2026-10-12, 80 MHz, 20+ installs in a row):

| Pack | Full paint | Slowest animation repaint |
|---|---|---|
| gameboy | 196–218 ms | 36 ms |
| synth | 255–276 ms | 96 ms |
| paper | 203–221 ms | (no animation) |
| blueprint | 440–446 ms | (no animation) |
| pastel | 222 ms | 29 ms |
| onebit | 166 ms | 25 ms |

An install takes 1–2 s from `curl` to the pack on screen (upload, slot copy ~0.6 s,
paint). `minHeap` stayed ≥ 23,040 B.

```sh
curl -u admin:<pass> -F "file=@firmware/styles/synth.pak" http://<clock>/api/style
curl -u admin:<pass> -X DELETE http://<clock>/api/style
```

## Storage

`StyleStore.cpp` (device only) keeps two copies of the pack:

1. **`/style.pak` on LittleFS** is the copy that persists. OTA never touches the
   filesystem.
2. **The flash slot** is a mirror of that file in the free sketch flash. It starts
   at the first 4 KB sector after the running image (`ESP.getSketchSize()`) and is
   capped at 32 KB, inside the 1 MB that is memory-mapped at `0x40200000`.
   `StylePack` reads the pack in place through the flash cache (`memcpy_P`), so its
   fonts, ops and strings cost no RAM. The interpreter's own state is about 360 B
   of bss.

How the two copies are kept in step:
- **OTA:** a firmware update stages the new image in that same free space. So the
  upload handler detaches the style first (`styleStoreSuspend`).
- **Boot:** the slot is compared with `/style.pak`. When the header differs or the
  CRC fails, the file is copied into the slot again. The app does not have to
  reinstall a pack after an update.
- **Failed OTA:** the slot is re-checked (`styleStoreResume`).
- **Crash while drawing:** the pack marks a word in RTC memory while it paints. If
  the clock then resets on an exception or a watchdog, the next boot deletes
  `/style.pak` and shows the classic screen, with `error` in `/api/style` saying
  so. A crash in a pack cannot boot-loop the clock. (A power cut clears the mark.)
- **Debug trail:** `/style.log` (newest ~2–4 KB, read it with
  `GET /api/fs/get?path=/style.log`) gets one line per install step (upload,
  erase, write, validate, attach, rename), one per boot, one per WiFi status
  change, and a heartbeat every 30 s while WiFi is down. It survives a power
  cycle, so after a clock drops off the network it shows whether the CPU still
  ran.
- **When flash is written:** only when a pack is installed, or at the first boot
  after a firmware update.
- **Install:** the upload goes to `/style.pak.tmp`. Its magic, API, size and CRC
  are checked from the file first, so a damaged upload leaves the current pack on
  screen. Only then is it copied into the slot, validated in full through the
  flash cache, attached, and renamed to `/style.pak`. An upload over 32 KB stops
  being written at once. When LittleFS is short of room (less than 64 KB free), the old
  `/style.pak` is deleted before the upload; the screen keeps drawing the old pack
  from the slot until the new one is in.

## Building packs

```sh
cd firmware
node tools/style/build.js <Clawdmeter>/firmware/src     # writes styles/*.pak + styles/index.json
```

The argument is the reference firmware's `src`, which holds the LVGL font `.c`
files the compiler subsets.
- The build fails when a pack is over 20 KB, or when a fixed label uses a glyph its
  font lacks.
- Packs are committed with the firmware, like `clawd_fonts.h`.
- The desktop app's `tools/bundle_firmware.py --src` and `tools/build_preview.py
  --src` copy `styles/` next to the bundled image and the preview renderer.

To check the packs on the host, run:

```sh
DESK_PACK_PATH=…/desk.pak sh tools/clawd_preview/run.sh /tmp/cp-out
```

This renders every pack listed in `styles/index.json` in four states
(`30-<id>-<k>-<state>`) plus synth animation frames,
and checks the guards: a corrupt pack, a future API, a truncated pack, an alert
over a style, Enterprise, and removing the pack. It ends with `styles: ok`.

### Writing one

A pack source is `tools/style/packs/<id>.js`:

```js
module.exports = {
  id: 'blueprint', name: 'Blueprint', bg: '#0d3b7a', light: false,
  draw(S) {
    S.rect(0, 0, 240, 240, '#164a8f', { pattern: ['cols', 10, 0, 1] });
    S.text('{s}% USED', 16, 30, 's14', '#eaf2ff', { when: 'ok' });
    S.rect(16, 46, 0, 22, '#eaf2ff', { when: 'ok', x1: ['s', 208] });   // a bar 0..208 px wide
    S.mask('clawd', 20, 150, 2, '#eaf2ff', { track: S.blink(500) });   // 2x scale, blinking
  },
};
```

Add the id to `ORDER` in `build.js`, and a Vietnamese description to
`STYLE_DESC` in the app's `ui/app.js`.

### From a design sheet

The Batch A packs are ports of a design sheet ("Clawd style packs - Batch A.html").
Each of its artboards draws the four states with a small vocabulary:
- `R` rect, with an optional stripe `p: [dir, period, on]`
- `O` 1 px outline
- `C` disc
- `P` triangle
- `T` text (`S12`…`S48` Styrene, `M18`/`M32` mono; `y` is the top of a line box
  as tall as the font size)
- `S` 1-bit sprite

`tools/style/mock.js` gives a pack source the same calls (`b.R`, `b.T`, `b.Sp`,
`b.clawd`, …), so a port keeps the artboard's numbers. It converts two things:
- **Text y:** the font's baseline offset (`Y_OFF`, measured once per font) moves
  the glyphs onto the mock's baseline.
- **Stripes:** the stripe phase is shifted from the element's edge to the screen.

`b.hatch` draws the sheet's 45° hatch (`\` lines), because the firmware's `diag`
pattern runs the other way.

Porting works like this:
- Each `if (s.hot)` / `if (ok)` in the artboard becomes a `when` gate:
  - `hot`
  - `ok fresh !hot` (fresh)
  - `ok !hot !fresh` (normal)
  - `!ok` (no data)
- Each threshold becomes a `gate` on `s`/`w`/`ts`/`tw`.
- Each data string becomes a token.
- Data that the stale screen must hide gets a separate `!ok` op printing `--`. The
  `fresh` bit does not need live data, so a fresh-only op also needs `ok`.

```sh
node tools/style/build.js <Clawdmeter>/firmware/src
CLAWD_STYLES_ONLY=1 [CLAWD_STYLE_ONLY=<id>] sh tools/clawd_preview/run.sh /tmp/cp
python3 tools/style/compare.py "<sheet>.html" /tmp/cp /tmp/cmp [<id> ...]
```

`compare.py` renders the sheet in headless Chrome and crops each pack's 1x states.
It writes `<id>-<k>.png` (mock | clock | difference) and prints the share of
pixels that differ. Text always differs a little, because the sheet's fonts (Space
Grotesk, JetBrains Mono) are not the clock's (Styrene, DejaVu Mono). Shapes,
colours and positions should match.

The preview also prints each pack's full paint time on the host, plus `~clock`.
`~clock` is a fit to the first six packs as measured on the clock:
43 ms + 1.54 ms per host µs, within about 12 ms for all six. It is a guide for
the 500 ms budget, not a measurement.

Batch A on the host, 2026-10-13:

| Pack | Size | Ops | ~clock full paint |
|---|---|---|---|
| ambercrt | 7,984 B | 126 | 226 ms |
| vt100tui | 5,376 B | 68 | 197 ms |
| mac84 | 18,348 B | 212 | 325 ms |
| win95 | 13,240 B | 135 | 327 ms |
| biossetup | 7,276 B | 71 | 221 ms |
| c64 | 5,112 B | 62 | 218 ms |
| zxspec | 5,900 B | 83 | 270 ms |
| casiolcd | 7,400 B | 75 | 369 ms |
| calc | 9,120 B | 57 | 222 ms |
| hifivfd | 9,660 B | 137 | 319 ms |

**Primitives:**
- `rect`: optional radius and a `diag`/`cols`/`rows` pattern.
- `strokeRect`
- `text`: with `{token}`s, aligned left, centre or right.
- `line`
- `tri`
- `mask`: a 1-bit bitmap, scaled. The built-in masks are `clawd`, `clawd_eyes`,
  `clawd_edge` and `clawd_inner`; `defMask` adds your own.
- `disc`: becomes rect rows.

**Fonts:** `s14`, `s24`, `m18` and `t34` are linked into the firmware, so they
are free. Any other font (`s12`, `s16`, `s20`, `s28`, `s48`, `t56`, `m32`) is embedded as a subset of the
characters the pack can print.

**Gating:** `when: 'ok !over hot'` combines the usage conditions with AND:

| Condition | True when |
|---|---|
| `ok` | live data |
| `hot` | session ≥ 80 % |
| `whot` | weekly ≥ 80 % |
| `fresh` | the window just reset |
| `over` | over 100 % pace |
| `paceover` | on course to run out |
| `out` | out before the reset |
| `clock` | the wall clock is known |
| `sr` | the session reset is known |
| `wr` | the weekly reset is known |

**Data-driven geometry:**
- `dx: [src, span]` moves an op by the source value times `span`.
- `x1: [src, span]` sets its width or end point the same way.
- `gate: [src, lo, hi]` shows the op only while the value is between `lo` and `hi`.
- The sources are `s`, `w` (usage 0–1), `ts` and `tw` (time elapsed in the
  window, 0–1).

**Tokens:**

| Token | Prints |
|---|---|
| `{s}`, `{w}` | the two usage percentages |
| `{sleft}` | the session percentage left |
| `{clock}`, `{dow}` | the time and the weekday |
| `{sr@}`, `{wr@}`, `{wr@d}` | when each window resets (`{wr@d}` with its day) |
| `{sr~}`, `{wr~}` | how long until each reset |
| `{out}` | the projected time of running out |
| `{end}` | the projected usage at reset |

**Animation:** `track: S.track(stepMs, [{dx, dy, vis, color}, …])` is a list
of steps played on absolute time. `phase` offsets an op along its
track. `S.blink(ms)` is a two-step track.
- Gradients, the synth sun's rows and the grid's perspective lines are computed in
  the compiler and stored as plain ops.
- Blur is not supported, so a glow is an offset dim copy.

## Pack format (API 1)

All fields are little-endian. The pack is laid out like this:
- a 96 B header
- the palette (RGB565)
- the font descriptors with their glyphs, bitmaps and kerning
- the tracks with their steps
- the ops
- the masks
- the strings

**Header:**

| Offset | Field |
|---|---|
| 0 | `"CSTY"`, api u8, flags u8 (bit 0: light), headerSize u16, size u32 |
| 12 | crc32 (zlib) of bytes [16..size) |
| 16 | id[16] |
| 32 | name[24] |
| 56 | bg565 u16 |
| 58 | fontCount u8 |
| 59 | trackCount u8 |
| 60 | opCount u16 |
| 62 | paletteCount u16 |
| 64 | maskCount u16 |
| 68–95 | paletteOff, fontsOff, tracksOff, opsOff, masksOff, stringsOff, stringsLen (u32 each) |

**FontDesc** (24 B): glyphsOff, bitmapOff, bitmapLen, kernsOff, kernCount u16,
glyphCount u8, lineHeight, baseLine, pad[2], builtin. `builtin` is 0xFF for an
embedded font, otherwise the id of the linked font. Glyphs are the firmware's
10-byte `ClawdGlyph`; a kern entry is 3 B.

**TrackDesc** (8 B): stepMs u16, steps u8, pad, stepsOff u32. A step is 4 B:
dx, dy, vis, color.

**MaskDesc** (8 B): w, h, pad[2], bitsOff. Rows are 1 bit per pixel, MSB first.

**Op** (36 B), in order:
- kind (rect 1, text 2, line 3, tri 4, mask 5)
- color
- need u16, not u16: condition bits
- track (0xFF = none)
- flags (zeroOk 1, upper 2, clamp 4)
- x, y, w, h. For text, `w` is the string offset. For a line, `w`,`h` are its end
  point.
- dxSrc, x1Src (0 none, 1 s, 2 w, 3 ts, 4 tw, 255 fixed), dxSpan, x1Span, phase
- gateSrc, a, gateLo, gateHi. `a` is the rect radius, the text font or the mask
  id. `gateLo`/`gateHi` are in thousandths.
- b (pattern modulus, text align or mask scale), c (min width)
- x3, y3. For a triangle, its third point. For a rect pattern, `x3` is the phase
  and `y3` is kind<<8 | on.

**Strings:** bytes below 13 in a string are token codes: s 1, w 2, sleft 3,
clock 4, dow 5, sr@ 6, wr@d 7, sr~ 8, wr~ 9, out 10, end 11, wr@ 12.

`styleValidate` checks the magic, the API, the CRC and every offset, index and
count before a pack is attached, so a damaged or hostile pack is refused, never
drawn. A pack for another API is refused as well. Bump `STYLE_API` (StylePack.h)
and the compiler's `API` together for any format change. The app sends a pack
only to a clock whose `styleApi` matches.

## Drawing

- `styleUpdate` turns the usage data into condition bits and token strings, and
  returns a signature of everything the paint shows. ClawdUi calls it every
  500 ms.
- When the signature changes, the clock repaints the whole screen.
- Otherwise `styleAnimate` repaints only what the animation tracks changed:
  - it finds the ops whose track step changed since the last frame,
  - merges their old and new boxes into at most 6 rectangles,
  - and composes each rectangle in bands, like ClawdDraw. That means every op that
    touches the band is drawn in order, anti-aliased text included, with one SPI
    push per band.
- There is no framebuffer. Frame time is reported in `/api/heap`. The target is
  p95 ≤ 100 ms; it is not measured on the device yet.
