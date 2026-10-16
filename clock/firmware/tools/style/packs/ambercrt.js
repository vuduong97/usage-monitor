// #01 Amber CRT terminal: amber phosphor, scanlines, a ring of 24 blocks for the
// 5-hour window and a blinking block cursor (Batch A, archetype 4: segmented ring).
const { mock, SPR } = require('../mock');

const BG = '#0b0700', SCAN = '#1f1300', AMB = '#ffb000', MID = '#c27c00', DIM = '#5c3b00', NDD = '#9a6400';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot';

module.exports = mock({
  id: 'ambercrt', name: 'Amber CRT', bg: BG,
  draw(b) {
    b.R(0, 0, 240, 240, SCAN, { p: ['h', 3, 1] });
    // 5h ring: 24 blocks clockwise from 12 o'clock, lit as the window fills
    for (let k = 0; k < 24; k++) {
      const a = (k * 15 - 90) * Math.PI / 180;
      const x = Math.round(120 + 72 * Math.cos(a)) - 5, y = Math.round(100 + 72 * Math.sin(a)) - 5;
      b.R(x, y, 10, 10, DIM);
      b.R(x, y, 10, 10, AMB, { when: OK, gate: ['s', (k + 1) / 24] });
    }
    b.T(120, 46, '5H', 'M18', MID, 'c', { when: OK });
    b.T(120, 52, 'WAITING', 'S12', MID, 'c', { when: NO });
    b.R(82, 66, 76, 36, AMB, { when: 'hot' });
    b.T(120, 68, '{s}%', 'M32', BG, 'c', { when: 'hot' });
    b.T(121, 69, '{s}%', 'M32', DIM, 'c', { when: 'ok !hot' });
    b.T(120, 68, '{s}%', 'M32', AMB, 'c', { when: 'ok !hot' });
    b.T(120, 68, '--', 'M32', NDD, 'c', { when: NO });

    b.clawd(104, 108, 1, { body: AMB, when: OK });
    b.eyes(104, 108, 1, BG, { when: NORMAL, track: b.eyeBlink(AMB) });
    b.eyes(104, 108, 1, BG, { when: 'hot' });
    b.eyes(104, 108, 1, BG, { when: FRESH });
    b.clawd(104, 108, 1, { body: NDD, eyes: NDD, when: NO });
    b.Sp(SPR.drop, 140, 104, 2, AMB, { when: 'hot', track: b.bob(3) });
    b.Sp(SPR.spark, 84, 112, 2, AMB, { when: FRESH, track: b.blink(400) });
    b.Sp(SPR.spark, 144, 126, 2, AMB, { when: FRESH, track: b.blink(400), phase: 400 });
    b.Sp(SPR.z, 140, 110, 1, NDD, { when: NO });
    b.Sp(SPR.z, 146, 96, 2, NDD, { when: NO });

    b.R(6, 6, 46, 22, AMB, { when: 'hot', track: b.blink(500) });
    b.T(13, 8, 'HOT', 'M18', BG, 'l', { when: 'hot', track: b.blink(500) });
    b.T(8, 8, 'NEW', 'M18', AMB, 'l', { when: FRESH });
    b.T(8, 8, '{dow}', 'M18', MID, 'l', { when: 'ok !fresh !hot', upper: true });
    b.T(8, 8, '{dow}', 'M18', MID, 'l', { when: NO, upper: true });
    b.T(232, 8, '{clock}', 'M18', AMB, 'r');

    b.T(8, 150, 'RESET', 'S12', MID);
    b.T(8, 164, '{sr@}', 'M18', AMB, 'l', { when: OK });
    b.T(8, 164, '--', 'M18', NDD, 'l', { when: NO });
    b.T(232, 150, 'LEFT', 'S12', MID, 'r');
    b.T(232, 164, '{sr~}', 'M18', AMB, 'r', { when: OK });
    b.T(232, 164, '--', 'M18', NDD, 'r', { when: NO });

    b.T(8, 190, '> WK', 'M18', MID);
    b.T(62, 190, '{w}%', 'M18', AMB, 'l', { when: OK });
    b.T(62, 190, '--', 'M18', NDD, 'l', { when: NO });
    b.R(100, 190, 10, 18, AMB, { track: b.blink(500) });
    b.T(232, 190, '{wr@d}', 'M18', AMB, 'r', { when: OK, upper: true });
    b.T(232, 190, '--', 'M18', NDD, 'r', { when: NO });
    for (let k = 0; k < 20; k++) {
      b.R(10 + k * 11, 214, 9, 8, DIM);
      b.R(10 + k * 11, 214, 9, 8, AMB, { when: OK, gate: ['w', (k + 1) / 20] });
    }
  },
});
