// #44 Neo-brutalist web: flat state colour (yellow, red hot, blue fresh, grey no data), hard
// black blocks with offset shadows, a hero number and a scrolling ticker (Batch E).
const { mock } = require('../mock');
const { OK, NO, NORMAL, FRESH, SPARK, lid, dripTrack, drop, blinkA } = require('./_e1_common');

const Y = '#ffe600', K = '#000000', W = '#ffffff', R = '#ff2e2e', B = '#2f4bff', G = '#d6d6d6';
const OR = '#d97757', DG = '#7a7a7a';

// The sheet's four looks: gate, screen colour, accent (top band, week label, ticker),
// stripe colour, tag text, ticker text.
const LOOKS = [
  { when: NORMAL, bg: Y, acc: Y, tag: 'CLAWD', tick: 'RESET {sr@} /// WEEK {wr~} LEFT /// ' },
  { when: 'hot', bg: R, acc: R, tag: 'OUT{out}', tick: '!!! OUT {out} /// RESET {sr@} /// ' },
  { when: FRESH, bg: B, acc: W, tag: 'FRESH', tick: 'FRESH WINDOW /// RESET {sr@} /// ' },
  { when: NO, bg: G, acc: G, tag: 'NO DATA', tick: 'NO DATA /// WAITING FOR SYNC /// ' },
];

// The ticker moves 2 px per 100 ms (216 steps) and wraps after 432 px (40 mono characters).
// A track step moves at most 127 px, so the run is cut in two windows (steps 0-101 and
// 102-215); each window is a track that is visible, and centred on its op's x, only in its
// own steps. The text is drawn twice, 432 px apart: A, and its copy B (on screen only in
// the second window).
const TICK_STEPS = 216, SPLIT = 102;

module.exports = mock({
  id: 'brutal', name: 'Neo-brutalist', bg: Y, light: true,
  draw(b) {
    const blink = blinkA(b);
    const half = (from, to) => b.S.track(100, Array.from({ length: TICK_STEPS }, (_, i) => (
      i >= from && i < to ? { dx: (to - from - 1) - 2 * (i - from) } : { vis: false })));
    const first = half(0, SPLIT), second = half(SPLIT, TICK_STEPS);
    // the stripes of the week bar: 45 deg, 4 px black / 4 px colour along the gradient
    b.S.defMask('stripes', 144, 34, (x, y) => ((((x - y + 34) * Math.SQRT1_2) % 8 + 8) % 8) < 4);

    for (const L of LOOKS.slice(1)) b.R(0, 0, 240, 240, L.bg, { when: L.when });
    b.R(0, 0, 240, 30, K);
    for (const L of LOOKS) {
      b.T(8, 6, '{dow} {clock}', 'M18', L.acc, 'l', { when: L.when, upper: true });
      b.T(232, 6, L.tag, 'M18', W, 'r', { when: L.when, track: L.when === 'hot' ? blink : undefined });
    }

    // hero box: hard shadow, border, white face
    b.R(14, 44, 150, 110, K);
    b.R(8, 38, 150, 110, K);
    b.R(11, 41, 144, 104, W);
    b.T(18, 48, '5H USED', 'M18', K);
    b.T(16, 70, '{s}%', 'S48', K, 'l', { when: 'ok !hot' });
    b.T(16, 70, '{s}%', 'S48', R, 'l', { when: 'hot' });
    b.T(16, 70, '--%', 'S48', DG, 'l', { when: NO });
    b.T(18, 128, 'RESET {sr@}', 'S12', K, 'l', { when: OK });
    b.T(18, 128, 'RESET --:--', 'S12', K, 'l', { when: NO });

    b.clawd(170, 48, 2, { body: OR, when: NORMAL });
    b.clawd(170, 48, 2, { body: OR, when: FRESH });
    b.clawd(170, 48, 2, { body: W, when: 'hot' });
    b.clawd(170, 48, 2, { body: DG, when: NO });
    lid(b, 170, 48, 2, DG, K, { when: NO });
    b.T(200, 34, 'Z', 'S14', K, 'l', { when: NO, track: blink });
    b.T(212, 32, 'z', 'S12', K, 'l', { when: NO, track: blink, phase: 500 });
    drop(b, 162, 48, K, dripTrack(b, 8), { when: 'hot' });
    b.Sp(SPARK, 158, 40, 2, W, { when: FRESH, track: blink });
    b.T(170, 136, '{sr~}', 'S14', K, 'l', { when: 'ok !fresh', upper: true });
    b.T(170, 136, '{sr~}', 'S14', W, 'l', { when: FRESH, upper: true });
    b.T(170, 136, '--', 'S14', DG, 'l', { when: NO });

    // week: label block and the striped bar
    b.R(8, 164, 72, 40, K);
    for (const L of LOOKS) b.T(14, 170, 'WEEK', 'S12', L.acc, 'l', { when: L.when });
    b.T(14, 182, '{w}%', 'S20', W, 'l', { when: OK });
    b.T(14, 182, '--%', 'S20', W, 'l', { when: NO });
    b.R(84, 164, 150, 40, K);
    b.R(87, 167, 144, 34, W);
    for (const L of LOOKS.slice(0, 3)) b.R(87, 167, 0, 34, L.bg, { when: L.when, x1: ['w', 144], zeroOk: true });
    b.S.mask('stripes', 87, 167, 1, K, { when: OK });
    b.R(87, 167, 144, 34, W, { when: OK, dx: ['w', 144], x1: ['fixed', 0] });   // the unfilled rest

    // ticker: black bar, the text in two windows plus its copy, then black caps to clip it to 6..234
    b.R(0, 208, 240, 32, K);
    for (const L of LOOKS) {
      const text = (x, track) => b.T(x, 215, L.tick, 'M18', L.acc, 'l', { when: L.when, upper: true, track });
      text(6 - (SPLIT - 1), first);
      text(6 - 2 * SPLIT - (TICK_STEPS - SPLIT - 1), second);
      text(438 - 2 * SPLIT - (TICK_STEPS - SPLIT - 1), second);
    }
    b.R(0, 208, 6, 32, K);
    b.R(234, 208, 6, 32, K);
  },
});
