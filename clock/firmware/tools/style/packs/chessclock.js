// #29 Chess clock: a wooden double clock. The left face shows the 5-hour window, the right
// face the week; each has a needle on a 12-position dial (value x 12 / 100) and a red flag at
// 12 o'clock that goes up at 80 % (Batch C, archetype 5: split screen, gated needles).
const { mock, SPR } = require('../mock');
const { rectRows } = require('./_c2_util');

const K = '#1c1410', WOOD = '#6b3a1e', HI = '#8a5230', PLUNGER = '#b89060', FACE = '#f3e6c8', PALE = '#d8c4a0', RED = '#c8102e',
  HOT = '#ff6a4a', DEAD = '#b5a88c', DIM = '#8a7a64', OR = '#d97757';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot';

const DROP = rectRows([[2, 0, 1, 1], [1, 1, 3, 1], [0, 2, 5, 2], [1, 4, 3, 1]], 5, 5, 8, 8);

module.exports = mock({
  id: 'chessclock', name: 'Chess clock', bg: K,
  draw(b) {
    // the case: two plungers on top, the body, a highlight along its top edge, the running dot
    b.R(36, 6, 40, 16, PLUNGER, { r: 3 });
    b.R(164, 12, 40, 10, PLUNGER, { r: 3 });
    b.R(6, 20, 228, 214, WOOD, { r: 10 });
    b.R(16, 20, 208, 3, HI);
    b.R(116, 28, 8, 8, RED, { r: 4, when: OK, track: b.blink(1000) });

    // a face: rim, dial, 12 ticks, label, needle positions, hub, flag. `src` is s (5H) or w (week).
    const face = (cx, label, src, hotWhen) => {
      b.C(cx, 84, 44, K);
      b.C(cx, 84, 42, FACE, { when: OK });
      b.C(cx, 84, 42, DEAD, { when: NO });
      for (let j = 0; j < 12; j++) {
        const a = (j * Math.PI) / 6;
        b.R(cx + Math.round(36 * Math.sin(a)) - 1, 84 - Math.round(36 * Math.cos(a)) - 1, 2, 2, K);
      }
      b.T(cx, 60, label, 'S12', K, 'c');
      // the needle at position j shows while round(value x 12 / 100) mod 12 = j: a thin triangle, base 4, length 32
      for (let j = 0; j < 12; j++) {
        const a = (j * Math.PI) / 6;
        const tri = [[cx + 2 * Math.cos(a), 84 + 2 * Math.sin(a)], [cx + 32 * Math.sin(a), 84 - 32 * Math.cos(a)], [cx - 2 * Math.cos(a), 84 - 2 * Math.sin(a)]];
        const spans = j === 0 ? [[0, 1 / 24], [23 / 24, 1.001]] : [[(2 * j - 1) / 24, (2 * j + 1) / 24]];
        for (const [from, to] of spans) {
          if (from < 0.8) b.P(tri, K, { when: hotWhen === 'hot' ? COOL : 'ok !whot', gate: [src, from, to] });
          if (to > 0.8) b.P(tri, RED, { when: hotWhen, gate: [src, from, to] });
        }
      }
      b.R(cx - 3, 81, 6, 6, K, { r: 3 });
      b.P([[cx, 44], [cx + 9, 47], [cx, 50]], RED, { when: hotWhen });
    };
    face(56, '5H', 's', 'hot');
    face(184, 'WEEK', 'w', 'whot');

    // Clawd on the plate between the faces
    b.M('clawd', 104, 62, 1, OR, { when: OK });
    b.M('clawd', 104, 62, 1, DIM, { when: NO });
    b.M('clawd_eyes', 104, 62, 1, K, { when: COOL, track: b.eyeBlink(OR) });
    b.M('clawd_eyes', 104, 62, 1, FACE, { when: 'hot' });
    b.M('clawd_eyes', 104, 62, 1, DIM, { when: NO });
    b.R(108, 77, 4, 1, K, { when: NO });
    b.R(128, 77, 4, 1, K, { when: NO });
    b.Sp(DROP, 108, 48, 1, FACE, { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]) });
    b.Sp(SPR.spark, 115, 46, 2, FACE, { when: 'ok fresh !hot' });
    b.T(114, 106, 'z z', 'S12', PALE, 'l', { when: NO });

    // readouts
    b.T(56, 134, '{s}%', 'S48', FACE, 'c', { when: 'ok !hot' });
    b.T(56, 134, '{s}%', 'S48', HOT, 'c', { when: 'hot' });
    b.T(56, 134, '--', 'S48', DIM, 'c', { when: NO });
    b.T(184, 144, '{w}%', 'S28', FACE, 'c', { when: 'ok !whot' });
    b.T(184, 144, '{w}%', 'S28', HOT, 'c', { when: 'whot' });
    b.T(184, 144, '--', 'S28', DIM, 'c', { when: NO });
    b.T(56, 188, 'IN {sr~}', 'S12', PALE, 'c', { when: COOL });
    b.T(56, 188, 'OUT {out}', 'S12', HOT, 'c', { when: 'hot' });
    b.T(56, 188, 'WAITING', 'S12', PALE, 'c', { when: NO });
    b.T(184, 188, 'IN {wr~}', 'S12', PALE, 'c', { when: OK });
    b.T(184, 188, 'FOR APP', 'S12', PALE, 'c', { when: NO });
    b.T(120, 208, '{dow} {clock}', 'S20', FACE, 'c', { upper: true });
  },
});
