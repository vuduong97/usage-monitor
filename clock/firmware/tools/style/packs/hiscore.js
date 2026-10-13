// #25 Arcade high-score: a ranked table of five coloured rows with dot leaders under a
// 5H hero, and a blinking ticker line (Batch C, archetype 9: segment / ticker rows).
const { mock, msk } = require('../mock');

const K = '#000000', WH = '#ffffff', RD = '#ff2a2a', YE = '#ffd400', CY = '#3ad6ff', PK = '#ff8ad8', OG = '#ff9a2a',
  GR = '#7cff6a', OR = '#d97757', GY = '#5a5a5a';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = 'ok !hot';
const SPARK = msk('..#.. ..#.. ##### ..#.. ..#..');
const DROP = ['...##...', '...##...', '..####..', '########', '########', '########', '..####..', '..####..'];   // the sheet's 5x5 drop at 8 px
const COLS = [YE, CY, PK, OG, GR];

module.exports = mock({
  id: 'hiscore', name: 'High score', bg: K,
  draw(b) {
    b.T(12, 8, '5H USED', 'M18', RD);
    b.T(228, 8, 'TIME', 'M18', RD, 'r');
    b.T(12, 30, '{s}%', 'M32', WH, 'l', { when: NORMAL });
    b.T(12, 30, '{s}%', 'M32', RD, 'l', { when: 'hot' });
    b.T(12, 30, '{s}%', 'M32', GR, 'l', { when: FRESH });
    b.T(12, 30, '--', 'M32', GY, 'l', { when: NO });
    b.T(228, 32, '{clock}', 'M18', WH, 'r');

    // Clawd x1 between the hero and the clock
    b.clawd(126, 22, 1, { body: OR, when: OK });
    b.eyes(126, 22, 1, K, { when: COOL, track: b.eyeBlink(OR) });
    b.eyes(126, 22, 1, RD, { when: 'hot' });
    b.clawd(126, 22, 1, { body: GY, eyes: GY, when: NO });
    b.R(130, 37, 4, 1, K, { when: NO });
    b.R(150, 37, 4, 1, K, { when: NO });
    b.Sp(DROP, 116, 22, 1, '#3ad6ff', { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]) });
    b.Sp(SPARK, 112, 18, 2, '#ffd400', { when: FRESH });
    b.Sp(SPARK, 160, 54, 2, GR, { when: FRESH });
    b.T(160, 54, 'z', 'S12', GY, 'l', { when: NO });

    b.T(120, 74, 'HIGH SCORES', 'M18', YE, 'c');
    // each row: rank + name, a dot leader, the value; one variant per state it changes in
    const row = (i, variants) => {
      const y = 100 + 22 * i;
      for (const [when, c, name, value] of variants) {
        b.T(12, y, name, 'M18', c, 'l', { when });
        b.R(126, y + 9, 32, 2, c, { when, p: ['v', 4, 2] });
        b.T(228, y, value, 'M18', c, 'r', { when });
      }
    };
    row(0, [['ok !whot', COLS[0], '1ST WEEK', '{w}%'], ['whot', RD, '1ST WEEK', '{w}%'], [NO, GY, '1ST WEEK', '--']]);
    row(1, [[OK, COLS[1], '2ND 5H RST', '{sr@}'], [NO, GY, '2ND 5H RST', '--']]);
    row(2, [[OK, COLS[2], '3RD 5H IN', '{sr~}'], [NO, GY, '3RD 5H IN', '--']]);
    row(3, [[OK, COLS[3], '4TH WK IN', '{wr~}'], [NO, GY, '4TH WK IN', '--']]);
    row(4, [[NORMAL, COLS[4], '5TH END', '~{end}%'], ['hot', RD, '5TH OUT', '{out}'], [FRESH, COLS[4], '5TH LEFT', '{sleft}%'], [NO, GY, '5TH END', '--']]);

    b.T(120, 214, 'PLAYER 1 ON PACE', 'M18', WH, 'c', { when: NORMAL });
    b.T(120, 214, 'OUT AT {out} !', 'M18', RD, 'c', { when: 'hot', track: b.blink(500) });
    b.T(120, 214, 'GAME START!', 'M18', GR, 'c', { when: FRESH });
    b.T(120, 214, 'WAITING FOR APP', 'M18', WH, 'c', { when: NO, track: b.blink(500) });
  },
});
