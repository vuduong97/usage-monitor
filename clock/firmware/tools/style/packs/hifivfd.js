// #10 Hi-fi VFD display: cyan vacuum-fluorescent segments on a dotted grid, two level
// meters, annunciator words and a spectrum analyser that fills with the elapsed 5-hour
// window (Batch A, archetype 2: two level meters).
const { mock, SPR } = require('../mock');

const K = '#000000', DOT = '#0b1f1c', OFF = '#0f3b35', CY = '#6fffe6', MD = '#2fc3ae', GL = '#11685b', OR = '#ff6a3d', ORG = '#7a2a16';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = 'ok !hot';

module.exports = mock({
  id: 'hifivfd', name: 'Hi-fi VFD', bg: K,
  draw(b) {
    // the dot grid: 1-in-4 columns, then all but 1-in-4 rows blacked out again
    b.R(0, 0, 240, 240, DOT, { p: ['v', 4, 1] });
    b.R(0, 0, 240, 240, K, { p: ['h', 4, 3] });
    b.T(10, 10, '5H SESSION', 'S12', MD);
    b.T(166, 12, '{dow}', 'S12', MD, 'r', { upper: true });
    b.T(231, 9, '{clock}', 'M18', GL, 'r');
    b.T(230, 8, '{clock}', 'M18', CY, 'r');

    b.T(11, 31, '{s}%', 'M32', GL, 'l', { when: COOL });
    b.T(10, 30, '{s}%', 'M32', CY, 'l', { when: COOL });
    b.T(11, 31, '{s}%', 'M32', ORG, 'l', { when: 'hot' });
    b.T(10, 30, '{s}%', 'M32', OR, 'l', { when: 'hot' });
    b.T(10, 30, '--', 'M32', MD, 'l', { when: NO });
    b.T(230, 32, 'RST {sr@}', 'S12', MD, 'r', { when: OK });
    b.T(230, 32, 'RST --', 'S12', MD, 'r', { when: NO });
    b.T(230, 46, '{sr~}', 'M18', CY, 'r', { when: OK });
    b.T(230, 46, '--', 'M18', MD, 'r', { when: NO });

    // annunciators: dark until their condition holds (lit = ok and cond)
    const an = (x, t, cond, c, o = {}) => {
      b.T(x, 70, t, 'S12', OFF, 'l', { when: `!${cond}` });
      b.T(x, 70, t, 'S12', OFF, 'l', { when: `!ok ${cond}` });
      b.T(x, 70, t, 'S12', c, 'l', { when: `ok ${cond}`, ...o });
    };
    an(10, 'FRESH', 'fresh', CY);
    an(58, 'PACE', 'paceover', OR);
    b.T(98, 70, 'HOT', 'S12', OFF, 'l', { when: '!hot' });
    b.T(98, 70, 'HOT', 'S12', OR, 'l', { when: 'hot', track: b.blink(300) });
    an(130, 'OUT', 'out', OR);
    b.T(230, 70, '{out}', 'S12', OR, 'r', { when: 'hot' });
    b.T(230, 70, 'END {end}%', 'S12', CY, 'r', { when: NORMAL });
    b.T(230, 70, 'NO LINK', 'S12', MD, 'r', { when: NO });

    const meter = (label, y, h, src) => {
      b.T(10, y + Math.round((h - 12) / 2), label, 'S12', MD);
      b.R(30, y, 190, h, OFF, { p: ['v', 8, 6] });
      for (let k = 0; k < 24; k++) b.R(30 + k * 8, y, 6, h, k >= 18 ? OR : CY, { when: OK, gate: [src, (k + 1) / 24] });
    };
    meter('5H', 90, 14, 's');
    meter('WK', 110, 8, 'w');
    b.T(10, 131, 'WEEK', 'S12', MD);
    b.T(48, 128, '{w}%', 'M18', CY, 'l', { when: OK });
    b.T(48, 128, '--', 'M18', MD, 'l', { when: NO });
    b.T(230, 128, '{wr@d}', 'M18', CY, 'r', { when: OK, upper: true });
    b.T(230, 128, '--', 'M18', MD, 'r', { when: NO });

    // spectrum analyser: 12 bars, lit left to right as the 5-hour window elapses
    b.T(10, 158, '5H ELAPSED', 'S12', MD);
    const H = [14, 22, 30, 40, 34, 26, 44, 52, 38, 28, 20, 12];
    H.forEach((h, k) => {
      const x = 10 + k * 10;
      b.R(x, 226 - h, 8, h, OFF, { p: ['h', 4, 3] });
      b.R(x, 226 - h, 8, h, CY, { p: ['h', 4, 3], when: COOL, gate: ['ts', (k + 1) / 12] });
      b.R(x, 226 - h, 8, h, OR, { p: ['h', 4, 3], when: 'hot', gate: ['ts', (k + 1) / 12] });
    });

    b.M('clawd_inner', 160, 152, 2, OFF);
    b.M('clawd_edge', 160, 152, 2, CY, { when: COOL });
    b.M('clawd_edge', 160, 152, 2, OR, { when: 'hot' });
    b.M('clawd_edge', 160, 152, 2, MD, { when: NO });
    b.eyes(160, 152, 2, CY, { when: NORMAL, track: b.eyeBlink(OFF) });
    b.eyes(160, 152, 2, CY, { when: FRESH });
    b.eyes(160, 152, 2, OR, { when: 'hot' });
    b.eyes(160, 152, 2, OFF, { when: NO });
    b.Sp(SPR.drop, 146, 150, 2, OR, { when: 'hot', track: b.bob(3) });
    b.Sp(SPR.spark, 144, 156, 2, CY, { when: FRESH, track: b.blink(400) });
    b.Sp(SPR.spark, 148, 190, 1, CY, { when: FRESH, track: b.blink(400), phase: 400 });
    b.Sp(SPR.z, 146, 162, 1, MD, { when: NO });
    b.Sp(SPR.z, 148, 146, 2, MD, { when: NO });
  },
});
