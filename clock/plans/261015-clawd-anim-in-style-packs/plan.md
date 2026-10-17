# Plan: official Clawd animations in the style packs

Source: advice 2026-10-15 (conversation). Mode: cook --auto.

## Contract
- Outcome: in all 46 usage-screen style packs, Clawd moves with the official
  animations already on the clock (`/clawd.pak`, 17 anims ported from Clawdmeter),
  in the pack's own colours, each pack with its own set.
- Constraints: firmware change allowed (STYLE_API 2, fwSeq bump, OTA). ESP8266,
  no framebuffer, image <= 520 KB, packs <= 20 KB (compiler cap). Never redraw or
  retouch Clawd frames.
- Non-goals: animations tied to usage state, desk view / idle picks, the official
  orange, new art.
- Acceptance:
  1. All 46 packs build (<= 20 KB each) and validate; every pack's with-data Clawd
     is a K_ANIM op, the no-data (sleeping) Clawd stays static.
  2. Colours come from the pack palette (role map: body, shade, ink, grey, light, accent).
  3. Missing/invalid `/clawd.pak` or unknown anim name -> the old static mask is drawn.
  4. Host preview: styles ok, animation repaints happen, worst repaint reasonable.
  5. `pio run -e clawd` image <= 532,480 B.

## Design
- Firmware (`StylePack.cpp`): op kind 6 `K_ANIM`. Fields: x,y = screen origin of the
  55x37 art stage; a = px per cell (1..8); w = string offset of "name,name,..."; h =
  idle ms between passes; color/b/c/x3 lo/x3 hi/y3 lo = palette index of the roles
  body/shade/ink/grey/light/accent. One player: the first shown K_ANIM op plays; it
  holds frame 0 of the current animation for `idle` ms, plays one pass, moves to the
  next name. Frames are read one at a time from `/clawd.pak` (ClawdPack). Official
  colours map to roles by RGB. Condition bit 10 `anim` = every K_ANIM name resolves in
  the current clawd.pak (rechecked when the pak changes).
- Compiler: `Style.animateClawd(cfg)` turns every with-data Clawd mask group into a
  K_ANIM op (`need anim`) and gates the old masks `!anim`. Config per pack in
  `tools/style/anims.js`.

## Phases
- [x] firmware op + player + validation + API 2 + fwSeq 8
- [x] compiler + anims.js for 46 packs, scene layouts where there is room
- [x] host preview checks (fallback, frames advance)
- [x] build + size + preview verify (image 504,720 B, RAM 46,028 B; largest pack tufte 18,892 B; preview all ok)
- [x] docs (STYLE_PACK.md, FIRMWARE.md, PROTOCOL.md)
- [x] code review + independent test pass (review fixes: API 1 still accepted, player retry/fallback, pak w*h check, drawn-box erase, idle int16, compiler exclusivity + max 8, preview asserts a drawn Clawd)
- [x] journal (plans/journals/2026-10-15-official-clawd-animations-in-the-style-packs.md)
- [ ] on-device: measure `styleFrameMaxUs`, 24 h soak (`minHeap` >= 24,000 B); not possible from this machine
