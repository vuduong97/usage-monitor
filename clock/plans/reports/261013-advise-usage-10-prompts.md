# 10 prompt cho Claude Design: 100 phong cách màn hình usage của đồng hồ Clawd

Mỗi prompt đứng riêng, đủ thông tin. Cách dùng mỗi lượt:

1. Mở một cuộc trò chuyện mới trong Claude Design.
2. Đính kèm `design/usage-screen-preview/style-6-sheet.png`.
3. Dán nguyên một khối prompt bên dưới.

Nếu một mẫu vẽ sai luật, nhắn tiếp trong cùng cuộc trò chuyện: `Redo #NN only. It breaks this rule: <mô tả>. Keep everything else.`

## Prompt 1/10: Máy tính & thiết bị cũ (#1–10)

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

THIS BATCH (1/10): Retro computers & devices
#1 Amber CRT terminal: amber on black, scanline stripes, blinking block cursor.
#2 VT100 green TUI: green on black, ASCII box borders, menu rows.
#3 Macintosh 1984: 1-bit window, striped title bar, close box, dithered desktop.
#4 Windows 95 dialog: teal desktop, grey bevelled dialog, blue block progress bars.
#5 BIOS setup: blue background, grey/yellow text, one highlighted row.
#6 Commodore 64: light blue on dark blue, thick border frame, READY. prompt.
#7 ZX Spectrum: black, rainbow corner stripes, loading-border stripes.
#8 Casio digital watch: grey-green LCD, mono digits with ghost segments.
#9 Pocket calculator: dark body, recessed LCD, hint of a button grid.
#10 Hi-fi VFD display: cyan-teal on black, dotted grid, spectrum columns.
```

## Prompt 2/10: In ấn & giấy (#11–20)

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

THIS BATCH (2/10): Print & paper
#11 Swiss International poster: strict grid, huge number, red/black/white.
#12 Bauhaus: primary circle, triangle, square; red/yellow/blue on cream.
#13 Risograph two-colour: pink + blue overprint, deliberate misregistration.
#14 Thermal receipt: itemised lines (5H SESSION .... 42%), dashed rules, mono.
#15 Library index card: ruled lines, typewriter mono, DUE 16:19 stamp box.
#16 Boarding pass: perforated stub, gate/seat-style fields.
#17 Split-flap departure board: black flaps, white mono, yellow accent.
#18 Gig ticket: torn edge, bold headliner type.
#19 Tear-off calendar page: one big day page, rings on top.
#20 Topographic map: contour lines, legend box, elevation = usage.
```

## Prompt 3/10: Game & đồ chơi (#21–30)

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

THIS BATCH (3/10): Games & toys
#21 Tetris: falling blocks, stack height = 5H usage.
#22 Pinball dot-matrix: orange dot-pattern display, score-style numbers.
#23 Pac-Man maze: Clawd eats pellets, pellets left = what is left.
#24 JRPG status window: blue window, CLAWD Lv.5, HP/MP bars.
#25 Arcade high-score table: red/yellow on black, 1UP, ranked rows.
#26 Space Invaders: invader rows, Clawd as the cannon.
#27 Tamagotchi: egg shell frame, small LCD, Clawd as the pet, hearts = usage.
#28 Platformer level: Clawd runs toward a flag by %.
#29 Chess clock: two clock faces side by side, 5H and WEEK.
#30 Racing HUD: segmented speedometer, lap and position counters.
```

## Prompt 4/10: Đồng hồ đo & kỹ thuật (#31–40)

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

## Prompt 5/10: Tối giản & UI hiện đại (#41–50)

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

THIS BATCH (5/10): Minimal & modern UI
#41 Hairline minimal: two numbers, hairline rules, lots of space.
#42 Phone widget: rounded cards on light grey.
#43 Material tonal: big rounded pills, tonal palette.
#44 Neo-brutalist web: thick black borders, flat yellow, raw layout.
#45 Soft UI: neumorphic feel faked with 1 px light/dark edges only.
#46 OLED black: pure black, one thin accent colour.
#47 E-reader ink: grey ink on paper white, serif.
#48 Tufte data-ink: sparkline-style marks, minimal ink.
#49 Frosted panels: frosted look faked with pre-mixed flat colours.
#50 Big Type: one giant number filling ~70% of the screen.
```

