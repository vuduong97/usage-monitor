// #49 Frosted panels: glass cards over a four-band dusk sky. There is no blur, so each
// panel is cut at the band lines and every piece is its band mixed 40% with white
// (Batch E, layout: layered glass cards).
const { mock, SPR } = require('../mock');

const B1 = '#2b2d6e', B2 = '#5b3a8c', B3 = '#b5508a', B4 = '#f08a5d';
const F1 = '#8081a8', F2 = '#9d89ba', F3 = '#d396b9', F4 = '#f6b99e';
const W = '#ffffff', CL = '#d97757', HOTC = '#ff4d4d';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot', FRESH = 'ok fresh !hot';

module.exports = mock({
  id: 'frost', name: 'Frosted Panels', bg: B1,
  draw(b) {
    b.R(0, 60, 240, 60, B2);
    b.R(0, 120, 240, 60, B3);
    b.R(0, 180, 240, 60, B4);
    b.T(12, 16, '{clock}', 'S24', W);
    b.T(82, 24, '{dow}', 'S12', W);

    // the big panel, cut at the band lines, and the week panel
    b.R(12, 52, 216, 16, F1, { r: 8 });
    b.R(12, 60, 216, 60, F2);
    b.R(12, 120, 216, 40, F3);
    b.R(12, 148, 216, 24, F3, { r: 8 });
    b.R(20, 52, 200, 1, W);

    // Clawd stands on the big panel
    b.clawd(188, 12, 1, { body: CL, when: OK });
    b.clawd(188, 12, 1, { body: F1, when: NO });
    for (const [dx, dy, w, h] of [[6, 12, 2, 2], [24, 12, 2, 2], [4, 14, 4, 1], [24, 14, 4, 1]]) b.R(188 + dx, 12 + dy, w, h, F1, { when: NO });
    for (const [dx, dy, w, h] of [[4, 15, 4, 1], [24, 15, 4, 1]]) b.R(188 + dx, 12 + dy, w, h, B1, { when: NO });
    b.T(172, 12, 'z', 'S14', W, 'l', { when: NO, track: b.blink(500) });
    b.T(164, 26, 'z', 'S12', W, 'l', { when: NO, track: b.blink(500), phase: 500 });
    b.Sp(SPR.drop, 178, 14, 2, W, { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }, { dy: 6 }]) });
    b.Sp(SPR.spark, 172, 18, 2, W, { when: FRESH, track: b.blink(500) });
    b.R(166, 62, 50, 16, W, { when: FRESH, r: 8 });
    b.T(180, 64, 'NEW', 'S12', B1, 'l', { when: FRESH });

    b.T(24, 66, '5-HOUR', 'S12', B1);
    b.T(22, 80, '{s}%', 'S48', B1, 'l', { when: OK });
    b.T(22, 80, '--%', 'S48', B2, 'l', { when: NO });
    b.T(216, 84, 'reset {sr@}', 'S12', B1, 'r', { when: OK });
    b.T(216, 84, 'reset --:--', 'S12', B1, 'r', { when: NO });
    b.T(216, 100, '{sr~}', 'S14', B1, 'r', { when: COOL });
    b.T(216, 100, '--', 'S14', B1, 'r', { when: NO });
    b.R(138, 98, 78, 20, HOTC, { when: 'hot', r: 10, track: b.blink(500) });
    b.T(146, 102, 'OUT {out}', 'S12', W, 'l', { when: 'hot', track: b.blink(500) });

    // the 5-hour bar: a navy hairline track and a white (red when hot) fill
    b.R(24, 152, 192, 1, B1);
    b.R(24, 148, 0, 9, W, { when: COOL, r: 4, x1: ['s', 192], minW: 9, zeroOk: true });
    b.R(24, 148, 0, 9, HOTC, { when: 'hot', r: 4, x1: ['s', 192], minW: 9, zeroOk: true });

    b.R(12, 182, 216, 46, F4, { r: 12 });
    b.R(24, 182, 192, 1, W);
    b.T(24, 188, 'WEEK', 'S12', B1);
    b.T(23, 202, '{w}%', 'S24', B1, 'l', { when: OK });
    b.T(23, 202, '--%', 'S24', B2, 'l', { when: NO });
    b.T(216, 190, 'reset {wr@d}', 'S12', B1, 'r', { when: OK });
    b.T(216, 190, 'reset --', 'S12', B1, 'r', { when: NO });
    b.T(216, 206, '{wr~}', 'S14', B1, 'r', { when: OK });
    b.T(216, 206, '--', 'S14', B2, 'r', { when: NO });
  },
});
