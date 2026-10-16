// #06 Commodore 64: light blue on dark blue in a thick border that shows the state;
// Clawd walks right as the 5-hour window fills, READY. prompt (Batch A, archetype 6:
// Clawd as the gauge).
const { mock, SPR } = require('../mock');

const BG = '#352879', BRD = '#6c5eb5', TXT = '#a69bf2', DIM = '#4b3f9e', CLW = '#b86962', RED = '#883932', LGR = '#9ae29b', GRY = '#6c6c6c', NDD = '#959595', WH = '#ffffff';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = 'ok !hot';

module.exports = mock({
  id: 'c64', name: 'Commodore 64', bg: BG,
  draw(b) {
    b.R(0, 0, 240, 240, BRD, { when: NORMAL });
    b.R(0, 0, 240, 240, RED, { when: 'hot' });
    b.R(0, 0, 240, 240, LGR, { when: FRESH });
    b.R(0, 0, 240, 240, GRY, { when: NO });
    b.R(12, 12, 216, 216, BG);
    // text in TXT with data, NDD (grey) without
    const T = (x, y, tx, f, al, o = {}) => {
      b.T(x, y, tx, f, TXT, al, { ...o, when: [OK, o.when].filter(Boolean).join(' ') });
      if (!o.when) b.T(x, y, o.no ?? tx, f, NDD, al, { upper: o.upper, when: NO });
    };
    T(120, 16, '*** CLAWD 64 ***', 'M18', 'c');
    T(18, 38, '5H USED', 'M18', 'l');
    b.T(222, 38, '{clock}', 'M18', TXT, 'r');
    T(37, 58, '{s}%', 'M32', 'l', { no: '--' });
    T(222, 64, 'RST {sr@}', 'M18', 'r', { no: 'RST --' });

    // Clawd at 18 + 172 x s: bobbing while it walks (not while hot)
    const walk = { dx: ['s', 172] };
    const bob = b.bob(-2, 250);
    b.M('clawd', 18, 96, 1, CLW, { when: COOL, ...walk, track: bob });
    b.eyes(18, 96, 1, BG, { when: COOL, ...walk, track: bob });
    b.M('clawd', 18, 96, 1, CLW, { when: 'hot', ...walk });
    b.eyes(18, 96, 1, BG, { when: 'hot', ...walk });
    b.clawd(18, 96, 1, { body: GRY, eyes: GRY, when: NO });
    b.R(18, 137, 204, 2, TXT, { when: OK });
    b.R(18, 137, 204, 2, NDD, { when: NO });
    [0, 50, 100].forEach((p) => {
      const x = 18 + Math.round(172 * p / 100) + 15;
      b.R(x, 140, 2, 4, TXT, { when: OK });
      b.R(x, 140, 2, 4, NDD, { when: NO });
    });
    b.Sp(SPR.drop, 54, 92, 2, WH, { when: 'hot', ...walk, track: b.bob(3) });
    b.Sp(SPR.spark, 54, 98, 2, WH, { when: FRESH, ...walk, track: b.blink(400) });
    b.Sp(SPR.spark, 66, 116, 1, WH, { when: FRESH, ...walk, track: b.blink(400) });
    b.Sp(SPR.z, 54, 98, 1, NDD, { when: NO });
    b.Sp(SPR.z, 60, 84, 2, NDD, { when: NO });

    T(18, 148, 'WK', 'M18', 'l');
    T(50, 148, '{w}%', 'M18', 'l', { no: '--' });
    T(222, 148, '{wr@d}', 'M18', 'r', { no: '--', upper: true });
    b.R(18, 170, 205, 10, DIM, { p: ['v', 13, 10] });
    for (let k = 0; k < 16; k++) b.R(18 + k * 13, 170, 10, 10, TXT, { when: OK, gate: ['w', (k + 1) / 16] });
    T(18, 186, '?OUT AT {out}', 'M18', 'l', { when: 'hot' });
    T(18, 186, 'NEW 5H. READY.', 'M18', 'l', { when: 'fresh !hot' });
    T(18, 186, 'READY.', 'M18', 'l', { when: '!hot !fresh' });
    b.T(18, 186, 'WAITING FOR APP', 'M18', NDD, 'l', { when: NO });
    b.R(18, 206, 11, 18, TXT, { track: b.blink(500) });
  },
});
