// #35 Patient monitor: an ECG trace for the 5-hour window and a pleth trace for the
// week, each with an erase gap that sweeps right with the elapsed time, vitals boxes,
// and Clawd by a beating heart (Batch D, archetype 6: vitals monitor). The traces' peaks
// rise and fall (tall, mid, low, mid) a step apart per beat, so the monitor looks live.
const { mock } = require('../mock');
const { SPR, OK, NO, FRESH, drip, clawd } = require('./_d1_common');

const BG = '#000000', BAR = '#1A2333', LINE = '#33405A', GREEN = '#3CF06A', CYAN = '#3FD3F0', WHITE = '#F0F0F0',
  GREY = '#7A8494', RED = '#E5262F', YELLOW = '#F2D23C', CLAWD = '#D97757', CGREY = '#4E5562';

// The live trace: the template's `moving` points scale by a level's factor, the rest
// stay. A segment touching a moving point is drawn once per level, each shown by its
// level's track; beat c runs c steps behind beat 0, so the peaks roll to the right.
const STEP = 250, SEQ = [0, 1, 2, 1];   // ms per step; the level of each step
const LEVELS = new WeakMap();           // per pack: one track per level, shared by both traces
function liveWave(b, tpl, moving, ks, x0, base, cycles, cw, c, when) {
  if (!LEVELS.has(b.S)) LEVELS.set(b.S, [0, 1, 2].map((L) => b.S.track(STEP, SEQ.map((l) => ({ vis: l === L })))));
  const lvl = LEVELS.get(b.S);
  if (ks.length !== lvl.length) throw new Error('monitor: one factor per level');
  const fixed = [];
  for (let cy = 0; cy < cycles; cy++) {
    const px = (i, k) => [x0 + cy * cw + Math.round(tpl[i][0] * cw / 48), base + Math.round(tpl[i][1] * (moving.includes(i) ? k : 1))];
    const phase = ((SEQ.length - cy % SEQ.length) % SEQ.length) * STEP;
    for (let i = 0; i < tpl.length - 1; i++) {
      if (!moving.includes(i) && !moving.includes(i + 1)) { fixed.push([px(i, 1), px(i + 1, 1)]); continue; }
      ks.forEach((k, L) => {
        const [p, q] = [px(i, k), px(i + 1, k)];
        b.S.line(p[0], p[1], q[0], q[1], c, { when, track: lvl[L], phase });
      });
    }
  }
  // the still segments, a flat run drawn as one line
  const runs = [];
  for (const [p, q] of fixed) {
    const r = runs[runs.length - 1];
    if (r && r[1][0] === p[0] && r[1][1] === p[1] && p[1] === q[1] && r[0][1] === r[1][1]) r[1] = q;
    else runs.push([p, q]);
  }
  for (const [p, q] of runs) b.S.line(p[0], p[1], q[0], q[1], c, { when });
}

