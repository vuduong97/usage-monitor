// #33 Aircraft PFD: sky and ground with Clawd as the aircraft symbol, a speed tape (5H)
// and an altitude tape (week) of 16 segments each, boxed readouts and annunciator bars
// (Batch D, archetype 5: two tapes).
const { mock } = require('../mock');
const { SPR, OK, NO, NORMAL, FRESH, drip, clawd } = require('./_d1_common');

const SKY = '#2F6DB5', GROUND = '#8A5A2E', WHITE = '#F2F2F2', BLACK = '#050505', TAPE = '#2B3038', GREEN = '#3BE07A',
  AMBER = '#F5B025', RED = '#E8403A', MAG = '#E070E0', CLAWD = '#D97757', GREY = '#5A5F66', LGREY = '#9AA0A8';
const segCol = (k) => (k < 10 ? GREEN : k < 13 ? AMBER : RED);

module.exports = mock({
  id: 'pfd', name: 'Aircraft PFD', bg: SKY,
  draw(b) {
    b.R(0, 0, 240, 120, GREY, { when: NO });
    b.R(0, 120, 240, 120, GROUND, { when: OK });
    b.R(0, 120, 240, 120, TAPE, { when: NO });
    b.R(108, 40, 24, 1, WHITE);
    b.R(100, 60, 40, 1, WHITE);
    b.R(100, 180, 40, 1, WHITE);
    b.R(108, 200, 24, 1, WHITE);
    b.T(96, 54, '10', 'S12', WHITE, 'r');
    b.T(96, 174, '10', 'S12', WHITE, 'r');
    b.R(56, 119, 128, 2, WHITE);

    // the annunciator bar
    b.R(0, 0, 240, 26, BLACK);
    b.T(8, 9, '5H %', 'S12', MAG);
    b.T(120, 6, '{clock}', 'M18', GREEN, 'c');
    b.T(232, 9, 'WK %', 'S12', MAG, 'r');

    // the tapes: a track, a tick strip, and 16 segments lit up to the value
    b.R(6, 66, 48, 144, TAPE);
    b.R(40, 66, 6, 144, WHITE, { p: ['h', 9, 1] });
    b.R(186, 66, 48, 144, TAPE);
    b.R(194, 66, 6, 144, WHITE, { p: ['h', 9, 1] });
    for (let k = 0; k < 16; k++) {
      const lo = (k + 0.5) * 6.25 / 100;
      b.R(47, 202 - 9 * k, 6, 8, segCol(k), { when: OK, gate: ['s', lo] });
      b.R(187, 202 - 9 * k, 6, 8, segCol(k), { when: OK, gate: ['w', lo] });
    }

    // the readouts
    b.R(6, 28, 64, 36, BLACK);
    b.O(6, 28, 64, 36, WHITE, { when: '!hot' });
    b.O(6, 28, 64, 36, RED, { when: 'hot' });
    b.T(64, 30, '{s}', 'M32', WHITE, 'r', { when: 'ok !hot' });
    b.T(64, 30, '{s}', 'M32', RED, 'r', { when: 'hot' });
    b.T(64, 30, '--', 'M32', LGREY, 'r', { when: NO });
    b.R(178, 28, 56, 36, BLACK);
    b.O(178, 28, 56, 36, WHITE, { when: '!whot' });
    b.O(178, 28, 56, 36, RED, { when: 'whot' });
    b.T(228, 37, '{w}', 'M18', WHITE, 'r', { when: 'ok !whot' });
    b.T(228, 37, '{w}', 'M18', RED, 'r', { when: 'whot' });
    b.T(228, 37, '--', 'M18', LGREY, 'r', { when: NO });
    b.T(30, 130, 'SPD', 'S14', AMBER, 'c', { when: NO });
    b.T(210, 130, 'ALT', 'S14', AMBER, 'c', { when: NO });

    // the aircraft symbol: Clawd with its amber wing bars
    b.R(59, 116, 28, 10, BLACK);
    b.R(60, 117, 26, 8, AMBER);
    b.R(153, 116, 28, 10, BLACK);
    b.R(154, 117, 26, 8, AMBER);
    clawd(b, 88, 80, 2, CLAWD, LGREY, BLACK, BLACK);
    b.Sp(SPR.drop, 156, 80, 2, WHITE, { when: 'hot', track: drip(b) });
    b.Sp(SPR.spark, 68, 86, 2, WHITE, { when: FRESH, track: b.blink(500) });
    b.Sp(SPR.spark, 160, 92, 2, WHITE, { when: FRESH });
    b.Sp(SPR.zz, 156, 62, 2, WHITE, { when: NO });
    b.R(92, 32, 56, 18, RED, { r: 2, when: 'hot', track: b.blink(500) });
    b.T(120, 34, 'LIMIT', 'S14', WHITE, 'c', { when: 'hot', track: b.blink(500) });

    // the bottom bar
    b.R(0, 214, 240, 26, BLACK);
    b.T(8, 220, 'RST {sr@}', 'S12', GREEN, 'l', { when: OK });
    b.T(8, 220, 'RST --:--', 'S12', GREEN, 'l', { when: NO });
    b.T(126, 220, 'WAITING FOR APP', 'S12', AMBER, 'c', { when: NO });
    b.T(120, 220, 'OUT {out}', 'S12', AMBER, 'c', { when: 'hot' });
    b.T(120, 220, 'NEW WINDOW', 'S12', GREEN, 'c', { when: 'ok fresh !hot' });
    b.T(120, 220, 'END ~{end}%', 'S12', GREEN, 'c', { when: NORMAL });
    b.T(232, 220, '{wr@d}', 'S12', GREEN, 'r', { when: OK });
    b.T(232, 220, '--', 'S12', GREEN, 'r', { when: NO });
  },
});
