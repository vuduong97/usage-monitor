// #30 Racing HUD: a 240-degree dial of 20 segments filled by the 5H % (white, then yellow, then a
// red line), a row of eight shift lights filled by the week, a carbon-fibre ground. While hot
// the shift lights blink like a rev limiter (Batch C, archetype 4: segmented dial).
const { mock, SPR } = require('../mock');
const { rectRows, polyRows } = require('./_c2_util');

const BG = '#0e0f12', CARBON = '#16181d', EDGE = '#3a3f4a', DIM = '#8a93a6', W = '#f2f4f8', OFF = '#2a2d35', GRN = '#2ee86a',
  YEL = '#ffd23a', RED = '#ff3b3b', BLU = '#3ba4ff', OR = '#d97757';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot';

const DROP = rectRows([[2, 0, 1, 1], [1, 1, 3, 1], [0, 2, 5, 2], [1, 4, 3, 1]], 5, 5, 8, 8);

// The dial: centre (120, 128), radius 70..88, 20 segments of 10 deg with a 2 deg gap, from -120 deg.
const pt = (r, deg) => [Math.round(120 + r * Math.sin((deg * Math.PI) / 180)), Math.round(128 - r * Math.cos((deg * Math.PI) / 180))];
const SEGS = Array.from({ length: 20 }, (_, k) => {
  const a = -120 + 12 * k + 1, c = a + 10;
  return polyRows([pt(88, a), pt(88, c), pt(70, c), pt(70, a)]);
});
// all 20 as one sprite, for the unlit set
const RING = (() => {
  const x0 = Math.min(...SEGS.map((g) => g.x)), y0 = Math.min(...SEGS.map((g) => g.y));
  const x1 = Math.max(...SEGS.map((g) => g.x + g.rows[0].length)), y1 = Math.max(...SEGS.map((g) => g.y + g.rows.length));
  const grid = Array.from({ length: y1 - y0 }, () => Array(x1 - x0).fill('.'));
  for (const g of SEGS) g.rows.forEach((row, j) => [...row].forEach((ch, i) => { if (ch === '#') grid[g.y - y0 + j][g.x - x0 + i] = '#'; }));
  return { x: x0, y: y0, rows: grid.map((r) => r.join('')) };
})();
// the eight shift lights as one sprite (discs r4 every 11 px)
const LEDS = Array.from({ length: 8 }, (_, y) => Array.from({ length: 85 }, (__, x) => {
  const dx = (x % 11) + 0.5 - 4, dy = y + 0.5 - 4;
  return x % 11 < 8 && dx * dx + dy * dy <= 16 ? '#' : '.';
}).join(''));

module.exports = mock({
  id: 'racinghud', name: 'Racing HUD', bg: BG,
  draw(b) {
    // carbon: 45 deg stripes (x - y + 240 = 7.07 n + 1.4), one pixel wide
    for (let n = 0; ; n++) {
      const k = Math.round(7.0711 * n + 1.4142 - 240);
      if (k >= 240) break;
      const ya = Math.max(0, -k), yb = Math.min(239, 239 - k);
      if (ya <= yb) b.S.line(ya + k, ya, yb + k, yb, CARBON);
    }

    // TIME and WEEK boxes
    b.R(6, 6, 64, 38, BG);
    b.O(6, 6, 64, 38, EDGE);
    b.T(12, 10, 'TIME', 'S12', DIM);
    b.T(12, 24, '{clock}', 'M18', W);
    b.R(170, 6, 64, 38, BG);
    b.O(170, 6, 64, 38, EDGE, { when: '!whot' });
    b.O(170, 6, 64, 38, RED, { when: 'whot' });
    b.T(176, 10, 'WEEK', 'S12', DIM);
    b.T(228, 24, '{w}%', 'M18', W, 'r', { when: 'ok !whot' });
    b.T(228, 24, '{w}%', 'M18', RED, 'r', { when: 'whot' });
    b.T(228, 24, '--', 'M18', DIM, 'r', { when: NO });

    // shift lights, lit by the week (8 steps of 12.5 %); the whole row blinks while hot (limiter)
    const limiter = b.blink(200);
    b.Sp(LEDS, 78, 10, 1, OFF, { when: '!hot' });
    b.Sp(LEDS, 78, 10, 1, OFF, { when: 'hot', track: limiter });
    [GRN, GRN, GRN, YEL, YEL, RED, RED, BLU].forEach((c, i) => {
      const gate = ['w', (i + 1) / 8];
      b.C(82 + 11 * i, 14, 4, c, { when: COOL, gate });
      b.C(82 + 11 * i, 14, 4, c, { when: 'hot', gate, track: limiter });
    });

    // the dial: unlit set always, lit segments on top while 5H >= (k + 1) x 5 %
    b.Sp(RING.rows, RING.x, RING.y, 1, OFF);
    SEGS.forEach((g, k) => b.Sp(g.rows, g.x, g.y, 1, k < 12 ? W : k < 16 ? YEL : RED, { when: OK, gate: ['s', (k + 1) / 20] }));
    b.T(120, 98, '{s}%', 'S48', W, 'c', { when: 'ok !hot !fresh' });
    b.T(120, 98, '{s}%', 'S48', RED, 'c', { when: 'hot' });
    b.T(120, 98, '{s}%', 'S48', GRN, 'c', { when: 'ok fresh !hot' });
    b.T(120, 98, '--', 'S48', DIM, 'c', { when: NO });
    b.T(120, 150, '5H USED', 'S12', DIM, 'c', { when: OK });
    b.T(120, 150, 'WAITING FOR APP', 'S12', DIM, 'c', { when: NO });

    // bottom row
    b.T(12, 184, '5H RESET IN', 'S12', DIM);
    b.T(12, 200, '{sr~}', 'M18', W, 'l', { when: OK });
    b.T(12, 200, '--', 'M18', DIM, 'l', { when: NO });
    b.T(228, 184, 'OUT AT', 'S12', DIM, 'r', { when: 'hot' });
    b.T(228, 184, 'START', 'S12', DIM, 'r', { when: 'ok fresh !hot' });
    b.T(228, 184, 'PACE END', 'S12', DIM, 'r', { when: 'ok !hot !fresh' });
    b.T(228, 184, 'PACE', 'S12', DIM, 'r', { when: NO });
    b.T(228, 200, '{out}', 'M18', RED, 'r', { when: 'hot' });
    b.T(228, 200, 'GO!', 'M18', GRN, 'r', { when: 'ok fresh !hot' });
    b.T(228, 200, '~{end}%', 'M18', W, 'r', { when: 'ok !hot !fresh' });
    b.T(228, 200, '--', 'M18', DIM, 'r', { when: NO });

    // Clawd
    b.M('clawd', 104, 188, 1, OR, { when: OK });
    b.M('clawd', 104, 188, 1, DIM, { when: NO });
    b.M('clawd_eyes', 104, 188, 1, BG, { when: COOL, track: b.eyeBlink(OR) });
    b.M('clawd_eyes', 104, 188, 1, RED, { when: 'hot' });
    b.M('clawd_eyes', 104, 188, 1, DIM, { when: NO });
    b.R(108, 203, 4, 1, BG, { when: NO });
    b.R(128, 203, 4, 1, BG, { when: NO });
    b.Sp(DROP, 140, 186, 1, BLU, { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]) });
    b.Sp(SPR.spark, 90, 184, 2, GRN, { when: 'ok fresh !hot' });
    b.Sp(SPR.spark, 140, 192, 2, YEL, { when: 'ok fresh !hot' });
    b.T(140, 182, 'z', 'S12', DIM, 'l', { when: NO });
  },
});
