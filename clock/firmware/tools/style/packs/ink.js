// #47 E-reader ink: a book page on paper. Running header and rule, a serif hero number,
// two prose lines, an ornament, and the week as a Kindle-style progress rule
// (Batch E, layout: book page).
const { mock, SPR } = require('../mock');

const PAPER = '#f1efe8', INK = '#2b2b2b', GREY = '#6e6c66', RULE = '#bdbab1', DIM = '#cfccc3';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot';
// The sheet's Newsreader boxes are `line-height: 1`; Tiempos' baseline sits this far off (px).
const SERIF_DY = { t56: -6, t34: -3 };

module.exports = mock({
  id: 'ink', name: 'E-reader Ink', bg: PAPER, light: true,
  draw(b) {
    const serif = (x, y, tx, f, c, o = {}) => b.S.text(tx, x, y + SERIF_DY[f], f, c, o);
    b.T(14, 12, 'Clawd', 'S12', GREY);
    b.T(226, 11, '{dow} {clock}', 'S14', INK, 'r');
    b.R(14, 32, 212, 1, RULE);
    b.T(14, 44, 'Five-hour limit', 'S12', GREY);

    // the hero: ink block with paper digits when hot
    b.R(8, 58, 150, 64, INK, { when: 'hot' });
    serif(11, 62, '{s}%', 't56', INK, { when: COOL });
    serif(11, 62, '{s}%', 't56', PAPER, { when: 'hot' });
    serif(11, 62, '--%', 't56', DIM, { when: NO });

    // Clawd x2 in grey ink (full ink when hot, pale when asleep)
    b.clawd(162, 44, 2, { body: GREY, when: COOL });
    b.clawd(162, 44, 2, { body: INK, when: 'hot' });
    b.clawd(162, 44, 2, { body: DIM, when: NO });
    const lid = (x, y, s, c, l) => {
      for (const [dx, dy, w, h] of [[6, 12, 2, 2], [24, 12, 2, 2], [4, 14, 4, 1], [24, 14, 4, 1]]) b.R(x + dx * s, y + dy * s, w * s, h * s, c, { when: NO });
      for (const [dx, dy, w, h] of [[4, 15, 4, 1], [24, 15, 4, 1]]) b.R(x + dx * s, y + dy * s, w * s, h * s, l, { when: NO });
    };
    lid(162, 44, 2, DIM, GREY);
    b.T(148, 46, 'z', 'S14', GREY, 'l', { when: NO, track: b.blink(500) });
    b.T(140, 60, 'z', 'S12', GREY, 'l', { when: NO, track: b.blink(500), phase: 500 });
    const drip = b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }, { dy: 6 }]);
    b.Sp(SPR.drop, 228, 42, 2, INK, { when: 'hot', track: drip });
    b.Sp(SPR.spark, 146, 50, 2, INK, { when: FRESH, track: b.blink(500) });

    // the prose, one pair of lines per state
    const say = (when, a, c, col) => { b.T(14, 132, a, 'S14', col, 'l', { when }); b.T(14, 150, c, 'S14', col, 'l', { when }); };
    say(NORMAL, 'Resets at {sr@},', 'in {sr~}.', INK);
    say('hot', 'Runs out {out}.', 'Resets {sr@}, in {sr~}.', INK);
    say(FRESH, 'A fresh window.', 'Resets {sr@}, in {sr~}.', INK);
    say(NO, 'No data yet.', 'Waiting to sync.', DIM);
    for (const x of [110, 120, 130]) b.T(x, 172, '*', 'S14', GREY, 'c');

    // the week, as page progress
    serif(13, 190, '{w}%', 't34', INK, { when: OK });
    serif(13, 190, '--%', 't34', DIM, { when: NO });
    b.T(226, 192, 'This week', 'S12', GREY, 'r');
    b.T(226, 208, '{wr@d} \u00b7 {wr~}', 'S12', INK, 'r', { when: OK });
    b.T(226, 208, '--', 'S12', DIM, 'r', { when: NO });
    b.R(14, 231, 212, 1, RULE);
    b.R(14, 230, 0, 3, INK, { when: OK, x1: ['w', 212] });
  },
});
