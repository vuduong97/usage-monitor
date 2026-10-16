// #09 Pocket calculator: dark body, solar strip, a recessed LCD with a right-aligned
// 48 px readout and a hint of the key grid (Batch A, archetype 1: giant hero number).
const { mock, SPR } = require('../mock');

const BODY = '#2a2c30', KEY = '#43474e', KT = '#e8e8e8', SOL = '#3a2a24', SOLL = '#5a4436', REC = '#17181b', LCD = '#c3cdb3', INK = '#1c2219', SHD = '#8c977e', ORG = '#e8833a', RED = '#d0261e', CLW = '#d97757';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot';

module.exports = mock({
  id: 'calc', name: 'Calculator', bg: BODY,
  draw(b) {
    b.T(12, 11, 'CLAWD', 'S14', KT);
    b.R(140, 8, 92, 20, SOL, { r: 2 });
    b.R(140, 8, 92, 20, SOLL, { p: ['v', 23, 1] });
    b.R(10, 34, 220, 96, REC, { r: 5 });
    b.R(16, 40, 208, 84, LCD, { r: 2 });
    b.R(16, 40, 208, 2, SHD);
    b.T(24, 46, 'WK {w}%', 'S14', INK, 'l', { when: OK });
    b.T(24, 46, 'WK --', 'S14', SHD, 'l', { when: NO });
    b.T(216, 46, '{clock}', 'S14', INK, 'r');
    b.T(24, 68, '5H', 'S12', INK);
    const blink = b.blink(400);
    b.T(24, 86, 'E', 'S20', INK, 'l', { when: 'hot', track: blink });
    b.T(24, 88, 'CLR', 'S12', INK, 'l', { when: FRESH });
    b.T(192, 62, '{s}', 'S48', INK, 'r', { when: OK });
    b.T(192, 62, '--', 'S48', SHD, 'r', { when: NO });
    b.T(196, 84, '%', 'S24', INK, 'l', { when: OK });
    b.T(196, 84, '%', 'S24', SHD, 'l', { when: NO });
    b.T(24, 110, 'OUT {out}', 'S12', INK, 'l', { when: 'hot' });
    b.T(24, 110, 'RST {sr@}', 'S12', INK, 'l', { when: 'ok !hot' });
    b.T(24, 110, 'WAITING FOR APP', 'S12', INK, 'l', { when: NO });
    b.T(216, 110, 'IN {sr~}', 'S12', INK, 'r', { when: OK });

    b.M('clawd', 12, 146, 2, CLW);
    b.eyes(12, 146, 2, BODY, { when: NORMAL, track: b.eyeBlink(CLW) });
    b.eyes(12, 146, 2, BODY, { when: FRESH });
    b.eyes(12, 146, 2, RED, { when: 'hot' });
    b.eyes(12, 146, 2, CLW, { when: NO });
    b.Sp(SPR.drop, 78, 150, 2, KT, { when: 'hot', track: b.bob(3) });
    b.Sp(SPR.spark, 78, 146, 2, KT, { when: FRESH, track: b.blink(400) });
    b.Sp(SPR.spark, 80, 176, 1, KT, { when: FRESH, track: b.blink(400), phase: 400 });
    b.Sp(SPR.z, 78, 156, 1, KT, { when: NO });
    b.Sp(SPR.z, 80, 140, 2, KT, { when: NO });

    // keys: % lights orange (red and blinking while hot), AC lights on a fresh window
    const kx = [92, 140, 188], ky = [144, 174, 204];
    const KL = [['7', '8', '%'], ['4', '5', '='], ['1', '2', 'AC']];
    KL.forEach((row, r) => row.forEach((lab, c) => {
      const x = kx[c], y = ky[r];
      const key = (kc, o = {}) => {
        b.R(x, y, 40, 24, kc, { r: 4, ...o });
        b.T(x + 20, y + 5, lab, 'S14', kc === ORG ? BODY : KT, 'c', o);
      };
      if (lab === '%') { key(ORG, { when: '!hot' }); key(RED, { when: 'hot', track: blink }); }
      else if (lab === 'AC') { key(ORG, { when: FRESH }); key(KEY, { when: '!fresh' }); key(KEY, { when: '!ok fresh' }); key(KEY, { when: 'hot fresh' }); }
      else key(KEY);
    }));
  },
});
