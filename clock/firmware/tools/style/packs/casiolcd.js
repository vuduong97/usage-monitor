// #08 Casio digital watch: black resin bezel with tiny mode labels and a grey-green
// LCD; every digit field is drawn over its 8-8 ghost segments, and the backlight
// comes on while hot (Batch A, archetype 9: segment display).
const { mock, SPR, msk } = require('../mock');

const BEZ = '#17191d', LBL = '#aeb6c8', ACC = '#d4432c', LOUT = '#7e8a70', LCD = '#b6c1a6', GH = '#a2ad92', INK = '#1d261b', BL = '#e3a548', BLG = '#cc8f35';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = '!hot';
const BELL = msk('....#.... ...###... ..#####.. ..#####.. ..#####.. .#######. ######### ......... ...###... ....#....');

module.exports = mock({
  id: 'casiolcd', name: 'Casio Watch', bg: BEZ,
  draw(b) {
    b.T(10, 10, 'LIGHT', 'S12', LBL);
    b.T(230, 10, 'USAGE CHRONOGRAPH', 'S12', LBL, 'r');
    b.R(10, 27, 220, 2, ACC);
    b.R(12, 34, 216, 164, LOUT, { r: 5 });
    b.R(14, 36, 212, 160, LCD, { r: 4, when: COOL });
    b.R(14, 36, 212, 160, BL, { r: 4, when: 'hot' });
    // a digit field: its ghost (unlit 8s), then the value in ink
    const ghost = (x, y, g, f, al = 'r') => {
      b.T(x, y, g, f, GH, al, { when: COOL });
      b.T(x, y, g, f, BLG, al, { when: 'hot' });
    };
    b.T(22, 42, '{dow}', 'M18', INK, 'l', { upper: true });
    ghost(218, 42, '88:88', 'M18');
    b.T(218, 42, '{clock}', 'M18', INK, 'r');
    b.R(188, 46, 6, 11, LCD, { when: NORMAL, track: b.blink(500) });   // the colon blinks
    b.R(22, 64, 196, 1, LOUT);

    b.M('clawd', 22, 70, 1, INK);
    b.eyes(22, 70, 1, LCD, { when: NORMAL, track: b.eyeBlink(INK) });
    b.eyes(22, 70, 1, LCD, { when: FRESH });
    b.eyes(22, 70, 1, BL, { when: 'hot' });
    b.T(62, 74, '5H', 'S12', INK);
    b.T(62, 90, 'USED', 'S12', INK);
    ghost(218, 74, '188%', 'M32');
    b.T(218, 74, '{s}%', 'M32', INK, 'r', { when: OK });
    b.T(218, 74, '--', 'M32', INK, 'r', { when: NO });
    b.R(22, 114, 198, 8, GH, { p: ['v', 10, 8], when: COOL });
    b.R(22, 114, 198, 8, BLG, { p: ['v', 10, 8], when: 'hot' });
    for (let k = 0; k < 20; k++) b.R(22 + k * 10, 114, 8, 8, INK, { when: OK, gate: ['s', (k + 1) / 20] });

    b.T(22, 133, 'WK', 'S12', INK);
    ghost(98, 130, '188%', 'M18');
    b.T(98, 130, '{w}%', 'M18', INK, 'r', { when: OK });
    b.T(98, 130, '--', 'M18', INK, 'r', { when: NO });
    // the weekly reset day and time: one right-aligned mono field over the time's ghost
    ghost(218, 130, '88:88', 'M18');
    b.T(218, 130, '{wr@d}', 'M18', INK, 'r', { when: OK, upper: true });
    b.T(118, 130, '--', 'M18', INK, 'l', { when: NO });
    b.T(218, 130, '--', 'M18', INK, 'r', { when: NO });
    b.T(22, 157, 'RST', 'S12', INK);
    ghost(104, 154, '88:88', 'M18');
    b.T(104, 154, '{sr@}', 'M18', INK, 'r', { when: OK });
    b.T(104, 154, '--', 'M18', INK, 'r', { when: NO });
    b.T(118, 157, 'IN', 'S12', INK);
    b.T(218, 154, '{sr~}', 'M18', INK, 'r', { when: OK });
    b.T(218, 154, '--', 'M18', INK, 'r', { when: NO });

    const alarm = b.blink(250);
    b.Sp(BELL, 22, 177, 1, GH, { when: COOL });
    b.Sp(BELL, 22, 177, 1, INK, { when: 'hot', track: alarm });
    b.T(38, 177, 'OUT {out}', 'S12', INK, 'l', { when: 'hot', track: alarm });
    b.T(38, 177, 'NEW SESSION', 'S12', INK, 'l', { when: FRESH });
    b.T(38, 177, 'END ~{end}%', 'S12', INK, 'l', { when: NORMAL });
    b.T(38, 177, 'WAITING FOR APP', 'S12', INK, 'l', { when: NO });
    b.Sp(SPR.spark, 56, 104, 1, INK, { when: FRESH, track: b.blink(400) });
    b.Sp(SPR.spark, 16, 67, 1, INK, { when: FRESH, track: b.blink(400), phase: 400 });
    b.Sp(SPR.z, 56, 68, 1, INK, { when: NO });
    b.T(10, 212, 'MODE', 'S12', LBL);
    b.T(120, 212, 'WATER RESIST', 'S12', LBL, 'c');
    b.T(230, 212, 'START', 'S12', LBL, 'r');
  },
});
