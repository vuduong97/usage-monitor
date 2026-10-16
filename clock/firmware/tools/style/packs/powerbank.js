// #38 Power bank: two battery gauges that drain as usage rises (8 cells for the
// 5-hour window, 10 for the week), a LOW tag when hot and Clawd beside the charge
// (Batch D, archetype 6: two horizontal bars).
const { mock } = require('../mock');
const { SPD, clawd } = require('./_d2_kit');

const BODY = '#1e2126', WHITE = '#f2f4f6', GREY = '#8b929c', GREEN = '#3ddc84', RED = '#ff4d4d', YELLOW = '#ffd84a';
const CLAWD = '#d97757', CGREY = '#5a616b', DARK = '#3a3f47', AMBER = '#ffa63d';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot', FRESH = 'ok fresh';

module.exports = mock({
  id: 'powerbank', name: 'Power bank', bg: BODY,
  draw(b) {
    b.T(12, 11, '5H SESSION', 'S14', GREY);
    b.T(228, 8, '{clock}', 'S20', WHITE, 'r');

    // a battery shell in white, dark grey without data
    const shell = (draw) => { draw(WHITE, { when: OK }); draw(DARK, { when: NO }); };
    shell((c, o) => b.R(10, 34, 202, 62, c, { r: 6, ...o }));
    b.R(13, 37, 196, 56, BODY, { r: 4 });
    shell((c, o) => b.R(212, 50, 8, 30, c, { r: 2, ...o }));

    // 5H cells: charge = 100 - usage; cell k is lit while charge > 12.5 k. The top lit
    // cell has a body-coloured twin that blinks.
    const blink = b.blink(500);
    for (let k = 0; k < 8; k++) {
      const hi = (100 - 12.5 * k) / 100, x = 16 + 24 * k;
      b.R(x, 40, 22, 50, GREEN, { r: 2, when: COOL, gate: ['s', 0, Math.min(hi, 0.75)] });
      if (hi > 0.75) b.R(x, 40, 22, 50, AMBER, { r: 2, when: COOL, gate: ['s', 0.75, hi] });
      b.R(x, 40, 22, 50, RED, { r: 2, when: 'hot', gate: ['s', 0, hi] });
      b.R(x, 40, 22, 50, BODY, { r: 2, when: OK, gate: ['s', hi - 0.125, hi], track: blink });
    }
    b.Sp(SPD.bolt, 104, 49, 4, WHITE, { when: FRESH });
    b.T(12, 100, '{s}%', 'S48', WHITE, 'l', { when: COOL });
    b.T(12, 100, '{s}%', 'S48', RED, 'l', { when: 'hot' });
    b.T(12, 100, '--%', 'S48', CGREY, 'l', { when: NO });
    b.R(108, 110, 36, 16, RED, { r: 2, when: 'hot', track: blink });
    b.T(126, 112, 'LOW', 'S12', WHITE, 'c', { when: 'hot', track: blink });
    b.T(14, 152, 'WAITING FOR APP', 'S12', GREY, 'l', { when: NO });
    b.T(14, 152, 'OUT AT {out}', 'S12', RED, 'l', { when: 'hot' });
    b.T(14, 152, 'RESETS {sr@}  {sr~}', 'S12', GREY, 'l', { when: COOL });

    // week battery: 10 cells, cell k lit while (100 - week) > 10 k
    shell((c, o) => b.R(10, 172, 120, 30, c, { r: 4, ...o }));
    b.R(12, 174, 116, 26, BODY, { r: 3 });
    shell((c, o) => b.R(130, 180, 5, 14, c, { r: 1, ...o }));
    for (let k = 0; k < 10; k++) {
      const hi = (100 - 10 * k) / 100;
      b.R(15 + 11 * k, 177, 9, 20, GREEN, { r: 1, when: 'ok !whot', gate: ['w', 0, hi] });
      if (k < 2) b.R(15 + 11 * k, 177, 9, 20, RED, { r: 1, when: 'whot', gate: ['w', 0, hi] });   // red needs week >= 80
    }
    b.T(12, 208, 'WEEK {w}%', 'S20', WHITE, 'l', { when: 'ok !whot' });
    b.T(12, 208, 'WEEK {w}%', 'S20', RED, 'l', { when: 'whot' });
    b.T(12, 208, 'WEEK --%', 'S20', CGREY, 'l', { when: NO });
    b.T(228, 194, 'WK RESET', 'S12', GREY, 'r');
    b.T(228, 210, '{wr@d}', 'S14', WHITE, 'r', { when: OK });
    b.T(228, 210, '--', 'S14', CGREY, 'r', { when: NO });

    clawd(b, 162, 100, 2, CLAWD, CGREY, BODY, COOL, ['hot']);
    b.Sp(SPD.drop, 150, 104, 2, WHITE, { when: 'hot' });
    b.Sp(SPD.spark, 148, 100, 2, YELLOW, { when: FRESH });
    b.Sp(SPD.zz, 142, 96, 2, GREY, { when: NO });
  },
});
