// #43 Material tonal: peach surface, fat tonal pills for the two windows, a clock chip
// and a round Clawd button (Batch E, two bars). Hot swaps to the error tones.
const { mock } = require('../mock');
const { OK, NO, NORMAL, FRESH, SPARK, lid, dripTrack, drop, blinkA } = require('./_e1_common');

const BG = '#fdf1ec', ON = '#3a1205', SEC = '#8a5a48', OR = '#d97757', WF = '#e9a385', ERR = '#ba1a1a';
const ERRC = '#ffdad6', CONT = '#ffdbcd', W = '#ffffff', DIM = '#c9b8b0';

module.exports = mock({
  id: 'tonal', name: 'Material tonal', bg: BG, light: true,
  draw(b) {
    const blink = blinkA(b);
    // chip + button (the pill colour follows the state)
    for (const [when, c] of [['!hot', CONT], ['hot', ERRC]]) {
      b.R(8, 8, 116, 40, c, { r: 20, when });
      b.R(178, 6, 52, 52, c, { r: 26, when });
    }
    b.T(20, 16, '{clock}', 'S24', ON);
    b.T(92, 22, '{dow}', 'S12', SEC);
    b.clawd(188, 12, 1, { body: OR, when: OK });
    b.clawd(188, 12, 1, { body: DIM, when: NO });
    lid(b, 188, 12, 1, DIM, SEC, { when: NO });
    b.T(166, 10, 'z', 'S14', SEC, 'l', { when: NO, track: blink });
    b.T(158, 24, 'z', 'S12', SEC, 'l', { when: NO, track: blink, phase: 500 });
    drop(b, 222, 8, ERR, dripTrack(b, 8), { when: 'hot' });
    b.Sp(SPARK, 162, 12, 2, ON, { when: FRESH, track: blink });

    // 5-hour pill
    b.T(12, 70, '5-hour', 'S14', ON);
    b.T(228, 72, 'out {out} · {sr~}', 'S12', ERR, 'r', { when: 'hot', track: blink });
    b.T(228, 72, 'new window · {sr@}', 'S12', SEC, 'r', { when: FRESH });
    b.T(228, 72, '{sr@} · {sr~} left', 'S12', SEC, 'r', { when: NORMAL });
    b.T(228, 72, '--', 'S12', DIM, 'r', { when: NO });
    b.R(8, 90, 224, 72, CONT, { r: 36, when: '!hot' });
    b.R(8, 90, 224, 72, ERRC, { r: 36, when: 'hot' });
    b.R(8, 90, 0, 72, OR, { r: 36, when: 'ok !hot', x1: ['s', 224], minW: 72, zeroOk: true });
    b.R(8, 90, 0, 72, ERR, { r: 36, when: 'hot', x1: ['s', 224], minW: 72, zeroOk: true });
    b.T(26, 102, '{s}%', 'S48', ON, 'l', { when: 'ok !hot' });
    b.T(26, 102, '{s}%', 'S48', W, 'l', { when: 'hot' });
    b.T(26, 102, '--%', 'S48', DIM, 'l', { when: NO });

    // week pill
    b.T(12, 176, 'Week', 'S14', ON);
    b.T(228, 178, '{wr@d} · {wr~}', 'S12', SEC, 'r', { when: OK });
    b.T(228, 178, '--', 'S12', DIM, 'r', { when: NO });
    b.R(8, 196, 224, 36, CONT, { r: 18, when: '!hot' });
    b.R(8, 196, 224, 36, ERRC, { r: 18, when: 'hot' });
    b.R(8, 196, 0, 36, WF, { r: 18, when: 'ok !hot', x1: ['w', 224], minW: 36, zeroOk: true });
    b.R(8, 196, 0, 36, ERR, { r: 18, when: 'hot', x1: ['w', 224], minW: 36, zeroOk: true });
    b.T(22, 204, '{w}%', 'S20', ON, 'l', { when: 'ok !hot' });
    b.T(22, 204, '{w}%', 'S20', W, 'l', { when: 'hot' });
    b.T(22, 204, '--%', 'S20', DIM, 'l', { when: NO });
  },
});
