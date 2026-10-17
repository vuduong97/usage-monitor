# Prompt cho Claude Design: 100 phong cách màn hình usage của đồng hồ Clawd

Cách dùng (tiếng Việt):
1. Mỗi lượt mở **một cuộc trò chuyện mới** trong Claude Design.
2. Đính kèm ảnh `design/usage-screen-preview/style-6-sheet.png`: đây là 6 mẫu đã có, Claude Design không được lặp lại.
3. Dán **toàn bộ phần "MASTER PROMPT"** bên dưới.
4. Thêm một dòng `BATCH: A` (lần lượt A → J) ở cuối.
5. Kết quả mỗi lượt: 10 mẫu × 4 trạng thái + thẻ thông số.

Tại sao mỗi lượt mở cuộc trò chuyện mới: 40 màn hình HTML mỗi lượt rất dài. Nếu dồn 10 lượt vào một cuộc, các luật ở đầu prompt dễ bị quên ở những lượt cuối.

---

## MASTER PROMPT (copy everything between the lines)

---

You are designing usage screens for **Clawd**, a tiny desk clock that shows how much of a person's Claude usage limits they have used. I need **100 distinct visual styles** in total. You will make them in 10 batches of 10. This message tells you which batch to make (the `BATCH:` line at the end). Make only that batch.

Every design must be buildable later as a "style pack": a small file the clock's ESP8266 firmware draws with a handful of primitives. So the hard rules below are not taste. They are what the hardware can draw. A design that breaks them is wasted.

### 1. The device

- 1.54-inch square IPS panel, **240 × 240 px**, RGB565 colour (16-bit). It sits on a desk about 50–70 cm from the eyes.
- Keep a **6 px safe margin** on every edge for important content. Background art may run to the edge.
- There is no framebuffer and the CPU is slow. A full repaint takes 200–450 ms, so only small areas may animate.

### 2. What every screen MUST show (in all states)

1. **5-hour session usage, %**: the most prominent number on the screen.
2. **Weekly (7-day) usage, %**: clearly secondary, but readable at a glance.
3. **The current time** (HH:MM).
4. **Clawd**, the mascot (section 5).

Everything else is your creative choice. Optional extras include when each window resets, time left, a pace or forecast line, the weekday, labels and decoration. Use the extras only where they fit; never cram.

Legibility floor:
- The 5-hour % is at least 28 px tall.
- No text is smaller than 12 px.
- Text has strong contrast with what is behind it.

### 3. The four states to draw for every design

Draw each design 4 times with this exact sample data. The labels in brackets are the data tokens the firmware can print (section 6).

| State | Time | 5H % | Week % | 5H resets [sr@] / in [sr~] | Week resets [wr@d] / in [wr~] | Forecast | Flags |
|---|---|---|---|---|---|---|---|
| **1 Normal** | Mon 14:05 | 42 | 67 | 16:19 / 2h 14m | Thu 08:05 / 2d 18h | lasts to reset, ends near 76% [end] | — |
| **2 Hot** | Mon 14:05 | 88 | 93 | 14:43 / 38m | Mon 23:05 / 9h 0m | out at ~14:41 [out] | hot, whot, paceover, out |
| **3 Fresh** | Mon 09:12 | 4 | 18 | 14:03 / 4h 51m | Sun 01:52 / 5d 16h | — | fresh (5-hour window just reset) |
| **4 No data** | Mon 14:05 | -- | -- | -- | -- | — | not ok (desktop app not sending) |

Time elapsed in each window (for time markers): Normal 5H 55%, Week 61%. Hot 5H 87%, Week 95%. Fresh 5H 3%, Week 19%.

How the states should feel:
- **Hot** must read as urgent at a glance, through colour, Clawd's reaction, or both.
- **Fresh** feels like a clean start.
- **No data** keeps the layout but shows `--`, dims or greys the data. Clawd looks asleep or waiting, and an optional short line says something like "waiting for app".

### 4. Hard drawing rules (the firmware's only primitives)

