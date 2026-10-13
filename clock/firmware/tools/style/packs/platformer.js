// #28 Platformer level: the 5-hour window as a level. Clawd runs to the reset flag as the 5H %
// grows; a dotted NOW marker shows how much of the window has passed (Clawd behind it = under
// pace). A mini-map track holds the week (Batch C, archetype 10: timeline with a NOW marker).
const { mock, SPR } = require('../mock');
const { rectRows } = require('./_c2_util');

const SKY = '#3a68d8', RED = '#b8281c', DIM = '#5a6a8a', W = '#ffffff', K = '#000000', BR = '#c84c0c', GR = '#00a800',
  GO = '#fcbc3c', OR = '#d97757', PALE = '#c8ced8';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot';

const DROP = rectRows([[2, 0, 1, 1], [1, 1, 3, 1], [0, 2, 5, 2], [1, 4, 3, 1]], 5, 5);

module.exports = mock({
  id: 'platformer', name: 'Platformer', bg: SKY,
  draw(b) {
    b.R(0, 0, 240, 240, RED, { when: 'hot' });
    b.R(0, 0, 240, 240, DIM, { when: NO });
    // clouds (calm only) and the hill
    b.R(36, 98, 40, 12, W, { r: 6, when: COOL });
    b.R(46, 90, 18, 18, W, { r: 9, when: COOL });
    b.R(128, 92, 32, 10, W, { r: 5, when: COOL });
    b.R(136, 86, 14, 14, W, { r: 7, when: COOL });
    b.C(40, 200, 30, GR);

    // HUD
    b.T(12, 8, '5H USED', 'S12', W);
    b.T(12, 22, '{s}%', 'M32', W, 'l', { when: 'ok !hot' });
    b.T(12, 22, '{s}%', 'M32', GO, 'l', { when: 'hot' });
    b.T(12, 22, '--', 'M32', PALE, 'l', { when: NO });
    b.T(104, 8, 'WEEK', 'S12', W);
    b.T(104, 24, '{w}%', 'M18', W, 'l', { when: 'ok !whot' });
    b.T(104, 24, '{w}%', 'M18', GO, 'l', { when: 'whot' });
    b.T(104, 24, '--', 'M18', PALE, 'l', { when: NO });
    b.T(228, 8, 'TIME', 'S12', W, 'r');
    b.T(228, 24, '{clock}', 'M18', W, 'r');
    // the week mini-map: track, fill, and a tick at the week's elapsed share
    b.R(104, 48, 124, 4, K);
    b.R(104, 48, 0, 4, GO, { when: OK, x1: ['w', 124] });
    b.R(103, 44, 2, 12, W, { when: OK, dx: ['tw', 124] });

    b.T(12, 62, 'OUT {out}', 'S14', GO, 'l', { when: 'hot' });
    b.T(12, 62, 'GO!', 'S14', W, 'l', { when: 'ok fresh !hot' });
    b.T(12, 62, 'END ~{end}%', 'S14', W, 'l', { when: 'ok !hot !fresh' });
    b.T(12, 62, 'WAITING FOR APP', 'S14', PALE, 'l', { when: NO });
    b.T(228, 62, 'RESET {sr@}', 'S12', W, 'r', { when: OK });
    b.T(228, 62, 'RESET --', 'S12', W, 'r', { when: NO });
    b.T(228, 77, 'IN {sr~}', 'S12', W, 'r', { when: OK });
    b.T(228, 77, 'IN --', 'S12', W, 'r', { when: NO });

    // the flag at the end of the level
    b.R(206, 92, 8, 8, GR, { r: 4 });
    b.R(209, 98, 2, 88, GR);
    b.P([[193, 108], [209, 102], [209, 114]], W);
    b.R(203, 184, 14, 12, K);
    b.R(204, 185, 12, 10, BR);

    // the NOW marker: dotted line and label at the 5-hour window's elapsed share
    b.R(23, 126, 2, 70, W, { p: ['h', 8, 4], when: OK, dx: ['ts', 168] });
    b.T(13, 110, 'NOW', 'S12', W, 'l', { when: OK, dx: ['ts', 168] });

    // the ground: brick colour, black mortar rows every 11 px and columns every 22 px
    b.R(0, 196, 240, 44, BR);
    for (const y of [206, 217, 228, 239]) b.R(0, y, 240, 1, K);
    for (let x = 21; x < 240; x += 22) b.R(x, 196, 1, 44, K);

    // Clawd: x = 8 + 168 x 5H. The run-bob (dy -2, 200 ms) and the 150 ms eye lid every 3 s share
    // one 50 ms track of 6 s, so the eyes bob with the body.
    const X = ['s', 168];
    const run = b.S.track(200, [{ dy: 0 }, { dy: -2 }]);
    const runBlink = b.S.track(50, Array.from({ length: 120 }, (_, i) => ({ dy: (i >> 2) & 1 ? -2 : 0, color: i % 60 >= 57 ? OR : undefined })));
    b.M('clawd', 8, 156, 1, OR, { when: COOL, dx: X, track: run });
    b.M('clawd', 8, 156, 1, OR, { when: 'hot', dx: X });
    b.M('clawd', 8, 156, 1, PALE, { when: NO });
    b.M('clawd_eyes', 8, 156, 1, K, { when: COOL, dx: X, track: runBlink });
    b.M('clawd_eyes', 8, 156, 1, W, { when: 'hot', dx: X });
    b.M('clawd_eyes', 8, 156, 1, PALE, { when: NO });
    b.R(12, 171, 4, 1, K, { when: NO });
    b.R(32, 171, 4, 1, K, { when: NO });

    // hot: a sweat drop; fresh: a sparkle. Both hop to Clawd's left near the flag (x + 34 > 196, 5H >= 92 %).
    const edge = 0.9196;
    const drip = b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]);
    b.Sp(DROP, 42, 150, 2, W, { when: 'hot', dx: X, gate: ['s', 0, edge], track: drip });
    b.Sp(DROP, -4, 150, 2, W, { when: 'hot', dx: X, gate: ['s', edge], track: drip });
    b.Sp(SPR.spark, 42, 148, 2, GO, { when: 'ok fresh !hot', dx: X, gate: ['s', 0, edge] });
    b.Sp(SPR.spark, -4, 148, 2, GO, { when: 'ok fresh !hot', dx: X, gate: ['s', edge] });
    b.T(44, 144, 'z Z', 'S16', W, 'l', { when: NO });
  },
});
