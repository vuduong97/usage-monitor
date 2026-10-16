// #24 JRPG status: two blue menu windows. The top one is the message box over the battle
// scene (Clawd on his shadow, the 5H number); the bottom one is the status list with the 5H
// and week gauges (Batch C, archetype 2: two horizontal bars).
const { mock, msk } = require('../mock');
const { outline } = require('./_c1_ring');

const K = '#000000', WH = '#ffffff', B1 = '#3444b4', B2 = '#24318f', B3 = '#16206a', LB = '#9fd0ff', GR = '#4ce070',
  RD = '#ff5040', WB = '#5fa8ff', YE = '#ffe14a', OR = '#d97757', GY = '#6a6ab8';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = 'ok !hot';
const DROP = msk('..#.. .###. ##### ##### .###.');
const SPARK = msk('..#.. ..#.. ##### ..#.. ..#..');
const BANG = msk('.###. .###. .###. .###. .###. ..... .###. .###.');

module.exports = mock({
  id: 'jrpgstatus', name: 'JRPG status', bg: K,
  draw(b) {
    // message window
    b.R(9, 9, 222, 12, B1);
    b.R(9, 21, 222, 9, B2);
    b.T(16, 13, 'ON PACE - ENDS ~{end}%', 'S14', WH, 'l', { when: NORMAL });
    b.T(16, 13, 'LIMIT! OUT AT {out}', 'S14', WH, 'l', { when: 'hot' });
    b.T(16, 13, 'FULLY RESTORED!', 'S14', YE, 'l', { when: FRESH });
    b.T(16, 13, 'WAITING FOR APP...', 'S14', WH, 'l', { when: NO });

    // battle scene: the 5H number and Clawd on his shadow
    b.T(18, 46, '5H SESSION', 'S12', LB, 'l', { when: OK });
    b.T(18, 46, '5H SESSION', 'S12', GY, 'l', { when: NO });
    b.T(14, 62, '{s}%', 'S48', WH, 'l', { when: NORMAL });
    b.T(14, 62, '{s}%', 'S48', RD, 'l', { when: 'hot' });
    b.T(14, 62, '{s}%', 'S48', GR, 'l', { when: FRESH });
    b.T(14, 62, '--', 'S48', GY, 'l', { when: NO });
    b.R(148, 119, 72, 3, B2);
    b.clawd(152, 40, 2, { body: OR, when: OK });
    b.eyes(152, 40, 2, K, { when: COOL, track: b.eyeBlink(OR) });
    b.eyes(152, 40, 2, RD, { when: 'hot' });
    b.clawd(152, 40, 2, { body: GY, eyes: GY, when: NO });
    b.R(160, 70, 8, 2, K, { when: NO });
    b.R(200, 70, 8, 2, K, { when: NO });
    b.Sp(BANG, 179, 38, 2, YE, { when: 'hot', track: b.blink(500) });
    b.Sp(DROP, 220, 50, 2, LB, { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]) });
    b.Sp(SPARK, 136, 44, 2, YE, { when: FRESH, track: b.S.track(400, [{ vis: true }, { vis: true }, { vis: false }]) });
    b.Sp(SPARK, 222, 66, 2, GR, { when: FRESH });
    b.T(214, 40, 'Z', 'S16', GY, 'l', { when: NO });

    // status window
    b.R(9, 127, 222, 36, B1);
    b.R(9, 163, 222, 35, B2);
    b.R(9, 198, 222, 32, B3);
    b.P([[14, 136], [21, 142], [14, 148]], WH, { track: b.blink(500) });
    b.T(26, 133, 'CLAWD', 'M18', WH);
    b.T(224, 133, '{dow} {clock}', 'M18', WH, 'r', { upper: true });
    b.T(16, 163, '5H', 'S14', LB);
    b.R(44, 165, 120, 10, K);
    b.R(44, 165, 0, 10, GR, { when: COOL, x1: ['s', 120] });
    b.R(44, 165, 0, 10, RD, { when: 'hot', x1: ['s', 120] });
    b.T(224, 163, '{sr~}', 'S14', WH, 'r', { when: OK });
    b.T(224, 163, '--', 'S14', GY, 'r', { when: NO });
    b.T(16, 189, 'WK', 'S14', LB);
    b.R(44, 191, 120, 10, K);
    b.R(44, 191, 0, 10, WB, { when: 'ok !whot', x1: ['w', 120] });
    b.R(44, 191, 0, 10, RD, { when: 'whot', x1: ['w', 120] });
    b.T(224, 187, '{w}%', 'M18', WH, 'r', { when: OK });
    b.T(224, 187, '--', 'M18', GY, 'r', { when: NO });
    b.T(16, 213, 'WEEK RESET {wr@d}', 'S12', LB, 'l', { when: OK, upper: true });
    b.T(16, 213, 'WEEK RESET --', 'S12', LB, 'l', { when: NO });

    // window frames, red when the session runs hot
    for (const [color, when] of [[WH, '!hot'], [RD, 'hot']]) {
      outline(b, 6, 6, 228, 28, 4, color, when);
      outline(b, 6, 124, 228, 110, 4, color, when);
    }
  },
});