Allowed elements:
- **Filled rectangle**: solid colour, optional corner radius (integer px). An optional repeating 1-colour **pattern** fills it instead of a solid fill: diagonal lines, vertical stripes, or horizontal stripes (stripe every N px, M px wide). Patterns stand in for textures, scanlines, hatching and halftone.
- **Rectangle outline**, 1 px.
- **Line**, 1 px, anti-aliased, any angle. A thicker line has to be a rectangle or two triangles.
- **Triangle**, filled.
- **Filled circle (disc)**.
- **Text** from the fonts below, left, centre or right aligned, ASCII only.
- **1-bit pixel sprite** (a "mask"): any size, one colour per layer, scaled by an integer (×1, ×2, ×3…). Clawd, icons and accessories are masks. A multi-colour sprite is several masks stacked.

Each element has exactly ONE flat colour. Keep each design to **≤ 12 colours** and pick colours that stay distinct in 16-bit colour. Nothing on the screen may be visually closer than about 8 levels per channel to its neighbour.

Data-driven geometry (this is how the data moves things):
- An element's **width** can be `span × value` (for example a bar 0…208 px wide by 5H %).
- An element's **x position** can be `span × value` (for example Clawd walking right as the week fills up).
- The values available are 5H %, week %, and time elapsed in each window.
- An element can be **shown only while a value is in a range**, for example "this segment lights when 5H ≥ 30%".
- Rings, arcs, dials and needles are built from **pre-drawn pieces gated by range**: at most 40 ring segments, or at most 12 needle positions. There is no arc primitive and no rotation.
- An element can be **shown only in some states**: hot, whot (week ≥ 80%), fresh, paceover, out, no-data and so on.

**Animation** is a list of steps on a timer. Each step can move an element by whole pixels (dx, dy within ±127), show or hide it, or swap its colour.
- Steps are ≥ 100 ms apart.
- Animate at most 3 small things at once, inside a combined area of about 120 × 60 px. Good uses: a blink, a bob, a sweat drop, a twinkle, a marquee dot.
- Never animate large areas or the background.

Size budget:
- ≤ 200 elements per design. Every stripe band, segment and gradient step counts as one.
- At most 2 fonts of 28 px or larger, used only for numbers and the clock.

**Forbidden:** blur, glow filters, box-shadow, text-shadow, opacity / transparency, blend modes, soft gradients, photos or bitmaps, emoji, icon fonts, rotation of text or elements, non-integer coordinates, accented or non-ASCII characters.
- Fake a glow with a dimmer offset copy.
- Fake a gradient with 3–8 flat colour bands.
- Fake transparency with a pre-mixed flat colour.

### 5. Clawd (mandatory, keep the official silhouette)

Clawd is a 32 × 40 pixel 1-bit sprite. The shape must stay exactly this (`#` = filled, `.` = empty; the two holes in rows 12–15 are the eyes):

```
..########............########..   row 0
..########............########..
..########............########..
..########............########..
..########............########..
..########............########..
..########............########..
..########............########..   row 7
################################   row 8
################################
################################
################################
######..################..######   row 12
######..################..######
####....################....####
####....################....####   row 15
################################   rows 16-31 (16 identical rows)
...
################################
####....####........####....####   rows 32-39 (8 identical rows: four legs)
...
####....####........####....####
```

Rules:
- Draw Clawd only at integer scale: ×1 (32×40), ×2 (64×80), ×3 (96×120) or ×4 (128×160), with crisp pixels.
- Default colour is terracotta **#D97757**. A style may recolour Clawd to fit its palette, as long as it stays clearly visible.
- Built-in derived layers you can colour separately: the eyes (holes), a 1-px outline, and the inner fill. This lets you do an outlined Clawd, a two-tone Clawd, or eyes that light up.
- You MAY add **pixel accessories as separate 1-bit sprites**: hat, glasses, headphones, sweat drop, Zzz, sparkles, a scarf, a lantern held, and so on.
- You MAY place Clawd anywhere, at any of the 4 sizes, and have it react per state. For example: blinking at normal, sweating or alarmed at hot, celebrating at fresh, asleep or grey at no-data.
- You MAY make Clawd part of the data display: it walks along a track by %, stands on a rising level, or carries a bar.
- Never redraw, round, stretch, rotate or mirror the body itself.

### 6. Fonts (only these, only these sizes)

| Role | Real firmware font | Use this stand-in in your mock | Sizes (px) |
|---|---|---|---|
| Sans | Styrene A | Space Grotesk | 12, 14, 16, 20, 24, 28, 48 |
| Serif | Tiempos | Newsreader | 34, 56 |
| Mono | Clawd mono | JetBrains Mono | 18, 32 |

- No other families or sizes, no faux bold or italic, no letter-spacing tricks.
- An all-caps style is allowed.

