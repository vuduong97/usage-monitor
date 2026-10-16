# Plan: 5–8 Clawd animations per usage screen, suited to each screen

Source: ak:advise 2026-10-15 (conversation), reframing confirmed by the user. Mode: cook --auto.

## Contract
- **Outcome:** every one of the 46 style packs plays 5–8 official Clawd animations, theme ones first where they fit.
- **Constraints:**
  - one Clawd size per screen; cell 3 packs may go to cell 2 only when a theme anim needs it; never below cell 2;
  - no firmware change (fwSeq 8, API 2): only `tools/style` and the rebuilt packs;
  - packs ≤ 20,480 B; no animation over text or data.
- **Non-goals:** per-state sets, per-anim scale, layout changes, cell 1.
- **Acceptance:**
  1. Build log: every pack has 5–8 names.
  2. The compiler's fit check reports 0 overlaps of any listed animation with text or data-bound ops (same states) and 0 cells off screen.
  3. The preview runs a full cycle per pack and asserts every listed name played; `styles: ok`, `style anims: ok (46 packs)`; the full `run.sh` passes.
  4. Largest pack ≤ 20,480 B; no cell < 2; `git diff firmware/src` is empty.
  5. On the clock: `styleFrameMaxUs` ≤ 100 ms for the packs with scene animations.

## Design
- **Fit check** (`style_pack.js`):
  - The cell union of every frame of each `/clawd.pak` animation, placed at the anim op's stage origin × cell and swept over its `dx` binding range.
  - It is tested against the boxes of protected ops: every text op (line box, worst-case token widths, track offsets) and every op bound to data (`dx`/`x1`/gate). Ops exclusive with the anim op (state bits or gates) are skipped. Cells off the 240×240 screen also count.
- **`build.js`:** fails a pack when its set has fewer than 5 names or the fit check reports anything. `--fit` prints, per pack, which of the 17 fit at the current cell and at cell 2.
- **Preview:** a per-pack run until the list wraps (cap 240 s), checking every name played; per-anim footprints when `CLAWD_STYLE_ONLY` is set.
- **`anims.js`:** new sets from the fit table, theme first, interleaved with small loops; idle stays 8 s.

## Phases
- [ ] fit check + build guards + `--fit` report
- [ ] preview: full cycle, every name played
- [ ] anims.js: 5–8 per pack from the fit table
- [ ] build, preview, size, visual check of scene packs
- [ ] docs (STYLE_PACK.md), review + test subagents, journal, commit
- [ ] on clock: scene packs' `styleFrameMaxUs`
