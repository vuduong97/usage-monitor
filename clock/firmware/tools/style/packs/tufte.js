// #48 Tufte data-ink: two annotated dot strips on a shared 0-50-100 range frame. One dot
// per value, a grey tick for the time elapsed; red is kept for "about to run out"
// (Batch E, layout: annotated dot strips).
const { mock, SPR, msk } = require('../mock');

const BG = '#fffff8', INK = '#111111', GREY = '#7a7a72', AXIS = '#cfcfc6', RED = '#b8312f', CL = '#d97757', DIM = '#c9c9c0';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot';
const SERIF_DY = { t56: -6, t34: -3 };   // Tiempos vs the sheet's Newsreader line box
const DOT = msk('..####.. .######. ######## ######## ######## ######## .######. ..####..');

module.exports = mock({
  id: 'tufte', name: 'Tufte Data-Ink', bg: BG, light: true,
  draw(b) {
    const serif = (x, y, tx, f, c, o = {}) => b.S.text(tx, x, y + SERIF_DY[f], f, c, o);
    b.T(12, 12, '5-hour', 'S12', GREY);
    b.T(228, 11, '{dow} {clock}', 'S14', INK, 'r');
    serif(9, 28, '{s}%', 't56', INK, { when: COOL });
    serif(9, 28, '{s}%', 't56', RED, { when: 'hot' });
    serif(9, 28, '--%', 't56', DIM, { when: NO });
    b.T(128, 36, 'resets {sr@}', 'S12', GREY, 'l', { when: OK });
    b.T(128, 36, 'resets --:--', 'S12', GREY, 'l', { when: NO });
    b.T(128, 52, 'in {sr~}', 'S12', INK, 'l', { when: OK });
    b.T(128, 52, 'in --', 'S12', DIM, 'l', { when: NO });
    b.T(128, 68, 'out at {out}', 'S12', RED, 'l', { when: 'hot' });
    b.T(128, 68, 'new window', 'S12', GREY, 'l', { when: FRESH });
    b.T(128, 68, '--', 'S12', GREY, 'l', { when: NO });
    // "time used NN%": the elapsed share has no token, so one op per percent
    for (let n = 5; n < 100; n++) {
      b.T(128, 68, `time used ${n}%`, 'S12', GREY, 'l', { when: NORMAL, gate: ['ts', (10 * n - 5) / 1000, (10 * n + 5) / 1000] });
    }

    // 5-hour strip: range frame, time-elapsed tick, used line and dot
    b.R(12, 104, 216, 1, AXIS);
    for (const x of [12, 120, 227]) b.R(x, 101, 1, 7, AXIS);
    b.R(12, 94, 1, 7, GREY, { when: OK, dx: ['ts', 215] });
    b.R(12, 103, 0, 2, INK, { when: COOL, x1: ['s', 215] });
    b.R(12, 103, 0, 2, RED, { when: 'hot', x1: ['s', 215] });
    b.Sp(DOT, 8, 100, 1, INK, { when: COOL, dx: ['s', 215] });
    b.Sp(DOT, 8, 100, 1, RED, { when: 'hot', dx: ['s', 215], track: b.S.track(500, [{}, { color: INK }]) });

    b.T(12, 124, 'week', 'S12', GREY);
    serif(11, 140, '{w}%', 't34', INK, { when: COOL });
    serif(11, 140, '{w}%', 't34', RED, { when: 'hot' });
    serif(11, 140, '--%', 't34', DIM, { when: NO });
    b.T(84, 144, 'reset {wr@d}', 'S12', GREY, 'l', { when: OK });
    b.T(84, 144, 'reset --', 'S12', GREY, 'l', { when: NO });
    b.T(84, 160, 'in {wr~}', 'S12', INK, 'l', { when: OK });
    b.T(84, 160, 'in --', 'S12', DIM, 'l', { when: NO });

    // Clawd x1 in its own orange (pale when asleep); the drop, sparkle and z z
    b.clawd(194, 128, 1, { body: CL, when: OK });
    b.clawd(194, 128, 1, { body: DIM, when: NO });
    for (const [dx, dy, w, h] of [[6, 12, 2, 2], [24, 12, 2, 2], [4, 14, 4, 1], [24, 14, 4, 1]]) b.R(194 + dx, 128 + dy, w, h, DIM, { when: NO });
    for (const [dx, dy, w, h] of [[4, 15, 4, 1], [24, 15, 4, 1]]) b.R(194 + dx, 128 + dy, w, h, GREY, { when: NO });
    b.T(184, 122, 'z', 'S12', GREY, 'l', { when: NO, track: b.blink(500) });
    b.T(176, 132, 'z', 'S12', GREY, 'l', { when: NO, track: b.blink(500), phase: 500 });
    b.Sp(SPR.drop, 186, 126, 2, RED, { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }, { dy: 6 }]) });
    b.Sp(SPR.spark, 180, 130, 2, INK, { when: FRESH, track: b.blink(500) });

    // week strip
    b.R(12, 192, 216, 1, AXIS);
    for (const x of [12, 120, 227]) b.R(x, 189, 1, 7, AXIS);
    b.R(12, 182, 1, 7, GREY, { when: OK, dx: ['tw', 215] });
    b.R(12, 191, 0, 2, INK, { when: COOL, x1: ['w', 215] });
    b.R(12, 191, 0, 2, RED, { when: 'hot', x1: ['w', 215] });
    b.Sp(DOT, 8, 188, 1, INK, { when: COOL, dx: ['w', 215] });
    b.Sp(DOT, 8, 188, 1, RED, { when: 'hot', dx: ['w', 215] });
    b.T(11, 202, '0', 'S12', GREY);
    b.T(114, 202, '50', 'S12', GREY);
    b.T(228, 202, '100', 'S12', GREY, 'r');
    b.T(12, 220, 'tick = time elapsed, dot = used', 'S12', GREY);
  },
});