Printable data tokens: 5H %, week %, 5H % left, clock (HH:MM), weekday (Mon…Sun), 5H reset time, week reset time (with or without the day), time until each reset (`2h 14m`, `5d 16h`), forecast "out at ~HH:MM", forecast end %. Any other text must be fixed English ASCII words.

### 7. How to build the mock (so it matches the hardware)

- One HTML page for the whole batch.
- Each screen is a `240×240` div with `position:relative; overflow:hidden`. Every child is `position:absolute` at integer px.
- Show each screen at **2× zoom** (wrap it in `transform:scale(2)` with the transform origin at top left) so pixels can be judged. Also add one **1× real-size strip** of the four states per design, to judge legibility.
- Allowed CSS:
  - `background-color`;
  - `border-radius`;
  - 1 px solid borders;
  - `clip-path: polygon()` for triangles;
  - `repeating-linear-gradient` **with hard stops only** to imitate the stripe patterns;
  - colours, fonts and sizes from the lists above.
- Draw Clawd, accessories and icons as inline SVG `<rect>` pixels with `shape-rendering="crispEdges"`.
- Animations use CSS `@keyframes` with `steps()`: integer translate, visibility or colour only.
- None of the forbidden CSS: no `box-shadow`, `filter`, `opacity`, `text-shadow`, `mix-blend-mode`, soft gradients, `rotate`, or images.

### 8. Output for the batch

1. A header with the batch letter and theme.
2. Then for each of the 10 designs, one row with:
   - **Title**: `#NN Name`, plus a short id (lowercase, ≤ 15 chars, e.g. `tetris`);
   - the **4 states** side by side at 2×, each labelled Normal / Hot / Fresh / No data;
   - the **1× strip**;
   - a compact **spec card** in monospace, ≤ 15 lines, in this shape:

```
#21 Tetris · id tetris · bg #0B0B1E · light: no
Palette: #0B0B1E #F2F2F2 #D97757 #3FA7FF #FFD23F #FF4D4D (6)
Fonts: Mono 32 (5H %), Sans 14 (labels), Sans 20 (clock)
5H %: text Mono 32 #F2F2F2 right-aligned at x=224 y=20
5H bar: 10 blocks 18x18 at x=16..196 y=60, block k lit when 5H >= k*10%
Week: text Sans 20 at (16,200); thin bar x=16 y=228 h=4 width = 208 x week
Clawd: x2 #D97757 at (150,96); hot: + sweat drop 6x8 #3FA7FF, red eyes
Animation: Clawd eye blink (eyes recoloured 150 ms every 3 s); falling block dy +4 every 250 ms (16x16 area)
States: hot -> blocks #FF4D4D; fresh -> confetti tri x6; nodata -> all data #555, "--", Clawd grey
Elements ~95
```

3. After the 10 rows, a **self-check table**, one line per design: all 4 states drawn? 5H %, week %, clock and Clawd visible in every state? Only allowed primitives and CSS? ASCII only? ≤ 12 colours? ≤ 200 elements? Fix any "no" before you answer.

### 9. Variety rules

- Each design must differ in **layout structure**, not only in colours. Across each batch of 10, use at least 6 of these layout archetypes:
  1. giant hero number;
  2. two horizontal bars;
  3. two vertical columns or thermometers;
  4. segmented ring or dial (gated segments);
  5. split screen (left/right or top/bottom);
  6. Clawd as the gauge (walks or climbs by %);
  7. a scene where a level, height or count shows usage;
  8. a card or ticket stack;
  9. a ticker or segment display;
  10. a timeline with a "now" marker (time elapsed).
- Do not reproduce the 6 existing styles in the attached sheet: Game Boy, Synthwave, Newspaper, Blueprint, Pastel kawaii, 1-bit Playdate.
- Be faithful to each style's own visual language: its typography choice from the 3 families, its palette, framing and ornaments. A viewer should name the style without reading the title.
- Prefer bold, simple compositions over detail: the screen is 1.54 inches.

### 10. The 100 styles

