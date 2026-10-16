// #03 Macintosh 1984: two overlapping 1-bit windows on a hatched desktop, a menu-bar
// clock and Clawd as a desktop icon (Batch A, archetype 8: card stack).
const { mock, SPR, msk } = require('../mock');

const K = '#000000', W = '#ffffff';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot';
const WATCH = msk('..######.. ..######.. ..######.. .########. #........# #....#...# #....#...# #....###.# #........# #........# .########. ..######.. ..######.. ..######..');
const BOMB = msk('.......##... ......#..... ....###..... ..#######... .#########.. .##.######.. .#.#######.. .#########.. .#########.. ..#######... ...#####....');

module.exports = mock({
  id: 'mac84', name: 'Macintosh', bg: W, light: true,
  draw(b) {
    // a window: shadow, face, outline, title bar (striped and with a close box when active)
    const win = (x, y, w, h, title, tw) => {
      b.R(x + 1, y + 1, w, h, K);
      b.R(x, y, w, h, W);
      b.O(x, y, w, h, K);
      b.R(x + 2, y + 4, w - 4, 11, K, { p: ['h', 2, 1], when: OK });
      b.R(x, y + 18, w, 1, K);
      const cx = x + Math.round(w / 2);
      b.R(cx - Math.round(tw / 2) - 6, y + 2, tw + 12, 15, W);
      b.T(cx, y + 3, title, 'S12', K, 'c');
      b.R(x + 8, y + 4, 11, 11, W, { when: OK });
      b.O(x + 8, y + 4, 11, 11, K, { when: OK });
    };
    b.hatch(0, 27, 240, 213, K, 4, 1);   // the desktop (the menu bar covers rows 0-26)
    b.R(0, 0, 240, 26, W);
    b.R(0, 26, 240, 1, K);
    b.T(12, 7, 'File', 'S14', K);
    b.T(50, 7, 'Edit', 'S14', K);
    b.R(83, 2, 52, 22, K, { when: 'hot' });
    b.T(88, 7, 'Usage', 'S14', W, 'l', { when: 'hot' });
    b.T(88, 7, 'Usage', 'S14', K, 'l', { when: '!hot' });
    b.T(232, 7, '{clock}', 'S14', K, 'r');

    // back card: week
    win(56, 36, 176, 104, 'Week', 30);
    b.T(64, 58, '{w}%', 'S20', K, 'l', { when: OK });
    b.T(64, 58, '--', 'S20', K, 'l', { when: NO });
    b.T(224, 58, '{wr@d}', 'S12', K, 'r', { when: OK });
    b.T(224, 58, '--', 'S12', K, 'r', { when: NO });
    b.O(120, 74, 104, 8, K);
    b.R(122, 76, 0, 4, K, { when: OK, x1: ['w', 100] });
    b.O(216, 86, 16, 54, K);
    b.hatch(217, 100, 14, 26, K, 4, 1);
    b.P([[219, 97], [229, 97], [224, 91]], K);
    b.P([[219, 129], [229, 129], [224, 135]], K);

    // front card: 5-hour
    win(8, 86, 188, 140, '5-Hour', 40);
    b.R(14, 110, 112, 46, K, { when: 'hot' });
    b.T(18, 108, '{s}%', 'S48', W, 'l', { when: 'hot' });
    b.T(18, 108, '{s}%', 'S48', K, 'l', { when: 'ok !hot' });
    b.T(18, 108, '--', 'S48', K, 'l', { when: NO });
    b.T(186, 112, 'Resets', 'S12', K, 'r');
    b.T(186, 128, '{sr@}', 'S16', K, 'r', { when: OK });
    b.T(186, 128, '--', 'S16', K, 'r', { when: NO });
    b.O(18, 166, 168, 14, K);
    b.R(20, 168, 0, 10, K, { when: OK, x1: ['s', 164] });
    b.T(20, 190, 'in {sr~}', 'S12', K, 'l', { when: OK });
    b.T(20, 190, 'in --', 'S12', K, 'l', { when: NO });
    b.Sp(BOMB, 94, 188, 1, K, { when: 'hot' });
    b.Sp(SPR.spark, 101, 182, 1, K, { when: 'hot', track: b.blink(300) });
    b.T(186, 190, 'Out {out}', 'S12', K, 'r', { when: 'hot' });
    b.T(186, 190, 'Fresh start', 'S12', K, 'r', { when: FRESH });
    b.T(186, 190, 'Ends ~{end}%', 'S12', K, 'r', { when: NORMAL });
    b.T(186, 190, 'Waiting for app', 'S12', K, 'r', { when: NO });
    b.Sp(WATCH, 120, 118, 2, K, { when: NO });

    // desktop icon (inverted while hot, like a selected icon)
    b.M('clawd_inner', 200, 150, 1, W, { when: '!hot' });
    b.M('clawd_inner', 200, 150, 1, K, { when: 'hot' });
    b.M('clawd_edge', 200, 150, 1, K);
    b.eyes(200, 150, 1, K, { when: NORMAL, track: b.eyeBlink(W) });
    b.eyes(200, 150, 1, K, { when: FRESH });
    b.eyes(200, 150, 1, W, { when: 'hot' });
    b.eyes(200, 150, 1, W, { when: NO });
    b.R(196, 193, 38, 15, W, { when: '!hot' });
    b.R(196, 193, 38, 15, K, { when: 'hot' });
    b.T(215, 194, 'Clawd', 'S12', K, 'c', { when: '!hot' });
    b.T(215, 194, 'Clawd', 'S12', W, 'c', { when: 'hot' });
    b.Sp(SPR.spark, 202, 118, 2, K, { when: FRESH, track: b.blink(400) });
    b.Sp(SPR.spark, 205, 100, 1, K, { when: FRESH, track: b.blink(400), phase: 400 });
    b.Sp(SPR.z, 202, 120, 2, K, { when: NO });
  },
});
