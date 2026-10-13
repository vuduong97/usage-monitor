# Prompt 4/10: Đồng hồ đo & kỹ thuật (#31–40)

Mở cuộc trò chuyện mới trong Claude Design, đính kèm `design/usage-screen-preview/style-6-sheet.png`, rồi dán khối dưới đây.

```text
Design 10 usage screens for Clawd, a desk clock with a 240x240 px screen (1.54" IPS, 16-bit colour, ESP8266) that shows Claude usage limits. Each design will later be rebuilt as a firmware "style pack", so follow the drawing rules exactly. The attached image shows 6 existing styles: do not repeat them.

MUST SHOW in every state: 5-hour % (the biggest thing, at least 28 px tall), weekly %, the clock (HH:MM), and Clawd. Everything else (reset times, time left, forecast, decoration) is optional and up to you. Text at least 12 px, 6 px safe margin, English ASCII text only.

4 STATES per design, with this sample data:
- Normal: Mon 14:05. 5H 42%, resets 16:19 (2h 14m). Week 67%, resets Thu 08:05 (2d 18h).
- Hot: Mon 14:05. 5H 88%, resets 14:43 (38m), "out at ~14:41". Week 93%, resets 23:05. Must feel urgent.
- Fresh: Mon 09:12. 5H 4%, resets 14:03 (4h 51m). Week 18%, resets Sun 01:52. A clean start.
- No data: 14:05. All values "--", data dimmed, Clawd asleep or grey.

DRAWING RULES (hardware limits):
- Only these: filled rectangle (corner radius, optional stripe pattern: diagonal, vertical or horizontal), 1 px rectangle outline, 1 px line, filled triangle, filled circle, text, 1-bit pixel sprites at integer scale.
- One flat colour per element, at most 12 colours per design. No blur, shadow, glow, opacity, soft gradients, images, emoji or rotation. Fake a glow with a dim offset copy and a gradient with 3-8 flat bands.
- Data can set a bar's width or an element's x position by %. An element can show only above a % threshold, or only in one state. Rings and needles are pre-drawn pieces: at most 40 ring segments or 12 needle positions.
- Animation is step-based only: move by whole pixels, show or hide, swap colour, at least 100 ms per step. At most 3 small animated things, about 120x60 px in total.
- At most 200 elements per design.
- Fonts, only these sizes: Sans (mock it with Space Grotesk) 12/14/16/20/24/28/48; Serif (Newsreader) 34/56; Mono (JetBrains Mono) 18/32. At most 2 sizes of 28 px or larger.

CLAWD (mandatory): the official 32x40 pixel sprite, scaled x1-x4 only, default colour #D97757. You may recolour it, add pixel accessories (hat, sweat drop, Zzz...), place it anywhere, make it react to each state, or use it as the gauge. Never reshape the body. Shape (# = filled; the holes in rows 12-15 are the eyes):
rows 0-7:   ..########............########..
rows 8-11:  ################################
rows 12-13: ######..################..######
rows 14-15: ####....################....####
rows 16-31: ################################
rows 32-39: ####....####........####....####

OUTPUT: one HTML page. Each screen is a 240x240 div with absolute, integer-px children, shown at 2x zoom plus a 1x real-size strip. Allowed CSS: background-color, border-radius, 1px solid border, clip-path polygon, hard-stop repeating-linear-gradient for stripes, SVG crispEdges rects for sprites, @keyframes with steps() for animation. For each design: a title "#NN Name · id", the 4 labelled states, and a spec card of at most 10 lines (background, palette hex, fonts and sizes, key element px positions, data-driven parts, animation, per-state changes). End with a yes/no self-check per design.

VARIETY: the 10 designs must differ in layout, using at least 6 of: hero number, two bars, columns, segmented ring, split screen, Clawd as the gauge, scene/level, card stack, ticker, timeline. Each style must be recognisable without its title. Keep it bold and simple: the screen is tiny.

Do not ask questions. If a rule is unclear, choose what is easier to draw and note it in the spec card.

THIS BATCH (4/10): Instruments & technical
#31 Analog VU meter: cream dial, gated needle positions, red zone.
#32 Oscilloscope: green graticule, trace, channel labels.
#33 Aircraft PFD: altitude and speed tapes, horizon band.
#34 Car fuel gauge: segmented E-F arc, warning lamp when hot.
#35 Patient monitor: ECG-style lines, vitals boxes.
#36 Server rack panel: rows of status LEDs, port labels.
#37 Weather station: barometer dial, sun/cloud pixel icons.
#38 Power bank: draining battery cells, charge bolt.
#39 Lab glassware: beaker and test tube liquid levels.
#40 Sonar screen: green rings, gated sweep segments, blips.
```
