// #50 Big Type: the 5-hour percentage as two seven-segment digits of rounded bars that
// fill ~70% of the glass; everything else is a footnote (Batch E, layout: hero number).
//
// No font is that big, so each digit is one of ten 1-bit masks (the sheet's seven r12
// bars, sampled at 2x). The digits follow the percentage with one gated op per value: the
// tens digit per decade, the ones digit per percent (a gate can only be one range, and
// "percent mod 10" is not one).
const { mock, SPR } = require('../mock');

const OR = '#d97757', NEAR = '#1a1a1a', HOT = '#ff5a3c', CREAM = '#f2eee6', CHAR = '#2a2a2a', DIMD = '#4a4a4a', FAINT = '#ddd6c8';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot', FRESH = 'ok fresh !hot';

// the sheet's segments: [x, y, w, h] in a 92 x 186 digit box, all radius 12
const SEG = { a: [0, 0, 92, 24], b: [68, 0, 24, 105], c: [68, 81, 24, 105], d: [0, 162, 92, 24], e: [0, 81, 24, 105], f: [0, 0, 24, 105], g: [0, 81, 92, 24] };
const MAP = ['abcdef', 'bc', 'abged', 'abgcd', 'fgbc', 'afgcd', 'afgedc', 'abc', 'abcdefg', 'abcdfg'];

function inRoundRect(px, py, [x, y, w, h], r) {
  if (px < x || px >= x + w || py < y || py >= y + h) return false;
  const dx = Math.max(x + r - px, px - (x + w - r), 0), dy = Math.max(y + r - py, py - (y + h - r), 0);
  return dx * dx + dy * dy <= r * r;
}

module.exports = mock({
  id: 'bigtype', name: 'Big Type', bg: OR,
  draw(b) {
    // ten digit masks at half size: 46 x 93 blocks of 2 x 2 px
    for (let k = 0; k < 10; k++) {
      const segs = MAP[k].split('').map((n) => SEG[n]);
      b.S.defMask(`dg${k}`, 46, 93, (x, y) => segs.some((s) => inRoundRect(2 * x + 1, 2 * y + 1, s, 12)));
    }
    // the screen colour per state
    b.R(0, 0, 240, 240, NEAR, { when: 'hot' });
    b.R(0, 0, 240, 240, CREAM, { when: FRESH });
    b.R(0, 0, 240, 240, CHAR, { when: NO });

    // tens digit: one op per decade (s in tenths of a percent, decades cut at x5.0 - 0.5);
    // the leading zero is faint; decade 8 also covers 79.5..80 (shown, not yet hot)
    const cut = (n) => (100 * n - 5) / 1000;
    for (let d = 0; d < 10; d++) {
      const gate = ['s', d === 0 ? 0 : cut(d), d === 9 ? undefined : cut(d + 1)];
      if (d === 0) b.M(`dg${d}`, 6, 6, 2, FAINT, { when: OK, gate });
      else if (d < 8) b.M(`dg${d}`, 6, 6, 2, NEAR, { when: COOL, gate });
      else {
        b.M(`dg${d}`, 6, 6, 2, HOT, { when: 'hot', gate });
        if (d === 8) b.M(`dg${d}`, 6, 6, 2, NEAR, { when: COOL, gate });
      }
    }
    // ones digit: one op per percent value
    for (let d = 0; d < 10; d++) {
      for (let v = 0; v < 10; v++) {
        const n = 10 * d + v;
        const gate = ['s', n === 0 ? 0 : (10 * n - 5) / 1000, n === 99 ? undefined : (10 * n + 5) / 1000];
        if (d < 8) b.M(`dg${v}`, 106, 6, 2, NEAR, { when: COOL, gate });
        else {
          b.M(`dg${v}`, 106, 6, 2, HOT, { when: 'hot', gate });
          if (n === 80) b.M(`dg${v}`, 106, 6, 2, NEAR, { when: COOL, gate });
        }
      }
    }
    // no data: "--", the middle bars only
    b.R(6, 87, 92, 24, DIMD, { when: NO, r: 12 });
    b.R(106, 87, 92, 24, DIMD, { when: NO, r: 12 });

    b.T(204, 6, '%', 'S28', NEAR, 'l', { when: COOL });
    b.T(204, 6, '%', 'S28', HOT, 'l', { when: 'hot', track: b.S.track(500, [{}, { color: CREAM }]) });
    b.T(204, 6, '%', 'S28', DIMD, 'l', { when: NO });
    b.T(205, 40, '5H', 'S14', NEAR, 'l', { when: COOL });
    b.T(205, 40, '5H', 'S14', CREAM, 'l', { when: 'hot' });
    b.T(205, 40, '5H', 'S14', DIMD, 'l', { when: NO });

    // Clawd: cream on orange, orange on dark and on cream, dim and asleep without data
    b.clawd(202, 152, 1, { body: CREAM, when: 'ok !hot !fresh' });
    b.clawd(202, 152, 1, { body: OR, when: 'hot' });
    b.clawd(202, 152, 1, { body: OR, when: FRESH });
    b.clawd(202, 152, 1, { body: DIMD, when: NO });
    for (const [dx, dy, w, h] of [[6, 12, 2, 2], [24, 12, 2, 2], [4, 14, 4, 1], [24, 14, 4, 1]]) b.R(202 + dx, 152 + dy, w, h, DIMD, { when: NO });
    b.T(210, 126, 'z', 'S14', CREAM, 'l', { when: NO, track: b.blink(500) });
    b.T(222, 138, 'z', 'S12', CREAM, 'l', { when: NO, track: b.blink(500), phase: 500 });
    b.Sp(SPR.drop, 224, 136, 2, HOT, { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }, { dy: 6 }]) });
    b.Sp(SPR.spark, 220, 136, 2, NEAR, { when: FRESH, track: b.blink(500) });

    // the footnotes: rule, clock, one status line, the week
    b.R(6, 198, 228, 1, NEAR, { when: COOL });
    b.R(6, 198, 228, 1, CREAM, { when: 'hot' });
    b.R(6, 198, 228, 1, DIMD, { when: NO });
    b.T(6, 206, '{clock}', 'S24', NEAR, 'l', { when: COOL });
    b.T(6, 206, '{clock}', 'S24', CREAM, 'l', { when: 'hot' });
    b.T(6, 206, '{clock}', 'S24', CREAM, 'l', { when: NO });
    b.T(84, 212, 'reset {sr@}', 'S12', NEAR, 'l', { when: COOL, gate: ['s', 0, 0.995] });
    b.T(84, 212, 'out {out}', 'S12', HOT, 'l', { when: 'hot', gate: ['s', 0, 0.995] });
    b.T(84, 212, 'limit reached', 'S12', HOT, 'l', { when: OK, gate: ['s', 0.995] });
    b.T(84, 212, '--', 'S12', DIMD, 'l', { when: NO });
    b.T(164, 212, 'wk', 'S12', NEAR, 'l', { when: COOL });
    b.T(164, 212, 'wk', 'S12', CREAM, 'l', { when: 'hot' });
    b.T(164, 212, 'wk', 'S12', DIMD, 'l', { when: NO });
    b.T(234, 206, '{w}%', 'S24', NEAR, 'r', { when: COOL });
    b.T(234, 206, '{w}%', 'S24', HOT, 'r', { when: 'hot' });
    b.T(234, 206, '--%', 'S24', DIMD, 'r', { when: NO });
  },
});
