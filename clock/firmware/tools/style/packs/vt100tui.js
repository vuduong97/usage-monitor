// #02 VT100 green TUI: a double-line box, an inverse menu row and htop-style bar
// tracks with a NOW caret for the time elapsed (Batch A, archetype 10: timeline).
const { mock, SPR } = require('../mock');

const BG = '#000000', GRN = '#33ff66', MID = '#1fa846', DIM = '#0d4a20', NDD = '#2a7a40';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot';

module.exports = mock({
  id: 'vt100tui', name: 'VT100', bg: BG,
  draw(b) {
    b.O(6, 14, 228, 220, MID);
    b.O(9, 17, 222, 214, MID);
    b.R(16, 6, 82, 18, BG);
    b.T(20, 6, '[CLAWD]', 'M18', GRN);
    b.R(140, 6, 84, 18, BG);
    b.T(144, 6, '[', 'M18', GRN);
    b.T(209, 6, '{clock}', 'M18', GRN, 'r');
    b.T(209, 6, ']', 'M18', GRN);

    b.R(12, 24, 216, 36, GRN, { when: OK });
    b.O(12, 24, 216, 36, MID, { when: NO });
    b.T(18, 33, '5H', 'M18', BG, 'l', { when: OK });
    b.T(18, 33, '5H', 'M18', MID, 'l', { when: NO });
    b.T(222, 26, '{s}%', 'M32', BG, 'r', { when: OK });
    b.T(222, 26, '--', 'M32', NDD, 'r', { when: NO });

    // a bar track: usage fill (src) and a NOW caret at the time elapsed (t)
    const track = (y, h, src, t) => {
      const by = y + Math.round((h - 18) / 2);
      b.T(12, by, '[', 'M18', MID);
      b.T(228, by, ']', 'M18', MID, 'r');
      b.R(24, y, 192, h, DIM, { p: ['v', 3, 2] });
      b.R(24, y, 0, h, GRN, { p: ['v', 3, 2], when: OK, x1: [src, 192] });
      b.R(23, y, 2, h, BG, { when: OK, dx: [t, 192] });
      b.P([[19, y - 8], [29, y - 8], [24, y - 2]], GRN, { when: OK, dx: [t, 192] });
    };
    track(70, 12, 's', 'ts');
    b.T(16, 88, 'RST', 'M18', MID);
    b.T(60, 88, '{sr@}', 'M18', GRN, 'l', { when: OK });
    b.T(60, 88, '--', 'M18', NDD, 'l', { when: NO });
    b.T(224, 88, '{sr~}', 'M18', GRN, 'r', { when: OK });
    b.T(224, 88, '--', 'M18', NDD, 'r', { when: NO });
    b.R(12, 112, 216, 1, MID, { p: ['v', 4, 2] });

    b.R(12, 118, 216, 22, GRN, { when: 'whot' });
    b.T(16, 120, 'WEEK', 'M18', BG, 'l', { when: 'whot' });
    b.T(16, 120, 'WEEK', 'M18', MID, 'l', { when: '!whot' });
    b.T(224, 120, '{w}%', 'M18', BG, 'r', { when: 'whot' });
    b.T(224, 120, '{w}%', 'M18', GRN, 'r', { when: 'ok !whot' });
    b.T(224, 120, '--', 'M18', NDD, 'r', { when: NO });
    track(150, 10, 'w', 'tw');
    b.T(16, 166, '{wr@d}', 'M18', GRN, 'l', { when: OK, upper: true });
    b.T(16, 166, '--', 'M18', NDD, 'l', { when: NO });
    b.T(224, 166, '{wr~}', 'M18', GRN, 'r', { when: OK });
    b.T(224, 166, '--', 'M18', NDD, 'r', { when: NO });

    b.M('clawd', 16, 188, 1, GRN, { when: OK });
    b.eyes(16, 188, 1, BG, { when: NORMAL, track: b.eyeBlink(GRN) });
    b.eyes(16, 188, 1, BG, { when: 'hot' });
    b.eyes(16, 188, 1, BG, { when: FRESH });
    b.clawd(16, 188, 1, { body: NDD, eyes: NDD, when: NO });

    // status line
    b.R(54, 197, 116, 22, GRN, { when: 'hot', track: b.blink(500) });
    b.T(58, 199, 'OUT {out}', 'M18', BG, 'l', { when: 'hot', track: b.blink(500) });
    b.Sp(SPR.drop, 50, 180, 2, GRN, { when: 'hot', track: b.bob(3) });
    b.T(58, 199, 'NEW SESSION', 'M18', GRN, 'l', { when: FRESH });
    b.Sp(SPR.spark, 50, 184, 2, GRN, { when: FRESH, track: b.blink(400) });
    b.Sp(SPR.spark, 186, 203, 1, GRN, { when: FRESH, track: b.blink(400), phase: 400 });
    b.T(58, 199, 'END ~{end}%', 'M18', MID, 'l', { when: NORMAL });
    b.R(148, 199, 10, 18, GRN, { when: NORMAL, track: b.blink(500) });
    b.T(58, 199, 'WAITING FOR APP', 'M18', MID, 'l', { when: NO });
    b.Sp(SPR.z, 50, 186, 1, MID, { when: NO });
  },
});
