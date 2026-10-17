// #31 Analog VU meter: a cream dial with a 12-position needle under a dark shroud, a
// peak lamp, and Clawd beside the big 5-hour readout (Batch D, archetype 4: dial).
const { mock } = require('../mock');
const { SPR, OK, NO, NORMAL, FRESH, polar, quadTris, bucketGate, drip, clawd } = require('./_d1_common');

const BG = '#17130F', CREAM = '#F1E4C3', BROWN = '#4A3A2C', DIM = '#B3A27F', RED = '#E5463A', CLAWD = '#D97757',
  GREY = '#6E6862', BLUE = '#5AA9E6', GREEN = '#79C27A';
const CX = 120, CY = 200;
const ang = (v) => -36 + 72 * v / 100;

module.exports = mock({
  id: 'vumeter', name: 'VU meter', bg: BG,
  draw(b) {
    b.R(8, 8, 224, 132, CREAM, { r: 6 });
    // scale: an arc to 80 %, a red band for 80-100 %, 11 ticks and 5 labels
    for (let i = 0; i < 6; i++) {
      const [x0, y0] = polar(CX, CY, 146, ang(i * 80 / 6)), [x1, y1] = polar(CX, CY, 146, ang((i + 1) * 80 / 6));
      b.S.line(x0, y0, x1, y1, BG);
    }
    for (let i = 0; i < 2; i++) for (const t of quadTris(CX, CY, 145, 152, ang(80 + i * 10), ang(90 + i * 10))) b.P(t, RED);
    for (let i = 0; i <= 10; i++) {
      const [x0, y0] = polar(CX, CY, 146, ang(i * 10)), [x1, y1] = polar(CX, CY, i % 5 === 0 ? 160 : 155, ang(i * 10));
      b.S.line(x0, y0, x1, y1, i >= 8 ? RED : BG);
    }
    for (const v of [0, 25, 50, 75, 100]) {
      const q = polar(CX, CY, 170, ang(v));
      b.T(q[0], q[1] - 7, String(v), 'S12', v === 100 ? RED : BG, 'c');
    }
    b.S.text('5H', 120, 59, 't34', BG, { align: 'center' });   // the sheet's serif (Newsreader 34) y = 62; t34 sits lower
    b.T(204, 96, 'PEAK', 'S12', BG, 'r');
    b.C(214, 102, 5, BROWN);
    b.C(214, 102, 5, RED, { when: 'hot', track: b.blink(500) });
    b.C(214, 102, 5, GREEN, { when: FRESH });
    b.T(112, 100, 'WAITING FOR APP', 'S12', BG, 'c', { when: NO });

    // the needle: 12 positions, k shown while floor(5H * 12 / 100) = k
    for (let k = 0; k < 12; k++) {
      const a = ang((k + 0.5) * 100 / 12);
      const tri = [polar(CX, CY, 78, a - 1.6), polar(CX, CY, 78, a + 1.6), polar(CX, CY, 150, a)];
      b.P(tri, BG, { when: OK, gate: bucketGate('s', k, 12) });
      if (k === 0) b.P(tri, GREY, { when: NO });
    }

    // the shroud hides the pivot
    b.R(8, 114, 224, 26, BROWN);
    b.T(16, 121, 'CLAWD', 'S12', DIM);
    b.T(120, 119, '{clock}', 'S16', CREAM, 'c');
    b.T(224, 121, 'VU', 'S12', DIM, 'r');

    b.T(14, 146, '{s}%', 'S48', RED, 'l', { when: 'hot' });
    b.T(14, 146, '{s}%', 'S48', CREAM, 'l', { when: 'ok !hot' });
    b.T(14, 146, '--%', 'S48', GREY, 'l', { when: NO });
    b.T(16, 198, 'OUT AT {out}', 'S12', RED, 'l', { when: 'hot' });
    b.T(16, 198, '5H RESETS {sr@}', 'S12', DIM, 'l', { when: 'ok !hot' });
    b.T(16, 198, '5H RESETS --:--', 'S12', GREY, 'l', { when: NO });
    b.T(16, 214, 'WEEK {w}%', 'S20', RED, 'l', { when: 'whot' });
    b.T(16, 214, 'WEEK {w}%', 'S20', CREAM, 'l', { when: 'ok !whot' });
    b.T(16, 214, 'WEEK --%', 'S20', GREY, 'l', { when: NO });

    clawd(b, 160, 150, 2, CLAWD, GREY, BG);
    b.Sp(SPR.drop, 148, 150, 2, BLUE, { when: 'hot', track: drip(b) });
    b.Sp(SPR.spark, 140, 150, 2, CREAM, { when: FRESH, track: b.blink(500) });
    b.Sp(SPR.spark, 148, 176, 1, CREAM, { when: FRESH });
    b.Sp(SPR.zz, 138, 146, 2, DIM, { when: NO });
  },
});
