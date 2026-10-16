// #34 Car fuel gauge: a 16-segment arc that empties as the 5-hour window fills (fuel =
// 100 - 5H), a low-fuel pump lamp, a week display and Clawd as the driver
// (Batch D, archetype 4: dial).
const { mock } = require('../mock');
const { SPR, OK, NO, FRESH, quadTris, drip, clawd } = require('./_d1_common');

const BG = '#0A0B0D', WHITE = '#F4F1EA', ORANGE = '#FF8A2A', RED = '#FF3B30', DIM = '#2A2C30', GREY = '#8C9096',
  DISP = '#1A1D22', AMBER = '#FFB52A', CLAWD = '#D97757', CGREY = '#6C7076', BLUE = '#5AA9E6', GREEN = '#5BD87A';
const CX = 120, CY = 120;

module.exports = mock({
  id: 'fuel', name: 'Fuel gauge', bg: BG,
  draw(b) {
    // the arc: an unlit quad under each lit one; segment k is lit while fuel (100 - 5H) >= (k + 0.5) * 6.25
    for (let k = 0; k < 16; k++) {
      const a1 = -100 + 12.5 * k + 1.25, a2 = a1 + 10;
      const tris = quadTris(CX, CY, 82, 100, a1, a2);
      const hi = (1000 - 62.5 * (k + 0.5) + 1) / 1000;   // 5H (in thousandths) <= 1000 - threshold
      for (const t of tris) b.P(t, DIM);
      for (const t of tris) b.P(t, k < 3 ? RED : ORANGE, { when: OK, gate: ['s', 0, hi] });
    }
    for (const a of [-100, 0, 100]) {
      const rad = a * Math.PI / 180;
      b.S.line(Math.round(CX + 103 * Math.sin(rad)), Math.round(CY - 103 * Math.cos(rad)),
        Math.round(CX + 110 * Math.sin(rad)), Math.round(CY - 110 * Math.cos(rad)), WHITE);
    }
    b.T(55, 121, 'E', 'S20', WHITE, 'c');
    b.T(185, 121, 'F', 'S20', WHITE, 'c');
    b.Sp(SPR.pump, 113, 44, 2, DIM, { when: NO });
    b.Sp(SPR.pump, 113, 44, 2, AMBER, { when: 'hot', track: b.blink(500) });
    b.Sp(SPR.pump, 113, 44, 2, GREEN, { when: 'ok fresh !hot' });
    b.Sp(SPR.pump, 113, 44, 2, GREY, { when: 'ok !fresh !hot' });

    b.T(120, 64, '{s}%', 'S48', WHITE, 'c', { when: 'ok !hot' });
    b.T(120, 64, '{s}%', 'S48', RED, 'c', { when: 'hot' });
    b.T(120, 64, '--%', 'S48', CGREY, 'c', { when: NO });
    b.T(120, 114, '5H USED', 'S12', GREY, 'c');
    b.T(120, 142, 'WAITING FOR APP', 'S12', GREY, 'c', { when: NO });
    b.T(120, 142, 'OUT AT {out}', 'S12', AMBER, 'c', { when: 'hot' });
    b.T(120, 142, 'FULL - RESETS {sr@}', 'S12', GREEN, 'c', { when: 'ok fresh !hot' });
    b.T(120, 142, 'RESETS {sr@}  {sr~}', 'S12', GREY, 'c', { when: 'ok !fresh !hot' });

    // the info display: week, a 10-block bar, clock and weekday
    b.R(86, 158, 146, 74, DISP, { r: 6 });
    b.T(94, 164, 'WEEK', 'S12', GREY);
    b.T(224, 160, '{w}%', 'M32', WHITE, 'r', { when: 'ok !whot' });
    b.T(224, 160, '{w}%', 'M32', RED, 'r', { when: 'whot' });
    b.T(224, 160, '--%', 'M32', CGREY, 'r', { when: NO });
    b.R(94, 198, 128, 8, DIM, { p: ['v', 13, 11] });
    for (let k = 0; k < 10; k++) {
      const g = ['w', (k + 0.5) / 10];
      b.R(94 + 13 * k, 198, 11, 8, ORANGE, { when: 'ok !whot', gate: g });
      b.R(94 + 13 * k, 198, 11, 8, RED, { when: 'whot', gate: g });
    }
    b.T(94, 210, '{clock}', 'S20', WHITE);
    b.T(224, 216, '{dow}', 'S12', GREY, 'r', { upper: true });

    clawd(b, 12, 154, 2, CLAWD, CGREY, BG);
    b.Sp(SPR.drop, 77, 152, 1, BLUE, { when: 'hot', track: drip(b) });
    b.Sp(SPR.spark, 74, 150, 2, GREEN, { when: FRESH });
    b.Sp(SPR.zz, 74, 148, 1, GREY, { when: NO });
  },
});
