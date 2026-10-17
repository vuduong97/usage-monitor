// #32 Oscilloscope: a graticule screen with two traces whose level is one of 16 rows and
// whose length is the elapsed time, a NOW cursor, and CH1/CH2 readouts on the bezel
// (Batch D, archetype 4: dial with gated positions).
const { mock } = require('../mock');
const { SPR, OK, NO, FRESH, bucketGate, drip, clawd } = require('./_d1_common');

const BEZEL = '#1B1D1C', SCR = '#06180E', GRAT = '#1E5134', YEL = '#F4D44D', CYAN = '#4FD6E8', WHITE = '#DDE8E0',
  DIM = '#8A9A90', RED = '#FF5A4E', REDDIM = '#5C2522', CLAWD = '#D97757', GREY = '#5E6460', GREEN = '#6EF08F';

module.exports = mock({
  id: 'scope', name: 'Oscilloscope', bg: BEZEL,
  draw(b) {
    b.R(8, 8, 224, 160, SCR, { r: 4 });
    // graticule: 8x8 divisions drawn as dotted 1 px lines, and the two axes with ticks
    for (let i = 1; i < 8; i++) b.R(8 + 28 * i, 9, 1, 158, GRAT, { p: ['h', 4, 1] });
    for (let i = 1; i < 8; i++) b.R(9, 8 + 20 * i, 222, 1, GRAT, { p: ['v', 4, 1] });
    b.R(9, 86, 222, 5, GRAT, { p: ['v', 7, 1] });
    b.R(118, 9, 5, 158, GRAT, { p: ['h', 5, 1] });
    b.R(9, 40, 222, 1, RED, { p: ['v', 6, 3], when: 'hot' });
    b.R(9, 40, 222, 1, REDDIM, { p: ['v', 6, 3], when: '!hot' });
    b.T(12, 26, 'LIM 80', 'S12', DIM);

    // traces: row k (y = 161 - 10 k) is shown while floor(value * 16 / 100) = k. CH1 is red
    // while hot (5H >= 80 %, which is inside rows 12-15; row 12 holds both colours).
    for (let k = 0; k < 16; k++) {
      const g = bucketGate('s', k, 16), y = 161 - 10 * k;
      const ch1 = (c, when) => {
        b.R(9, y + 1, 222, 1, c, { p: ['v', 4, 2], when, gate: g });
        b.R(9, y, 0, 2, c, { when, gate: g, x1: ['ts', 222], minW: 2 });
      };
      if (k < 12) ch1(YEL, OK);
      else if (k === 12) { ch1(YEL, 'ok !hot'); ch1(RED, 'hot'); }
      else ch1(RED, 'hot');
      b.R(9, y, 0, 2, CYAN, { when: OK, gate: bucketGate('w', k, 16), x1: ['tw', 222], minW: 2 });
    }
    b.R(9, 9, 1, 158, WHITE, { p: ['h', 3, 2], when: OK, dx: ['ts', 222] });
    b.T(12, 152, 'NOW', 'S12', WHITE, 'l', { when: OK, dx: ['ts', 222] });
    b.R(178, 12, 50, 16, RED, { r: 2, when: 'hot', track: b.blink(500) });
    b.T(203, 14, 'LIMIT', 'S12', SCR, 'c', { when: 'hot', track: b.blink(500) });
    b.R(178, 12, 50, 16, GREEN, { r: 2, when: 'ok fresh !hot' });
    b.T(203, 14, 'RESET', 'S12', SCR, 'c', { when: 'ok fresh !hot' });
    b.R(9, 163, 222, 1, GREY, { when: NO });
    b.T(120, 70, 'NO SIGNAL', 'S14', DIM, 'c', { when: NO });
    b.T(120, 94, 'WAITING FOR APP', 'S12', DIM, 'c', { when: NO });

    // CH1: 5-hour
    b.R(12, 178, 30, 14, YEL, { r: 2, when: 'ok !hot' });
    b.R(12, 178, 30, 14, RED, { r: 2, when: 'hot' });
    b.R(12, 178, 30, 14, GREY, { r: 2, when: NO });
    b.T(27, 179, 'CH1', 'S12', BEZEL, 'c');
    b.T(12, 196, '5H %', 'S12', DIM);
    b.T(48, 172, '{s}%', 'M32', YEL, 'l', { when: 'ok !hot' });
    b.T(48, 172, '{s}%', 'M32', RED, 'l', { when: 'hot' });
    b.T(48, 172, '--%', 'M32', GREY, 'l', { when: NO });
    // CH2: week
    b.R(12, 212, 30, 14, CYAN, { r: 2, when: OK });
    b.R(12, 212, 30, 14, GREY, { r: 2, when: NO });
    b.T(27, 213, 'CH2', 'S12', BEZEL, 'c');
    b.T(48, 210, '{w}% WK', 'M18', CYAN, 'l', { when: 'ok !whot' });
    b.T(48, 210, '{w}% WK', 'M18', RED, 'l', { when: 'whot' });
    b.T(48, 210, '--% WK', 'M18', GREY, 'l', { when: NO });

    b.T(228, 174, '{clock}', 'M18', WHITE, 'r');
    b.T(228, 198, 'OUT {out}', 'S12', RED, 'r', { when: 'hot' });
    b.T(228, 198, 'RST {sr@}', 'S12', DIM, 'r', { when: 'ok !hot' });
    b.T(228, 198, 'RST --:--', 'S12', DIM, 'r', { when: NO });
    b.T(228, 214, '{wr@d}', 'S12', DIM, 'r', { when: OK });
    b.T(228, 214, '--', 'S12', DIM, 'r', { when: NO });

    clawd(b, 122, 190, 1, CLAWD, GREY, BEZEL);
    b.Sp(SPR.drop, 114, 190, 1, CYAN, { when: 'hot', track: drip(b) });
    b.Sp(SPR.spark, 156, 186, 1, GREEN, { when: FRESH });
    b.Sp(SPR.zz, 156, 182, 1, DIM, { when: NO });
  },
});