**A — Retro computers & devices**
1. Amber CRT terminal: amber phosphor on black, scanline stripe pattern, blinking block cursor.
2. VT100 green TUI: green on black, ASCII box borders, menu-like rows.
3. Macintosh 1984: 1-bit window with a striped title bar and close box, dithered desktop.
4. Windows 95 dialog: teal desktop, grey bevelled dialog (1 px light/dark edges), blue block progress bars.
5. BIOS setup: blue background, grey/yellow text, one highlighted row.
6. Commodore 64: light blue on dark blue inside a thick border frame, `READY.` prompt.
7. ZX Spectrum: black, rainbow corner stripes, loading-border stripes.
8. Casio digital watch: grey-green LCD, mono digits with ghost segments, tiny mode labels.
9. Pocket calculator: dark body, recessed LCD, hints of a button grid.
10. Hi-fi VFD display: cyan-teal segments on black, dotted grid, spectrum-analyser columns.

**B — Print & paper**
11. Swiss International poster: strict grid, huge number, red/black/white.
12. Bauhaus: primary circle, triangle and square, red/yellow/blue on cream.
13. Risograph two-colour: pink + blue overprint with deliberate misregistration (offset copies).
14. Thermal receipt: itemised lines (`5H SESSION .... 42%`), dashed rules, mono.
15. Library index card: ruled lines, typewriter mono, a "DUE 16:19" stamp box.
16. Boarding pass: stub with perforation dots, gate/seat-style fields.
17. Split-flap departure board: black flaps with split lines, white mono, yellow accent.
18. Gig ticket: torn edge (triangles), bold headliner type.
19. Tear-off calendar page: one big day-page, rings at the top.
20. Topographic map: contour lines, legend box, elevation = usage.

**C — Games & toys**
21. Tetris: falling blocks; stack height = 5H usage.
22. Pinball dot-matrix: orange dot-pattern display, score-style numbers.
23. Pac-Man maze: Clawd eats pellets; pellets left = what's left.
24. JRPG status window: blue window, "CLAWD Lv.5", HP/MP bars.
25. Arcade high-score table: red/yellow on black, `1UP`, ranked rows.
26. Space Invaders: invader rows above, Clawd as the cannon.
27. Tamagotchi: egg-shaped shell frame, small LCD, Clawd as the pet, hearts = usage.
28. Platformer level: Clawd runs along the ground toward a flag by %.
29. Chess clock: two clock faces side by side, 5H and WEEK.
30. Racing HUD: segmented speedometer, lap and position counters.

**D — Instruments & technical**
31. Analog VU meter: cream dial, gated needle positions, red zone.
32. Oscilloscope: green graticule, trace, channel labels.
33. Aircraft PFD: altitude and speed tapes, horizon band.
34. Car fuel gauge: segmented E–F arc, warning lamp at hot.
35. Patient monitor: ECG-style lines, vitals boxes.
36. Server rack panel: rows of status LEDs, port labels.
37. Weather station: barometer dial, sun/cloud pixel icons.
38. Power bank: battery cells that drain, charge bolt.
39. Lab glassware: beaker and test tube liquid levels.
40. Sonar screen: green concentric rings, gated sweep segments, contact blips.

**E — Minimal & modern UI**
41. Hairline minimal: two numbers, hairline rules, lots of empty space.
42. Phone widget: rounded cards on light grey, small captions.
43. Material tonal: big rounded pills, a tonal palette.
44. Neo-brutalist web: thick black borders, flat yellow, raw layout.
45. Soft UI: a neumorphic feel faked with 1 px light/dark offset edges only.
46. OLED black: pure black, one thin accent colour.
47. E-reader ink: grey ink on paper white, serif.
48. Tufte data-ink: sparkline-style marks, minimal ink.
49. Frosted panels: the frosted look faked with pre-mixed flat panel colours.
50. Big Type: one giant number filling about 70% of the screen.

**F — Cute & character** (not like the existing Pastel kawaii style)
51. Bento box: compartments, each holding a stat.
52. Sticker sheet: die-cut white outlines around every element.
53. Clawd's bedroom: a room scene, desk lamp, poster on the wall.
54. Cloud & rainbow: rainbow bands fill with usage.
55. Candy shop: striped candy jars, candy level = usage.
56. Bubble tea: tapioca pearls and a tea level = usage.
57. Plushie: dashed stitch borders, felt colours.
58. Comic panel: halftone pattern, a speech balloon with the number.
59. Toy blocks: chunky letter blocks spelling the stats.
60. Pixel garden: flowers grow taller with usage.