## Prompt 6/10: Dễ thương & nhân vật (#51–60)

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

THIS BATCH (6/10): Cute & character (not like the existing Pastel kawaii)
#51 Bento box: compartments, each holding a stat.
#52 Sticker sheet: die-cut white outlines around every element.
#53 Clawd's bedroom: room scene, desk lamp, wall poster.
#54 Cloud & rainbow: rainbow bands fill with usage.
#55 Candy shop: striped candy jars, level = usage.
#56 Bubble tea: tapioca pearls, tea level = usage.
#57 Plushie: dashed stitch borders, felt colours.
#58 Comic panel: halftone pattern, speech balloon with the number.
#59 Toy blocks: chunky letter blocks spelling the stats.
#60 Pixel garden: flowers grow taller with usage.
```

## Prompt 7/10: Thiên nhiên & mùa (#61–70)

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

THIS BATCH (7/10): Nature & seasons
#61 Aquarium: water level, fish, Clawd underwater with bubbles.
#62 Night sky: twinkling stars, moon phase = weekly usage.
#63 Desert dunes at sunset: banded sky, dune layers.
#64 Campfire: logs, stepped flames, stars.
#65 Ocean tide: tide line on a pier post.
#66 Autumn: falling leaves pile up with usage.
#67 Winter: snow pile height, snowflakes.
#68 Spring sakura: petals on a branch.
#69 Volcano: lava rises, erupts when hot.
#70 Mountain hike: altitude profile, Clawd climbs by %.
```

## Prompt 8/10: Khoa học viễn tưởng (#71–80)

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

THIS BATCH (8/10): Sci-fi & future
#71 Starship LCARS-like panels: rounded elbows, orange and lilac blocks.
#72 Mars mission control: rust tones, telemetry rows.
#73 Glitch HUD: magenta/cyan offset copies, corner brackets.
#74 Mech cockpit: hazard frames, targeting brackets around Clawd.
#75 Flat radar hologram: concentric lines, blips.
#76 Code rain: green character columns, bright column heads.
#77 Airlock: diagonal hazard stripes, pressure bars.
#78 Atomic-age retro-futurism: boomerangs, starbursts, turquoise/orange.
#79 Solarpunk: greens and gold, leaves, solar panels.
#80 Deep space: planets on orbits, orbit position = time elapsed.
```

## Prompt 9/10: Trào lưu nghệ thuật (#81–90)

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

THIS BATCH (9/10): Art movements & design styles
#81 Art Deco: gold on black, sunburst lines, symmetry.
#82 Memphis 80s: squiggles, confetti triangles, pastel with black.
#83 De Stijl: Mondrian grid of primary blocks, block size = usage.
#84 Pop Art: Ben-Day dots, thick black outlines.
#85 Constructivist: red/black diagonals, bold blocks.
#86 Ukiyo-e wave: flat layered wave bands, indigo and cream.
#87 Op Art: black/white stripes bending around the numbers.
#88 Y2K Aero: bubbly rounded panels, aqua and lime, flat.
#89 Stained glass: black leading lines, jewel-colour cells.
#90 Scandinavian folk: patterned borders, red/white, simple flowers.
```

## Prompt 10/10: Việt Nam (#91–100)

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

THIS BATCH (10/10): Vietnam (on-screen text stays English or unaccented words like TET, PHIN)
#91 Tet: red and gold, li xi envelope, firecrackers, apricot blossoms.
#92 Son mai lacquer: black and red lacquer, gold and eggshell flecks.
#93 Dong Ho woodblock print: warm paper, bold outlines, folk motifs around Clawd.
#94 Chu Dau / Bat Trang blue-and-white ceramics: cobalt on cream, patterned rims.
#95 Old Saigon shop signs: hand-painted sign blocks, red/yellow/blue.
#96 Hoi An lanterns at night: lanterns light up with usage.
#97 Phin coffee: drip coffee level = usage, condensed-milk layer.
#98 Rice terraces & non la: terrace layers = weekly usage.
#99 Vintage Vietnamese postage stamp: perforated border, % as the denomination.
#100 Moped speedometer (Cub/Dream era): odometer digits, gated needle.
```
