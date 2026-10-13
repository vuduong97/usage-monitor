// #42 Phone widget: light grey, one wide card for the 5-hour window and two small
// cards for the week and the clock (Batch E, card stack). Hot turns the wide card red.
const { mock } = require('../mock');
const { OK, NO, NORMAL, FRESH, SPARK, lid, dripTrack, drop, blinkA } = require('./_e1_common');

const BG = '#e6e7eb', W = '#ffffff', INK = '#1c1c1e', GREY = '#8e8e93', TRACK = '#ececef';
const OR = '#d97757', RED = '#e5484d', GRN = '#30a46c', DIM = '#b5b6bb', HOTTRACK = '#f4a9ab';

module.exports = mock({
  id: 'widget', name: 'Phone widget', bg: BG, light: true,
  draw(b) {
    const blink = blinkA(b);
    // card A: 5-hour
    b.R(8, 8, 224, 124, W, { r: 16, when: '!hot' });
    b.R(8, 8, 224, 124, RED, { r: 16, when: 'hot' });
    b.T(20, 20, '5-hour limit', 'S12', GREY, 'l', { when: '!hot' });
    b.T(20, 20, '5-hour limit', 'S12', W, 'l', { when: 'hot' });
    b.C(102, 26, 4, W, { when: 'hot', track: blink });
    b.T(18, 36, '{s}%', 'S48', INK, 'l', { when: 'ok !hot' });
    b.T(18, 36, '{s}%', 'S48', W, 'l', { when: 'hot' });
    b.T(18, 36, '--%', 'S48', DIM, 'l', { when: NO });
    b.T(20, 92, '{sr~} left', 'S12', GREY, 'l', { when: NORMAL });
    b.T(20, 92, 'out at {out}', 'S12', W, 'l', { when: 'hot' });
    b.T(20, 92, 'new window · {sr~}', 'S12', GRN, 'l', { when: FRESH });
    b.T(20, 92, 'no data', 'S12', DIM, 'l', { when: NO });
    b.R(20, 110, 200, 8, TRACK, { r: 4, when: '!hot' });
    b.R(20, 110, 200, 8, HOTTRACK, { r: 4, when: 'hot' });
    b.R(20, 110, 0, 8, OR, { r: 4, when: NORMAL, x1: ['s', 200], minW: 8, zeroOk: true });
    b.R(20, 110, 0, 8, GRN, { r: 4, when: FRESH, x1: ['s', 200], minW: 8, zeroOk: true });
    b.R(20, 110, 0, 8, W, { r: 4, when: 'hot', x1: ['s', 200], minW: 8, zeroOk: true });

    // Clawd at 2x, asleep when there is no data
    b.clawd(156, 20, 2, { body: OR, when: NORMAL });
    b.clawd(156, 20, 2, { body: OR, when: FRESH });
    b.clawd(156, 20, 2, { body: W, when: 'hot' });
    b.clawd(156, 20, 2, { body: DIM, when: NO });
    lid(b, 156, 20, 2, DIM, GREY, { when: NO });
    b.T(140, 22, 'z', 'S14', GREY, 'l', { when: NO, track: blink });
    b.T(130, 36, 'z', 'S12', GREY, 'l', { when: NO, track: blink, phase: 500 });
    drop(b, 146, 22, W, dripTrack(b, 8), { when: 'hot' });
    b.Sp(SPARK, 138, 24, 2, GRN, { when: FRESH, track: blink });

    // card B: week
    b.R(8, 140, 108, 92, W, { r: 16 });
    b.T(20, 152, 'Week', 'S12', GREY);
    b.T(19, 168, '{w}%', 'S28', INK, 'l', { when: 'ok !hot' });
    b.T(19, 168, '{w}%', 'S28', RED, 'l', { when: 'hot' });
    b.T(19, 168, '--%', 'S28', DIM, 'l', { when: NO });
    b.T(20, 200, '{wr@d}', 'S12', GREY, 'l', { when: OK });
    b.T(20, 200, '--', 'S12', GREY, 'l', { when: NO });
    b.R(20, 218, 84, 6, TRACK, { r: 3 });
    b.R(20, 218, 0, 6, OR, { r: 3, when: NORMAL, x1: ['w', 84], minW: 6, zeroOk: true });
    b.R(20, 218, 0, 6, GRN, { r: 3, when: FRESH, x1: ['w', 84], minW: 6, zeroOk: true });
    b.R(20, 218, 0, 6, RED, { r: 3, when: 'hot', x1: ['w', 84], minW: 6, zeroOk: true });

    // card C: clock
    b.R(124, 140, 108, 92, W, { r: 16 });
    b.T(136, 152, '{dow}', 'S12', GREY);
    b.T(135, 168, '{clock}', 'S28', INK);
    b.T(136, 200, 'reset {sr@}', 'S12', GREY, 'l', { when: 'ok !hot' });
    b.T(136, 200, 'reset {sr@}', 'S12', RED, 'l', { when: 'hot' });
    b.T(136, 200, 'reset --:--', 'S12', GREY, 'l', { when: NO });
  },
});
