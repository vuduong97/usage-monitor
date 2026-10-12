// #04 Windows 95 dialog: teal desktop, a bevelled grey dialog with Clawd as the
// message icon, block progress bars and a tray clock (Batch A, archetype 2: two bars).
const { mock, SPR, msk } = require('../mock');

const TEAL = '#008080', FACE = '#c0c0c0', WH = '#ffffff', SH = '#808080', K = '#000000', NAVY = '#000080', RED = '#ff0000', YEL = '#ffff00', CLW = '#d97757';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot';
const XM = msk('##....## .##..##. ..####.. ...##... ..####.. .##..##. ##....##');
const HG = msk('######### #.......# .#.....#. .#.###.#. ..#.#.#.. ...#.#... ....#.... ...#.#... ..#...#.. .#..#..#. .#.###.#. #.......# #########');

module.exports = mock({
  id: 'win95', name: 'Windows 95', bg: TEAL,
  draw(b) {
    const raised = (x, y, w, h, f) => {
      b.R(x, y, w, h, K); b.R(x, y, w - 1, h - 1, WH);
      b.R(x + 1, y + 1, w - 2, h - 2, SH); b.R(x + 1, y + 1, w - 3, h - 3, f);
    };
    const sunken = (x, y, w, h, f) => {
      b.R(x, y, w, h, SH); b.R(x + 1, y + 1, w - 1, h - 1, WH);
      b.R(x + 1, y + 1, w - 2, h - 2, K); b.R(x + 2, y + 2, w - 3, h - 3, f);
    };
    const flash = b.blink(500);
    raised(6, 8, 228, 190, FACE);
    b.R(9, 11, 221, 18, NAVY, { when: OK });
    b.R(9, 11, 221, 18, SH, { when: NO });
    b.T(14, 13, 'Clawd Usage - Warning', 'S14', WH, 'l', { when: 'hot' });
    b.T(14, 13, 'Clawd Usage', 'S14', WH, 'l', { when: 'ok !hot' });
    b.T(14, 13, 'Clawd (Not Responding)', 'S14', WH, 'l', { when: NO });
    raised(211, 13, 16, 14, FACE);
    b.Sp(XM, 215, 16, 1, K);

    b.M('clawd', 16, 38, 1, CLW, { when: OK });
    b.eyes(16, 38, 1, K, { when: NORMAL, track: b.eyeBlink(CLW) });
    b.eyes(16, 38, 1, K, { when: 'hot' });
    b.eyes(16, 38, 1, K, { when: FRESH });
    b.clawd(16, 38, 1, { body: SH, eyes: SH, when: NO });
    b.P([[31, 84], [59, 84], [45, 58]], K, { when: 'hot', track: flash });
    b.P([[34, 82], [56, 82], [45, 62]], YEL, { when: 'hot', track: flash });
    b.T(45, 69, '!', 'S12', K, 'c', { when: 'hot', track: flash });
    b.Sp(SPR.spark, 50, 32, 1, NAVY, { when: FRESH, track: b.blink(400) });
    b.Sp(SPR.spark, 9, 70, 1, NAVY, { when: FRESH, track: b.blink(400), phase: 400 });
    b.Sp(HG, 130, 58, 2, K, { when: NO });
    b.Sp(SPR.z, 50, 34, 1, K, { when: NO });

    b.T(60, 40, '5-hour session', 'S14', K);
    b.T(58, 56, '{s}%', 'S48', K, 'l', { when: OK });
    b.T(58, 56, '--', 'S48', K, 'l', { when: NO });
    sunken(16, 108, 208, 22, WH);
    for (let k = 0; k < 18; k++) {
      b.R(20 + k * 11, 112, 9, 14, RED, { when: 'hot', gate: ['s', (k + 1) / 18] });
      b.R(20 + k * 11, 112, 9, 14, NAVY, { when: 'ok !hot', gate: ['s', (k + 1) / 18] });
    }
    b.T(16, 136, 'Resets {sr@}  (in {sr~})', 'S12', K, 'l', { when: OK });
    b.T(16, 136, 'Resets --  (in --)', 'S12', K, 'l', { when: NO });
    b.T(16, 156, 'Weekly', 'S14', K);
    b.T(72, 156, '{w}%', 'S14', K, 'l', { when: OK });
    b.T(72, 156, '--', 'S14', K, 'l', { when: NO });
    b.T(224, 158, '{wr@d}', 'S12', K, 'r', { when: OK });
    b.T(224, 158, '--', 'S12', K, 'r', { when: NO });
    sunken(16, 176, 208, 16, WH);
    for (let k = 0; k < 18; k++) {
      b.R(20 + k * 11, 180, 9, 10, RED, { when: 'ok whot', gate: ['w', (k + 1) / 18] });
      b.R(20 + k * 11, 180, 9, 10, NAVY, { when: 'ok !whot', gate: ['w', (k + 1) / 18] });
    }

    // taskbar
    b.R(0, 208, 240, 32, FACE);
    b.R(0, 209, 240, 1, WH);
    raised(6, 214, 124, 20, FACE);
    b.T(12, 218, 'Out at {out}', 'S12', K, 'l', { when: 'hot' });
    b.T(12, 218, 'New 5h window', 'S12', K, 'l', { when: FRESH });
    b.T(12, 218, 'Ends ~{end}%', 'S12', K, 'l', { when: NORMAL });
    b.T(12, 218, 'Waiting for app', 'S12', K, 'l', { when: NO });
    b.R(7, 215, 121, 17, NAVY, { when: 'hot', track: flash });
    b.T(12, 218, 'Out at {out}', 'S12', WH, 'l', { when: 'hot', track: flash });
    sunken(176, 214, 58, 20, FACE);
    b.T(205, 217, '{clock}', 'S14', K, 'c');
  },
});
