// #35 Patient monitor: an ECG trace for the 5-hour window and a pleth trace for the
// week, each with an erase gap that sweeps right with the elapsed time, vitals boxes,
// and Clawd by a beating heart (Batch D, archetype 6: vitals monitor). A bright head
// sweeps along both traces and leaves them lit, then fading, then dim.
const { mock } = require('../mock');
const { mix } = require('../style_pack');
const { SPR, OK, NO, FRESH, drip, clawd } = require('./_d1_common');

const BG = '#000000', BAR = '#1A2333', LINE = '#33405A', GREEN = '#3CF06A', CYAN = '#3FD3F0', WHITE = '#F0F0F0',
  GREY = '#7A8494', RED = '#E5262F', YELLOW = '#F2D23C', CLAWD = '#D97757', CGREY = '#4E5562';

// The live trace, like a monitor drawing it: a white-hot head sweeps left to right, the
// trace behind it stays lit, fades, then waits dim for the next pass. The trace is cut at
// every BW px, and each piece takes the colour track at a phase one step per piece, so
// one track drives the whole sweep (one op per piece: the clock animates at most 64).
const STEP = 150, BW = 10, X0 = 6, W = 138;
const PIECES = Math.ceil(W / BW);                // also the loop: 14 steps, 2.1 s a pass
const TRAIL = [0, 1, 1, 1, 1, 1, 1, 2, 2, 2];    // steps from the head: 0 head, 1 lit, 2 fading
function sweepWave(b, tpl, base, cycles, cw, c, when) {
  const shade = [mix(c, '#FFFFFF', 0.6), c, mix(c, BG, 0.4), mix(c, BG, 0.68)];   // head, lit, fading, dim
  const track = b.S.track(STEP, Array.from({ length: PIECES }, (_, k) => ({ color: shade[k < TRAIL.length ? TRAIL[k] : 3] })));
  let pts = [];
  for (let cy = 0; cy < cycles; cy++) for (const q of tpl) pts.push([X0 + cy * cw + Math.round(q[0] * cw / 48), base + q[1]]);
  pts = pts.filter((p, i) => i === 0 || p[0] !== pts[i - 1][0] || p[1] !== pts[i - 1][1]);
  pts = pts.filter((p, i) => i === 0 || i === pts.length - 1 || !(pts[i - 1][1] === p[1] && pts[i + 1][1] === p[1]));
  const piece = (p, q) => {
    const j = Math.min(PIECES - 1, Math.floor((Math.min(p[0], q[0]) - X0) / BW));
    b.S.line(p[0], p[1], q[0], q[1], shade[3], { when, track, phase: ((PIECES - j) % PIECES) * STEP });
  };
  for (let i = 0; i < pts.length - 1; i++) {
    const [p, q] = [pts[i], pts[i + 1]];
    // a flat or long segment is cut at the piece edges it crosses (a short slope lights
    // at once: cutting every one would pass the clock's 64)
    const cut = p[1] === q[1] || q[0] - p[0] > BW;
    let s = p;
    for (let X = X0 + (Math.floor((p[0] - X0) / BW) + 1) * BW; cut && X < q[0]; X += BW) {
      const t = [X, Math.round(p[1] + (q[1] - p[1]) * (X - p[0]) / (q[0] - p[0]))];
      piece(s, t);
      s = t;
    }
    piece(s, q);
  }
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
    // ECG, two beats: P, Q, a small notch, the tall R, a deep S, a small T dip
    sweepWave(b, [[0, 0], [4, 0], [6, -11], [8, 7], [10, 0], [12, -2], [13, 0], [19, 0], [22, -28], [26, 16], [30, 0], [33, 0], [34, 3], [35, 0], [48, 0]],
      64, 2, 69, GREEN, OK);
    b.R(6, 86, 138, 1, LINE);
    b.T(8, 92, 'WK', 'S12', CYAN);
    sweepWave(b, [[0, 0], [10, -28], [16, -20], [20, -22], [30, -6], [48, 0]], 134, 3, 46, CYAN, OK);
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
