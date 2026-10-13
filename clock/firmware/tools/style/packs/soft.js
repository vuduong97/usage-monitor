// #45 Soft UI (neumorphism): a raised dial with a ring of 24 dots for the 5-hour window, a
// pressed centre, a groove for the week and a raised Clawd button (Batch E, segmented ring).
// Every soft shape is three flat copies: light at -1,-1, dark at +1,+1, the face on top.
const { mock } = require('../mock');
const { OK, NO, NORMAL, FRESH, SPARK, lid, dripTrack, drop, blinkA } = require('./_e1_common');

const BG = '#e4e8ee', HI = '#ffffff', SH = '#b8c1cd', INK = '#3e4756', SUB = '#7d8797', OFF = '#cbd2dc';
const OR = '#d97757', RED = '#e5484d', GRN = '#2fa36b', DIM = '#b3bbc6';

module.exports = mock({
  id: 'soft', name: 'Soft UI', bg: BG, light: true,
  draw(b) {
    const blink = blinkA(b);
    b.T(14, 10, '{clock}', 'S20', INK);
    b.T(226, 14, '{dow}', 'S12', SUB, 'r');

    // raised dial, then the ring: 24 dots on r64 from 12 o'clock, clockwise; dot i is lit
    // when round(24 s) > i, i.e. s >= (i + 0.5) / 24, in the state's accent colour
    b.C(119, 117, 74, HI);
    b.C(121, 119, 74, SH);
    b.C(120, 118, 74, BG);
    const dots = [];
    for (let i = 0; i < 24; i++) {
      const a = (-90 + i * 15) * Math.PI / 180;
      dots.push([Math.round(120 + 64 * Math.cos(a)) , Math.round(118 + 64 * Math.sin(a)), (i + 0.5) / 24]);
    }
    for (const [x, y] of dots) b.C(x, y, 4, OFF);
    for (const [x, y, thr] of dots) {
      b.C(x, y, 4, OR, { when: NORMAL, gate: ['s', thr] });
      if (thr < 0.8) b.C(x, y, 4, GRN, { when: FRESH, gate: ['s', thr] });
      b.C(x, y, 4, RED, thr < 0.8 ? { when: 'hot' } : { when: 'hot', gate: ['s', thr] });
    }

    // pressed centre
    b.C(119, 117, 54, SH);
    b.C(121, 119, 54, HI);
    b.C(120, 118, 54, BG);
    b.T(120, 76, '5 HOUR', 'S12', SUB, 'c');
    b.T(120, 92, '{s}%', 'S48', INK, 'c', { when: 'ok !hot' });
    b.T(120, 92, '{s}%', 'S48', RED, 'c', { when: 'hot' });
    b.T(120, 92, '--%', 'S48', DIM, 'c', { when: NO });
    b.T(120, 146, 'reset {sr@}', 'S12', SUB, 'c', { when: 'ok !hot' });
    b.T(120, 146, 'out {out}', 'S12', RED, 'c', { when: 'hot', track: blink });
    b.T(120, 146, '--:--', 'S12', DIM, 'c', { when: NO });

    // week: label, number, reset and the groove
    b.T(14, 200, 'WEEK', 'S12', SUB);
    b.T(14, 214, '{w}%', 'S20', INK, 'l', { when: 'ok !hot' });
    b.T(14, 214, '{w}%', 'S20', RED, 'l', { when: 'hot' });
    b.T(14, 214, '--%', 'S20', DIM, 'l', { when: NO });
    b.T(66, 200, '{wr@d}', 'S12', SUB, 'l', { when: OK });
    b.T(66, 200, '--', 'S12', SUB, 'l', { when: NO });
    b.R(65, 215, 114, 10, SH, { r: 5 });
    b.R(67, 217, 114, 10, HI, { r: 5 });
    b.R(66, 216, 114, 10, OFF, { r: 5 });
    b.R(66, 216, 0, 10, OR, { r: 5, when: NORMAL, x1: ['w', 114], minW: 10, zeroOk: true });
    b.R(66, 216, 0, 10, GRN, { r: 5, when: FRESH, x1: ['w', 114], minW: 10, zeroOk: true });
    b.R(66, 216, 0, 10, RED, { r: 5, when: 'hot', x1: ['w', 114], minW: 10, zeroOk: true });

    // Clawd's button: raised, pressed while hot, flat with no data
    b.R(187, 187, 44, 44, HI, { r: 12, when: '!hot' });
    b.R(189, 189, 44, 44, SH, { r: 12, when: '!hot' });
    b.R(187, 187, 44, 44, SH, { r: 12, when: 'hot' });
    b.R(189, 189, 44, 44, HI, { r: 12, when: 'hot' });
    b.R(188, 188, 44, 44, BG, { r: 12 });
    b.clawd(194, 190, 1, { body: OR, when: OK });
    b.clawd(194, 190, 1, { body: DIM, when: NO });
    lid(b, 194, 190, 1, DIM, SUB, { when: NO });
    b.T(178, 188, 'z', 'S14', SUB, 'l', { when: NO, track: blink });
    b.T(170, 200, 'z', 'S12', SUB, 'l', { when: NO, track: blink, phase: 500 });
    drop(b, 179, 190, RED, dripTrack(b, 8), { when: 'hot' });
    b.Sp(SPARK, 176, 196, 2, GRN, { when: FRESH, track: blink });
  },
});