module.exports = mock({
  id: 'monitor', name: 'Patient monitor', bg: BG,
  draw(b) {
    b.R(0, 0, 240, 26, BAR);
    b.T(8, 9, 'BED 1  CLAWD', 'S12', WHITE);
    b.T(232, 7, '{clock}', 'S16', WHITE, 'r');
    b.R(96, 4, 56, 18, RED, { r: 2, when: 'hot', track: b.blink(500) });
    b.T(124, 7, 'ALARM', 'S12', WHITE, 'c', { when: 'hot', track: b.blink(500) });

    b.T(8, 30, '5H', 'S12', GREEN);
    // ECG: the R spike's tip moves
    liveWave(b, [[0, 0], [10, 0], [12, -4], [14, 0], [18, 0], [20, 3], [23, -26], [26, 8], [28, 0], [34, 0], [38, -6], [42, 0], [48, 0]],
      [6], [1, 0.7, 0.4], 6, 66, 3, 46, GREEN, OK);
    b.R(6, 86, 138, 1, LINE);
    b.T(8, 92, 'WK', 'S12', CYAN);
    // pleth: the pulse and its notch move
    liveWave(b, [[0, 0], [10, -28], [16, -20], [20, -22], [30, -6], [48, 0]], [1, 2, 3], [1, 0.75, 0.5], 6, 134, 3, 46, CYAN, OK);
    // the erase gap (a real monitor's sweep)
    b.R(6, 34, 8, 48, BG, { when: OK, dx: ['ts', 130] });
    b.R(6, 102, 8, 40, BG, { when: OK, dx: ['tw', 130] });
    b.R(6, 66, 138, 1, GREY, { when: NO });
    b.R(6, 134, 138, 1, GREY, { when: NO });
    b.T(75, 104, 'WAITING FOR APP', 'S12', GREY, 'c', { when: NO });

    // vitals: 5H
    b.R(151, 29, 82, 94, RED, { when: 'hot' });
    b.O(150, 28, 84, 96, LINE);
    b.T(154, 32, '5H %', 'S12', GREEN, 'l', { when: '!hot' });
    b.T(154, 32, '5H %', 'S12', WHITE, 'l', { when: 'hot' });
    b.T(230, 32, 'HI 80', 'S12', GREY, 'r', { when: '!hot' });
    b.T(230, 32, 'HI 80', 'S12', WHITE, 'r', { when: 'hot' });
    b.T(230, 62, '{s}', 'S48', GREEN, 'r', { when: 'ok !hot' });
    b.T(230, 62, '{s}', 'S48', WHITE, 'r', { when: 'hot' });
    b.T(230, 62, '--', 'S48', CGREY, 'r', { when: NO });
    // vitals: week
    b.O(150, 128, 84, 62, LINE);
    b.T(154, 132, 'WK %', 'S12', CYAN);
    b.T(230, 152, '{w}', 'S28', CYAN, 'r', { when: 'ok !whot' });
    b.T(230, 152, '{w}', 'S28', YELLOW, 'r', { when: 'whot' });
    b.T(230, 152, '--', 'S28', CGREY, 'r', { when: NO });
    // vitals: reset
    b.O(150, 194, 84, 40, LINE);
    b.T(154, 198, 'OUT AT', 'S12', YELLOW, 'l', { when: 'hot' });
    b.T(154, 198, '5H RESET', 'S12', GREY, 'l', { when: '!hot' });
    b.T(154, 214, '{out}', 'S16', YELLOW, 'l', { when: 'hot' });
    b.T(154, 214, '{sr@}', 'S16', WHITE, 'l', { when: 'ok !hot' });
    b.T(154, 214, '--:--', 'S16', GREY, 'l', { when: NO });

    clawd(b, 8, 152, 2, CLAWD, CGREY, BG);
    b.Sp(SPR.drop, 74, 154, 1, CYAN, { when: 'hot' });
    b.Sp(SPR.spark, 72, 150, 1, GREEN, { when: FRESH });
    b.Sp(SPR.zz, 72, 148, 1, GREY, { when: NO });
    b.Sp(SPR.heart, 82, 158, 2, CGREY, { when: NO });
    b.Sp(SPR.heart, 82, 158, 2, RED, { when: 'hot', track: b.blink(200) });
    b.Sp(SPR.heart, 82, 158, 2, GREEN, { when: 'ok !hot', track: b.blink(500) });
    b.T(82, 178, 'NO DATA', 'S12', GREY, 'l', { when: NO });
    b.T(79, 178, 'PACE OVER', 'S12', YELLOW, 'l', { when: 'hot' });
    b.T(82, 178, 'NEW 5H', 'S12', GREEN, 'l', { when: 'ok fresh !hot' });
    b.T(82, 178, 'END ~{end}%', 'S12', WHITE, 'l', { when: 'ok !fresh !hot' });
    b.T(82, 198, '5H {sr~}', 'S12', GREY, 'l', { when: OK });
    b.T(82, 198, '5H --', 'S12', GREY, 'l', { when: NO });
    b.T(82, 214, 'WK {wr~}', 'S12', GREY, 'l', { when: OK });
    b.T(82, 214, 'WK --', 'S12', GREY, 'l', { when: NO });
  },
});
