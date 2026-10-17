// #22 Pinball DMD: an orange dot-matrix panel (a 3x3 dot grid laid over everything), the 5H
// number as the hero, a week bar and a TILT! flash (Batch C, archetype 1: giant hero number).
const { mock, msk } = require('../mock');

const BG = '#0a0604', PANEL = '#1e0e05', OR = '#ff7a1a', DIM = '#7a3a12', RD = '#ff3b2f', CR = '#ffd08a';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = 'ok !hot';
const DROP = msk('..#.. .###. ##### ##### .###.');
const SPARK = msk('..#.. ..#.. ##### ..#.. ..#..');

module.exports = mock({
  id: 'pinballdmd', name: 'Pinball DMD', bg: BG,
  draw(b) {
    b.R(6, 6, 228, 228, PANEL);
    b.T(14, 14, '5H SESSION', 'S16', OR, 'l', { when: OK });
    b.T(14, 14, '5H SESSION', 'S16', DIM, 'l', { when: NO });
    b.T(226, 12, '{clock}', 'S20', OR, 'r');

    // Clawd x2 recoloured to the dot-matrix orange
    b.clawd(16, 44, 2, { body: OR, when: OK });
    b.eyes(16, 44, 2, BG, { when: COOL, track: b.eyeBlink(OR) });
    b.eyes(16, 44, 2, CR, { when: 'hot' });
    b.clawd(16, 44, 2, { body: DIM, eyes: DIM, when: NO });
    b.R(24, 74, 8, 2, BG, { when: NO });
    b.R(64, 74, 8, 2, BG, { when: NO });
    b.Sp(DROP, 84, 48, 2, CR, { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]) });
    b.Sp(SPARK, 84, 44, 3, CR, { when: FRESH, track: b.S.track(400, [{ vis: true }, { vis: true }, { vis: false }]) });
    b.T(84, 44, 'Z', 'S20', DIM, 'l', { when: NO });

    b.T(226, 50, '{s}%', 'S48', OR, 'r', { when: NORMAL });
    b.T(226, 50, '{s}%', 'S48', RD, 'r', { when: 'hot' });
    b.T(226, 50, '{s}%', 'S48', CR, 'r', { when: FRESH });
    b.T(226, 50, '--', 'S48', DIM, 'r', { when: NO });
    b.T(226, 106, '{sleft}% LEFT', 'S16', OR, 'r', { when: OK });
    b.T(226, 106, '--% LEFT', 'S16', DIM, 'r', { when: NO });
    b.R(14, 132, 212, 3, OR, { when: OK });
    b.R(14, 132, 212, 3, DIM, { when: NO });

    b.T(14, 146, 'WEEK', 'S16', OR, 'l', { when: OK });
    b.T(14, 146, 'WEEK', 'S16', DIM, 'l', { when: NO });
    b.T(226, 144, '{w}%', 'S20', OR, 'r', { when: 'ok !whot' });
    b.T(226, 144, '{w}%', 'S20', RD, 'r', { when: 'whot' });
    b.T(226, 144, '--', 'S20', DIM, 'r', { when: NO });
    b.R(14, 170, 212, 9, DIM);
    b.R(14, 170, 0, 9, OR, { when: 'ok !whot', x1: ['w', 212] });
    b.R(14, 170, 0, 9, RD, { when: 'whot', x1: ['w', 212] });

    b.T(14, 192, 'RESET {sr@}', 'S16', OR, 'l', { when: OK });
    b.T(14, 192, 'RESET --', 'S16', DIM, 'l', { when: NO });
    b.T(226, 192, '{sr~}', 'S16', OR, 'r', { when: OK });
    b.T(226, 192, '--', 'S16', DIM, 'r', { when: NO });
    b.T(14, 214, 'LASTS - ENDS ~{end}%', 'S14', OR, 'l', { when: NORMAL });
    b.T(14, 214, 'OUT AT {out}', 'S14', RD, 'l', { when: 'hot' });
    b.T(226, 212, 'TILT!', 'S16', CR, 'r', { when: 'hot', track: b.blink(400) });
    b.T(14, 214, 'NEW BALL - SHOOT AGAIN', 'S14', CR, 'l', { when: FRESH });
    b.T(14, 214, 'WAITING FOR APP', 'S14', OR, 'l', { when: NO });

    // the dot grid: every 4th row and column of the panel blacked out again
    b.R(6, 9, 228, 225, BG, { p: ['h', 4, 1] });
    b.R(9, 6, 225, 228, BG, { p: ['v', 4, 1] });
  },
});
