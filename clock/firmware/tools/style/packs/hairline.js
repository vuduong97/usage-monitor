// #41 Hairline minimal: flat off-white, two big numbers, 1 px rules for the two bars
// (Batch E, split screen: 5-hour block over the week block).
const { mock } = require('../mock');
const { OK, NO, NORMAL, FRESH, SPARK, lid, dripTrack, drop, blinkA } = require('./_e1_common');

const BG = '#f7f7f5', INK = '#111111', LAB = '#8c8c88', RULE = '#d4d4cf', RED = '#e0321c';
const OR = '#d97757', ASLEEP = '#b8b8b3', DIM = '#c4c4bf';

module.exports = mock({
  id: 'hairline', name: 'Hairline', bg: BG, light: true,
  draw(b) {
    const blink = blinkA(b);
    b.T(14, 16, '{clock}', 'S16', INK);
    b.T(64, 19, '{dow}', 'S12', LAB);
    b.clawd(194, 10, 1, { body: OR, when: OK });
    b.clawd(194, 10, 1, { body: ASLEEP, when: NO });
    lid(b, 194, 10, 1, ASLEEP, LAB, { when: NO });
    b.T(182, 8, 'z', 'S12', LAB, 'l', { when: NO, track: blink });
    b.T(174, 20, 'z', 'S12', LAB, 'l', { when: NO, track: blink, phase: 500 });
    drop(b, 184, 12, RED, dripTrack(b, 4), { when: 'hot' });
    b.Sp(SPARK, 178, 14, 2, INK, { when: FRESH, track: blink });

    b.R(14, 58, 212, 1, RULE);
    b.T(14, 74, '5-HOUR', 'S12', LAB);
    b.T(12, 88, '{s}%', 'S48', INK, 'l', { when: 'ok !hot' });
    b.T(12, 88, '{s}%', 'S48', RED, 'l', { when: 'hot' });
    b.T(12, 88, '--%', 'S48', DIM, 'l', { when: NO });
    b.T(226, 92, 'resets {sr@}', 'S12', LAB, 'r', { when: OK });
    b.T(226, 92, 'resets --:--', 'S12', LAB, 'r', { when: NO });
    b.T(226, 108, 'in {sr~}', 'S14', INK, 'r', { when: OK });
    b.T(226, 108, 'in --', 'S14', DIM, 'r', { when: NO });
    b.T(226, 128, 'out at {out}', 'S12', RED, 'r', { when: 'hot' });
    b.T(226, 128, 'new window', 'S12', LAB, 'r', { when: FRESH });
    // 5h rule: track with end caps, fill 211 px * s, and a 1x11 tick at the fill's end
    b.R(14, 147, 1, 7, RULE);
    b.R(225, 147, 1, 7, RULE);
    b.R(14, 150, 212, 1, RULE);
    b.R(14, 150, 0, 1, INK, { when: 'ok !hot', x1: ['s', 211] });
    b.R(14, 150, 0, 1, RED, { when: 'hot', x1: ['s', 211] });
    b.R(14, 145, 1, 11, INK, { when: 'ok !hot', dx: ['s', 211] });
    b.R(14, 145, 1, 11, RED, { when: 'hot', dx: ['s', 211], track: blink });

    b.T(14, 172, 'WEEK', 'S12', LAB);
    b.T(13, 188, '{w}%', 'S24', INK, 'l', { when: 'ok !hot' });
    b.T(13, 188, '{w}%', 'S24', RED, 'l', { when: 'hot' });
    b.T(13, 188, '--%', 'S24', DIM, 'l', { when: NO });
    b.T(226, 176, 'resets {wr@d}', 'S12', LAB, 'r', { when: OK });
    b.T(226, 176, 'resets --', 'S12', LAB, 'r', { when: NO });
    b.T(226, 192, 'in {wr~}', 'S14', INK, 'r', { when: OK });
    b.T(226, 192, 'in --', 'S14', DIM, 'r', { when: NO });
    b.R(14, 219, 1, 7, RULE);
    b.R(225, 219, 1, 7, RULE);
    b.R(14, 222, 212, 1, RULE);
    b.R(14, 222, 0, 1, INK, { when: 'ok !hot', x1: ['w', 211] });
    b.R(14, 222, 0, 1, RED, { when: 'hot', x1: ['w', 211] });
    b.R(14, 217, 1, 11, INK, { when: 'ok !hot', dx: ['w', 211] });
    b.R(14, 217, 1, 11, RED, { when: 'hot', dx: ['w', 211] });
  },
});
