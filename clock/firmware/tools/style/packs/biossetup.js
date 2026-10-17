// #05 BIOS setup: a setup-utility list on the left, an ITEM HELP panel with Clawd on
// the right and a key legend at the bottom (Batch A, archetype 5: split screen).
const { mock, SPR } = require('../mock');

const BLUE = '#0000aa', GREY = '#aaaaaa', WH = '#ffffff', YEL = '#ffff55', RED = '#aa0000', CLW = '#d97757';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = 'ok !hot';

module.exports = mock({
  id: 'biossetup', name: 'BIOS Setup', bg: BLUE,
  draw(b) {
    b.T(120, 7, 'CLAWD SETUP UTILITY', 'M18', WH, 'c');
    b.O(6, 30, 228, 180, GREY);
    b.O(9, 33, 222, 174, GREY);
    b.R(150, 33, 1, 174, GREY);
    b.R(153, 33, 1, 174, GREY);

    // the highlighted 5H row: grey bar, red while hot, an outline without data
    b.R(12, 37, 136, 40, GREY, { when: COOL });
    b.R(12, 37, 136, 40, RED, { when: 'hot' });
    b.O(12, 37, 136, 40, GREY, { when: NO });
    b.T(18, 48, '5H', 'M18', BLUE, 'l', { when: COOL });
    b.T(18, 48, '5H', 'M18', YEL, 'l', { when: 'hot' });
    b.T(18, 48, '5H', 'M18', GREY, 'l', { when: NO });
    b.T(144, 41, '{s}%', 'M32', BLUE, 'r', { when: COOL });
    b.T(144, 41, '{s}%', 'M32', YEL, 'r', { when: 'hot' });
    b.T(144, 41, '--', 'M32', GREY, 'r', { when: NO });

    const row = (y, label, tok, o = {}) => {
      b.T(18, y, label, 'M18', GREY);
      b.T(146, y, tok, 'M18', o.c || WH, 'r', { when: o.when || OK });
      if (!o.when) b.T(146, y, o.no || '--', 'M18', GREY, 'r', { when: NO });
    };
    b.T(18, 84, 'WEEK', 'M18', GREY);
    b.T(146, 84, '[{w}%]', 'M18', YEL, 'r', { when: 'ok whot' });
    b.T(146, 84, '[{w}%]', 'M18', WH, 'r', { when: 'ok !whot' });
    b.T(146, 84, '[--]', 'M18', GREY, 'r', { when: NO });
    row(108, 'WK IN', '{wr~}');
    row(132, '5H IN', '{sr~}');
    b.T(18, 156, 'OUT', 'M18', GREY, 'l', { when: 'hot' });
    b.T(146, 156, '{out}', 'M18', YEL, 'r', { when: 'hot' });
    b.T(18, 156, '5H', 'M18', GREY, 'l', { when: FRESH });
    b.T(146, 156, 'FRESH', 'M18', YEL, 'r', { when: FRESH });
    b.T(18, 156, 'END', 'M18', GREY, 'l', { when: NORMAL });
    b.T(146, 156, '~{end}%', 'M18', WH, 'r', { when: NORMAL });
    b.T(18, 156, 'END', 'M18', GREY, 'l', { when: NO });
    b.T(146, 156, '--', 'M18', GREY, 'r', { when: NO });
    b.T(18, 180, 'TIME', 'M18', GREY);
    b.T(146, 180, '{clock}', 'M18', WH, 'r');

    // right panel
    b.T(192, 40, 'ITEM HELP', 'S12', GREY, 'c');
    b.R(158, 58, 70, 1, GREY);
    b.M('clawd', 160, 66, 2, CLW, { when: OK });
    b.eyes(160, 66, 2, BLUE, { when: NORMAL, track: b.eyeBlink(CLW) });
    b.eyes(160, 66, 2, BLUE, { when: 'hot' });
    b.eyes(160, 66, 2, BLUE, { when: FRESH });
    b.clawd(160, 66, 2, { body: GREY, eyes: GREY, when: NO });
    b.Sp(SPR.drop, 206, 92, 2, WH, { when: 'hot', track: b.bob(3) });
    b.R(157, 150, 74, 54, RED, { when: 'hot' });
    b.O(159, 152, 70, 50, WH, { when: 'hot' });
    b.T(194, 158, 'WARNING', 'S12', YEL, 'c', { when: 'hot', track: b.blink(500) });
    b.T(194, 176, '5H HOT', 'M18', WH, 'c', { when: 'hot' });
    b.T(192, 154, '5H RESET', 'S12', GREY, 'c', { when: '!hot' });
    b.T(192, 170, '{sr@}', 'M18', YEL, 'c', { when: COOL });
    b.T(192, 170, '--', 'M18', GREY, 'c', { when: NO });
    b.Sp(SPR.spark, 186, 66, 2, YEL, { when: FRESH, track: b.blink(400) });
    b.Sp(SPR.spark, 214, 58, 1, YEL, { when: FRESH, track: b.blink(400), phase: 400 });
    b.Sp(SPR.z, 186, 70, 1, GREY, { when: NO });
    b.Sp(SPR.z, 192, 58, 2, GREY, { when: NO });

    b.T(120, 216, 'WARNING: 5H LIMIT NEAR', 'S12', YEL, 'c', { when: 'hot' });
    b.T(120, 216, '5H WINDOW RESET - ALL CLEAR', 'S12', YEL, 'c', { when: FRESH });
    b.T(120, 216, 'ESC Quit    F10 Save & Exit', 'S12', GREY, 'c', { when: NORMAL });
    b.T(120, 216, 'WAITING FOR APP...', 'S12', GREY, 'c', { when: NO });
  },
});
