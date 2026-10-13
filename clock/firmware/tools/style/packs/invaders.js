// #26 Space Invaders: 32 invaders in four rows, one shot for every 3.125 % of the 5-hour window
// used, so the invaders left are the 5H left; Clawd is the cannon and slides along the ground with
// the week (Batch C, archetype 7: count + 6: cannon by week).
const { mock, SPR } = require('../mock');
const { rectRows } = require('./_c2_util');

const K = '#000000', W = '#ffffff', G = '#20ff40', RD = '#ff3030', YL = '#ffe14a', OR = '#d97757', GR = '#5a5a5a';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot';

const SQUID = rectRows([[3, 0, 2, 1], [2, 1, 4, 1], [1, 2, 6, 1], [0, 3, 2, 1], [3, 3, 2, 1], [6, 3, 2, 1], [0, 4, 8, 1],
  [2, 5, 1, 1], [5, 5, 1, 1], [1, 6, 1, 1], [3, 6, 2, 1], [6, 6, 1, 1], [0, 7, 1, 1], [2, 7, 1, 1], [5, 7, 1, 1], [7, 7, 1, 1]], 8, 8);
const CRAB = rectRows([[2, 0, 1, 1], [8, 0, 1, 1], [3, 1, 1, 1], [7, 1, 1, 1], [2, 2, 7, 1], [1, 3, 2, 1], [4, 3, 3, 1], [8, 3, 2, 1],
  [0, 4, 11, 1], [0, 5, 1, 1], [2, 5, 7, 1], [10, 5, 1, 1], [0, 6, 1, 1], [2, 6, 1, 1], [8, 6, 1, 1], [10, 6, 1, 1], [3, 7, 2, 1], [6, 7, 2, 1]], 11, 8);
const OCTO = rectRows([[4, 0, 4, 1], [1, 1, 10, 1], [0, 2, 12, 1], [0, 3, 3, 1], [5, 3, 2, 1], [9, 3, 3, 1], [0, 4, 12, 1],
  [3, 5, 2, 1], [7, 5, 2, 1], [2, 6, 2, 1], [5, 6, 2, 1], [8, 6, 2, 1], [0, 7, 2, 1], [10, 7, 2, 1]], 12, 8);
const BUNKER = rectRows([[2, 0, 8, 1], [1, 1, 10, 1], [0, 2, 12, 4], [0, 6, 4, 1], [8, 6, 4, 1], [0, 7, 3, 1], [9, 7, 3, 1]], 12, 8);
const DROP = rectRows([[2, 0, 1, 1], [1, 1, 3, 1], [0, 2, 5, 2], [1, 4, 3, 1]], 5, 5, 8, 8);

module.exports = mock({
  id: 'invaders', name: 'Space Invaders', bg: K,
  draw(b) {
    // HUD
    b.T(12, 8, 'SCORE<5H>', 'S12', W);
    b.T(12, 22, '{s}%', 'M32', W, 'l', { when: 'ok !hot !fresh' });
    b.T(12, 22, '{s}%', 'M32', RD, 'l', { when: 'hot' });
    b.T(12, 22, '{s}%', 'M32', G, 'l', { when: 'ok fresh !hot' });
    b.T(12, 22, '--', 'M32', GR, 'l', { when: NO });
    b.T(120, 8, 'WEEK', 'S12', W, 'c');
    b.T(120, 26, '{w}%', 'M18', W, 'c', { when: 'ok !whot' });
    b.T(120, 26, '{w}%', 'M18', RD, 'c', { when: 'whot' });
    b.T(120, 26, '--', 'M18', GR, 'c', { when: NO });
    b.T(228, 8, 'TIME', 'S12', W, 'r');
    b.T(228, 26, '{clock}', 'M18', W, 'r');

    // the formation: invader k (bottom row first, left to right) stands while 5H < (k + 1) x 3.125 %.
    // Only the last seven can still be on screen while hot (5H >= 80 %): those get a red copy.
    const rows = [[OCTO, 12, 112, 0], [CRAB, 11, 94, 8], [CRAB, 11, 76, 16], [SQUID, 8, 58, 24]];
    for (const [rows8, w, y, k0] of rows) {
      for (let c = 0; c < 8; c++) {
        const k = k0 + c, x = 36 + 24 * c - w, gate = ['s', 0, (k + 1) / 32];
        b.Sp(rows8, x, y, 2, W, { when: k >= 25 ? COOL : OK, gate });
        if (k >= 25) b.Sp(rows8, x, y, 2, RD, { when: 'hot', gate });
      }
    }
    b.T(120, 84, 'WAITING FOR APP', 'M18', W, 'c', { when: NO });

    // the reset line
    b.T(12, 136, 'RESET {sr@}', 'S12', W, 'l', { when: OK });
    b.T(12, 136, 'RESET --', 'S12', GR, 'l', { when: NO });
    b.T(228, 136, 'IN {sr~}', 'S12', W, 'r', { when: COOL });
    b.T(228, 136, 'OUT {out}', 'S12', RD, 'r', { when: 'hot' });
    b.T(228, 136, 'IN --', 'S12', GR, 'r', { when: NO });

    // bunkers and the laser
    for (const x of [30, 82, 134, 186]) b.Sp(BUNKER, x, 154, 2, G);
    const shot = b.S.track(150, [{ dy: 0 }, { dy: -8 }, { dy: -16 }, { dy: -24 }]);
    b.R(27, 174, 2, 6, W, { when: OK, dx: ['w', 184], track: shot });

    // Clawd, the cannon: x = 12 + 184 x week
    const X = ['w', 184];
    b.M('clawd', 12, 184, 1, OR, { when: OK, dx: X });
    b.M('clawd', 12, 184, 1, GR, { when: NO });
    b.M('clawd_eyes', 12, 184, 1, K, { when: COOL, dx: X, track: b.eyeBlink(OR) });
    b.M('clawd_eyes', 12, 184, 1, RD, { when: 'hot', dx: X });
    b.M('clawd_eyes', 12, 184, 1, GR, { when: NO });
    b.R(16, 199, 4, 1, K, { when: NO });
    b.R(36, 199, 4, 1, K, { when: NO });

    // hot: a sweat drop; fresh: a sparkle. Both hop to the left of the cannon at the right edge (cx > 190, week >= 97 %).
    const edge = 0.9701;
    b.Sp(DROP, 46, 186, 1, W, { when: 'hot', dx: X, gate: ['w', 0, edge], track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]) });
    b.Sp(DROP, 2, 186, 1, W, { when: 'hot', dx: X, gate: ['w', edge], track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]) });
    b.Sp(SPR.spark, 46, 182, 2, YL, { when: 'ok fresh !hot', dx: X, gate: ['w', 0, edge] });
    b.Sp(SPR.spark, 2, 182, 2, YL, { when: 'ok fresh !hot', dx: X, gate: ['w', edge] });
    b.T(48, 186, 'z Z', 'S12', GR, 'l', { when: NO });

    b.R(6, 226, 228, 2, G);
  },
});
