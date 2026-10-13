// #39 Lab glassware: a beaker for the 5-hour window and a test tube for the week,
// liquid in 5 % bands, rising bubbles, and Clawd in goggles on the bench (Batch D,
// archetype 7: liquid levels).
const { mock } = require('../mock');
const { SPD, tracks } = require('./_d2_kit');

const BG = '#e9eef0', INK = '#1f2a33', GLASS = '#8fa3ad', GFILL = '#cfdde3', LIQUID = '#2f74d0', RED = '#e0453a';
const BENCH = '#343d46', WHITE = '#f5f7f8', YELLOW = '#ffc93c', CLAWD = '#d97757', GREEN = '#2fa36b';
const OK = 'ok', NO = '!ok', FRESH = 'ok fresh !hot', PLAIN = 'ok !hot !fresh';

module.exports = mock({
  id: 'lab', name: 'Lab glassware', bg: BG, light: true,
  draw(b) {
    const T = tracks(b);
    b.T(12, 6, '{s}%', 'S48', INK, 'l', { when: 'ok !hot' });
    b.T(12, 6, '{s}%', 'S48', RED, 'l', { when: 'hot' });
    b.T(12, 6, '--%', 'S48', GLASS, 'l', { when: NO });
    b.T(14, 56, '5H SESSION', 'S12', INK);
    b.T(182, 12, 'WEEK', 'S12', INK, 'c');
    b.T(182, 28, '{w}%', 'S20', INK, 'c', { when: 'ok !whot' });
    b.T(182, 28, '{w}%', 'S20', RED, 'c', { when: 'whot' });
    b.T(182, 28, '--%', 'S20', GLASS, 'c', { when: NO });

    // the beaker's liquid: band k is full once 5H reaches (k + 0.5) x 5 %
    for (let k = 0; k < 20; k++) {
      const g = { gate: ['s', (k + 0.5) * 5 / 100] };
      b.R(26, 194 - 6 * k, 108, 6, LIQUID, { when: PLAIN, ...g });
      b.R(26, 194 - 6 * k, 108, 6, RED, { when: 'hot', ...g });
      b.R(26, 194 - 6 * k, 108, 6, GREEN, { when: FRESH, ...g });
    }
    b.C(56, 188, 3, WHITE, { when: OK, gate: ['s', 0.3], track: T.rise });
    b.C(100, 180, 2, WHITE, { when: 'hot', track: T.rise });
    // the glass: walls, base, lips, graduations
    b.R(24, 72, 2, 130, GLASS);
    b.R(134, 72, 2, 130, GLASS);
    b.R(24, 200, 112, 2, GLASS);
    b.R(18, 70, 8, 2, GLASS);
    b.R(134, 70, 8, 2, GLASS);
    for (let j = 1; j <= 10; j++) b.R(26, 200 - 12 * j, j % 5 === 0 ? 14 : 8, 1, INK);
    b.T(42, 134, '50', 'S12', INK);
    b.T(42, 74, '100', 'S12', INK);
    b.P([[152, 62], [142, 80], [162, 80]], YELLOW, { when: 'hot' });
    b.T(152, 66, '!', 'S14', INK, 'c', { when: 'hot' });

    // the test tube (week)
    b.R(166, 56, 32, 146, GLASS, { r: 16 });
    b.R(168, 58, 28, 142, GFILL, { r: 14 });
    b.R(162, 54, 40, 4, GLASS, { r: 2 });
    for (let k = 0; k < 20; k++) {
      const g = { gate: ['w', (k + 0.5) * 5 / 100] };
      const [y, h, r] = k === 0 ? [188, 12, 6] : [194 - 6 * k, 6, 0];
      b.R(170, y, 24, h, LIQUID, { r, when: 'ok !whot', ...g });
      b.R(170, y, 24, h, RED, { r, when: 'whot', ...g });
    }
    b.R(160, 170, 44, 6, INK);
    b.R(162, 176, 4, 28, INK);
    b.R(198, 176, 4, 28, INK);

    // Clawd on the bench in lab goggles
    b.M('clawd', 202, 164, 1, CLAWD, { when: OK });
    b.M('clawd', 202, 164, 1, GLASS, { when: NO });
    b.eyes(202, 164, 1, BG);
    b.Sp(SPD.glasses, 202, 176, 1, GFILL);
    b.Sp(SPD.drop, 194, 156, 1, LIQUID, { when: 'hot' });
    b.Sp(SPD.spark, 206, 152, 1, GREEN, { when: 'ok fresh', track: b.blink(500) });
    b.Sp(SPD.zz, 210, 150, 1, INK, { when: NO });

    // the bench
    b.R(0, 204, 240, 36, BENCH);
    b.T(10, 208, 'WAITING FOR APP', 'S12', WHITE, 'l', { when: NO });
    b.T(10, 208, 'OUT AT {out}', 'S12', YELLOW, 'l', { when: 'hot' });
    b.T(10, 208, 'NEW WINDOW {sr@}', 'S12', WHITE, 'l', { when: FRESH });
    b.T(10, 208, 'RESET {sr@}', 'S12', WHITE, 'l', { when: PLAIN });
    b.T(10, 222, 'IN {sr~}', 'S12', GLASS, 'l', { when: OK });
    b.T(10, 222, '--', 'S12', GLASS, 'l', { when: NO });
    b.T(168, 216, '{dow}', 'S12', GLASS, 'r', { upper: true });
    b.T(228, 210, '{clock}', 'S20', WHITE, 'r');
  },
});
