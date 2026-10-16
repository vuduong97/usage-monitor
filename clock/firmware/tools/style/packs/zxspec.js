// #07 ZX Spectrum: black paper, attribute-cell columns in the rainbow bands, corner
// rainbow stripes and the border colour as the state light; loading stripes while
// there is no data (Batch A, archetype 3: two columns).
const { mock, SPR } = require('../mock');

const K = '#000000', WH = '#ffffff', GRY = '#d7d7d7', CYN = '#00ffff', GRN = '#00ff00', YEL = '#ffff00', RED = '#ff0000', BLU = '#0000d7', CLW = '#d70000';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = 'ok !hot';

module.exports = mock({
  id: 'zxspec', name: 'ZX Spectrum', bg: K,
  draw(b) {
    b.R(0, 0, 240, 240, BLU, { when: NORMAL });
    b.R(0, 0, 240, 240, GRN, { when: FRESH });
    b.R(0, 0, 240, 240, RED, { when: 'hot' });
    b.R(0, 0, 240, 240, RED, { when: NO });
    b.R(0, 0, 240, 240, CYN, { when: NO, p: ['h', 8, 4] });
    b.R(10, 10, 220, 220, K);
    b.T(16, 14, 'CLAWD 48K', 'M18', WH, 'l', { when: OK });
    b.T(16, 14, 'PROGRAM: CLAWD', 'M18', WH, 'l', { when: NO });
    b.T(224, 14, '{clock}', 'M18', WH, 'r');
    b.T(16, 40, '5H', 'M18', CYN);
    b.R(12, 58, 84, 36, RED, { when: 'hot', track: b.blink(320) });
    b.T(16, 60, '{s}%', 'M32', WH, 'l', { when: OK });
    b.T(16, 60, '--', 'M32', GRY, 'l', { when: NO });
    b.T(16, 98, 'RST {sr@}', 'M18', GRY, 'l', { when: OK });
    b.T(16, 98, 'RST --', 'M18', GRY, 'l', { when: NO });
    b.T(16, 122, 'WEEK', 'M18', CYN);
    b.T(16, 142, '{w}%', 'M18', WH, 'l', { when: OK });
    b.T(16, 142, '--', 'M18', GRY, 'l', { when: NO });
    b.T(16, 162, '{wr@d}', 'M18', GRY, 'l', { when: OK, upper: true });
    b.T(16, 162, '--', 'M18', GRY, 'l', { when: NO });

    // two columns of 20 attribute cells, banded cyan / green / yellow / red
    const col = (x, w, src) => {
      b.R(x, 37, w, 160, BLU, { p: ['h', 8, 7] });
      for (let k = 0; k < 20; k++) {
        const c = k >= 15 ? RED : k >= 10 ? YEL : k >= 5 ? GRN : CYN;
        b.R(x, 189 - 8 * k, w, 7, c, { when: OK, gate: [src, (k + 1) / 20] });
      }
    };
    col(150, 32, 's');
    col(194, 20, 'w');
    b.T(166, 202, '5H', 'M18', CYN, 'c');
    b.T(204, 202, 'WK', 'M18', CYN, 'c');
    [RED, YEL, GRN, CYN].forEach((c, i) => {
      const x0 = 10 + i * 10;
      b.P([[x0, 230], [x0 + 10, 230], [x0 + 50, 190]], c);
      b.P([[x0, 230], [x0 + 50, 190], [x0 + 40, 190]], c);
    });

    b.M('clawd', 100, 186, 1, CLW, { when: OK });
    b.eyes(100, 186, 1, K, { when: COOL, track: b.eyeBlink(CLW) });
    b.eyes(100, 186, 1, K, { when: 'hot' });
    b.clawd(100, 186, 1, { body: GRY, eyes: GRY, when: NO });
    b.Sp(SPR.drop, 134, 180, 2, WH, { when: 'hot', track: b.bob(3) });
    b.Sp(SPR.spark, 134, 184, 1, YEL, { when: FRESH, track: b.blink(400) });
    b.Sp(SPR.spark, 136, 204, 1, YEL, { when: FRESH, track: b.blink(400), phase: 400 });
    b.Sp(SPR.z, 134, 182, 1, WH, { when: NO });
    b.Sp(SPR.z, 138, 168, 2, WH, { when: NO });
  },
});
