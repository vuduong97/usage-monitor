// #21 Tetris: a well whose stack grows with the 5-hour usage (one row per 6.25 %), a falling
// T-piece, ten week blocks and Clawd in the side panel (Batch C, archetype 7: scene).
const { mock, msk } = require('../mock');

const BG = '#0b0b1e', WALL = '#4a4a7a', DIM = '#1e1e3c', WH = '#f2f2f2', LAV = '#9a9ac8', CY = '#3fd0ff', YE = '#ffd23f',
  PU = '#b05cff', GR = '#4ce06a', RD = '#ff4d4d', OR = '#d97757', GY = '#555570';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = 'ok !hot';
const CYC = [CY, YE, PU, GR];
const GP = [3, 5, 1, 6, 2, 0, 4, 3, 1, 5, 2, 6, 0, 4, 2, 5];     // the empty column of each row
const DROP = msk('..#.. .###. ##### ##### .###.');

module.exports = mock({
  id: 'tetris', name: 'Tetris', bg: BG,
  draw(b) {
    // the well: walls and floor, red when the session runs hot
    for (const [x, y, w, h] of [[8, 8, 6, 224], [98, 8, 6, 224]]) {
      b.R(x, y, w, h, WALL, { p: ['h', 12, 11], when: '!hot' });
      b.R(x, y, w, h, RD, { p: ['h', 12, 11], when: 'hot' });
    }
    b.R(8, 226, 96, 6, WALL, { when: '!hot' });
    b.R(8, 226, 96, 6, RD, { when: 'hot' });

    // the stack: row k fills when 5H >= (k+1) * 6.25 %, with one empty cell per row
    for (let k = 0; k < 16; k++) {
      const y = 214 - 12 * k, g = ['s', (k + 1) * 0.0625];
      b.R(14, y, 84, 11, CYC[k % 4], { p: ['v', 12, 11], when: COOL, gate: g });
      b.R(14, y, 84, 11, RD, { p: ['v', 12, 11], when: 'hot', gate: g });
      b.R(14 + 12 * GP[k], y, 11, 11, BG, { when: OK, gate: g });
    }

    // the falling T-piece: 4 steps of 4 px, then it starts again
    const fall = b.S.track(300, [{ dy: 0 }, { dy: 4 }, { dy: 8 }, { dy: 12 }]);
    for (const [c, w] of [[PU, NORMAL], [RD, 'hot'], [GR, FRESH]]) {
      b.R(38, 10, 35, 11, c, { p: ['v', 12, 11], when: w, track: fall });
      b.R(50, 22, 11, 11, c, { when: w, track: fall });
    }
    b.T(56, 118, 'READY', 'S16', YE, 'c', { when: FRESH });
    b.T(56, 100, 'PAUSED', 'S16', WH, 'c', { when: NO });
    b.T(56, 124, 'WAITING', 'S12', LAV, 'c', { when: NO });
    b.T(56, 140, 'FOR APP', 'S12', LAV, 'c', { when: NO });

    // side panel
    b.T(114, 12, '5H', 'S12', LAV);
    b.T(232, 8, '{clock}', 'S20', WH, 'r');
    b.T(112, 30, '{s}%', 'S48', WH, 'l', { when: NORMAL });
    b.T(112, 30, '{s}%', 'S48', RD, 'l', { when: 'hot' });
    b.T(112, 30, '{s}%', 'S48', GR, 'l', { when: FRESH });
    b.T(112, 30, '--', 'S48', GY, 'l', { when: NO });
    b.T(114, 88, 'WEEK', 'S12', LAV);
    b.T(232, 85, '{w}%', 'M18', WH, 'r', { when: 'ok !whot' });
    b.T(232, 85, '{w}%', 'M18', RD, 'r', { when: 'whot' });
    b.T(232, 85, '--', 'M18', GY, 'r', { when: NO });
    b.R(114, 106, 118, 8, DIM, { p: ['v', 12, 10] });
    for (let j = 0; j < 10; j++) {
      b.R(114 + 12 * j, 106, 10, 8, YE, { when: 'ok !whot', gate: ['w', (j + 1) / 10] });
      b.R(114 + 12 * j, 106, 10, 8, RD, { when: 'whot', gate: ['w', (j + 1) / 10] });
    }

    b.O(112, 122, 120, 104, WALL);
    b.T(188, 132, 'RESET', 'S12', LAV);
    b.T(188, 148, '{sr@}', 'S14', WH, 'l', { when: OK });
    b.T(188, 148, '--', 'S14', GY, 'l', { when: NO });
    b.T(188, 166, '{sr~}', 'S12', LAV, 'l', { when: OK });
    b.T(188, 166, '--', 'S12', LAV, 'l', { when: NO });
    b.T(188, 190, 'END', 'S12', LAV, 'l', { when: NORMAL });
    b.T(188, 206, '~{end}%', 'S12', GR, 'l', { when: NORMAL });
    b.T(188, 190, 'OUT', 'S12', RD, 'l', { when: 'hot' });
    b.T(188, 206, '{out}', 'S12', RD, 'l', { when: 'hot' });

    // Clawd x2: body, eyes and the lid that closes them for 150 ms every 3 s
    b.clawd(118, 140, 2, { body: OR, when: OK });
    b.eyes(118, 140, 2, BG, { when: COOL, track: b.eyeBlink(OR) });
    b.eyes(118, 140, 2, RD, { when: 'hot' });
    b.clawd(118, 140, 2, { body: GY, eyes: GY, when: NO });
    b.R(126, 170, 8, 2, BG, { when: NO });
    b.R(166, 170, 8, 2, BG, { when: NO });
    b.Sp(DROP, 144, 142, 2, '#3fd0ff', { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]) });
    b.Sp(msk('..#.. ..#.. ##### ..#.. ..#..'), 124, 126, 2, YE, { when: FRESH });
    b.Sp(msk('..#.. ..#.. ##### ..#.. ..#..'), 168, 128, 2, GR, { when: FRESH });
    b.T(154, 128, 'z', 'S12', LAV, 'l', { when: NO });
    b.T(164, 124, 'Z', 'S16', LAV, 'l', { when: NO });
  },
});