**G — Nature & seasons**
61. Aquarium: water level, fish, Clawd underwater with rising bubbles.
62. Night sky: stars twinkle, moon phase = weekly usage.
63. Desert dunes at sunset: banded sky, dune layers.
64. Campfire: logs, flame steps, stars.
65. Ocean tide: tide line on a pier post.
66. Autumn: falling leaves pile up with usage.
67. Winter: snow pile height, snowflakes.
68. Spring sakura: petals on a branch.
69. Volcano: lava rises; erupts at hot.
70. Mountain hike: altitude profile, Clawd climbs by %.

**H — Sci-fi & future**
71. Starship LCARS-like panels: rounded elbows, orange and lilac blocks.
72. Mars mission control: rust tones, telemetry rows.
73. Glitch HUD: magenta/cyan offset copies, corner brackets.
74. Mech cockpit: hazard frames, targeting brackets around Clawd.
75. Flat radar hologram: concentric lines, blips.
76. Code rain: green character columns, one bright head per column.
77. Airlock: diagonal hazard stripes, pressure bars.
78. Atomic-age retro-futurism: boomerangs, starbursts, turquoise and orange.
79. Solarpunk: greens and gold, leaves, solar panels.
80. Deep space: black, planets on orbits, orbit position = time elapsed.

**I — Art movements & design styles**
81. Art Deco: gold on black, sunburst lines, symmetry.
82. Memphis 80s: squiggles, confetti triangles, pastel with black.
83. De Stijl: a Mondrian grid of primary blocks, block size = usage.
84. Pop Art: Ben-Day dot pattern, thick black outlines.
85. Constructivist: red/black diagonals, bold blocks.
86. Ukiyo-e wave: flat layered wave bands, indigo and cream.
87. Op Art: black/white stripe patterns that bend around the numbers.
88. Y2K Aero: bubbly rounded panels, aqua and lime (flat, no gloss).
89. Stained glass: black leading lines, jewel-colour cells.
90. Scandinavian folk: patterned borders, red/white, simple flowers.

**J — Vietnam** (on-screen text stays English or unaccented ASCII words, such as TET or PHIN)
91. Tết: red and gold, a lì xì envelope, firecrackers, apricot blossoms.
92. Lacquer (sơn mài): black and red lacquer, gold and eggshell flecks.
93. Đông Hồ woodblock print: warm paper, bold outlines, folk-print motifs around Clawd.
94. Chu Đậu / Bát Tràng blue-and-white ceramics: cobalt on cream, patterned rims.
95. Old Saigon shop signs: hand-painted sign blocks, red/yellow/blue.
96. Hội An lanterns at night: lanterns light up with usage.
97. Phin coffee: drip coffee level = usage, a condensed-milk layer.
98. Rice terraces & nón lá: terrace layers = weekly usage.
99. Vintage Vietnamese postage stamp: perforated border, the % as the denomination.
100. Moped speedometer (Cub/Dream era): odometer digits, gated needle.

Batch mapping: A = #1–10, B = #11–20 … J = #91–100.

Start now with the batch named below. Do not ask questions first. Where a rule is ambiguous, pick the option that is easier for the firmware to draw and note it in the spec card.

BATCH: A

---

## Câu lệnh cho từng lượt

Mỗi lượt: cuộc trò chuyện mới + ảnh tham chiếu + MASTER PROMPT. Chỉ đổi dòng cuối:

| Lượt | Dòng cuối | Nhóm |
|---|---|---|
| 1 | `BATCH: A` | Máy tính & thiết bị cũ (#1–10) |
| 2 | `BATCH: B` | In ấn & giấy (#11–20) |
| 3 | `BATCH: C` | Game & đồ chơi (#21–30) |
| 4 | `BATCH: D` | Đồng hồ đo & kỹ thuật (#31–40) |
| 5 | `BATCH: E` | Tối giản & UI hiện đại (#41–50) |
| 6 | `BATCH: F` | Dễ thương & nhân vật (#51–60) |
| 7 | `BATCH: G` | Thiên nhiên & mùa (#61–70) |
| 8 | `BATCH: H` | Khoa học viễn tưởng (#71–80) |
| 9 | `BATCH: I` | Trào lưu nghệ thuật (#81–90) |
| 10 | `BATCH: J` | Việt Nam (#91–100) |

Nếu một mẫu bị vẽ sai luật, gửi tiếp trong cùng cuộc trò chuyện:
`Redo #NN only. It breaks rule <số mục>: <mô tả ngắn>. Keep everything else.`
