// #46 OLED black: pixels off, one thin accent per state (mint, or red when hot), two rails
// for usage vs. time; Clawd walks the 5-hour rail (Batch E, layout: timeline).
const { mock, SPR } = require('../mock');

const K = '#000000', W = '#ffffff', G = '#8a8a8a', RAIL = '#2e2e2e', MINT = '#3cf2c4', RED = '#ff453a', DIM = '#4a4a4a';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot', FRESH = 'ok fresh !hot';

module.exports = mock({
  id: 'oled', name: 'OLED Black', bg: K,
  draw(b) {
    b.T(8, 10, '{clock}', 'M18', W);
    b.T(68, 14, '{dow}', 'S12', G, 'l', { upper: true });
    b.T(8, 40, '5-HOUR', 'S12', G);
    b.T(6, 54, '{s}%', 'S48', W, 'l', { when: COOL });
    b.T(6, 54, '{s}%', 'S48', RED, 'l', { when: 'hot' });
    b.T(6, 54, '--%', 'S48', DIM, 'l', { when: NO });
    b.T(232, 58, 'resets {sr@}', 'S12', G, 'r', { when: OK });
    b.T(232, 58, 'resets --:--', 'S12', G, 'r', { when: NO });
    b.T(232, 76, '{sr~}', 'M18', MINT, 'r', { when: COOL });
    b.T(232, 76, 'OUT {out}', 'M18', RED, 'r', { when: 'hot' });
    b.T(232, 76, '--', 'M18', DIM, 'r', { when: NO });

    // Clawd stands on the 5-hour rail at x = clamp(w - 8, 6, 202), w = 225 * used: three
    // ops per colour, because a position clamp is not an op flag for masks.
    const LO = 0.0644, HI = 0.9311;
    const bob = b.S.track(200, [{ dy: 0 }, { dy: -2 }]);
    b.clawd(6, 104, 1, { body: MINT, when: COOL, gate: ['s', 0, LO] });
    b.clawd(-8, 104, 1, { body: MINT, when: COOL, gate: ['s', LO, HI], dx: ['s', 225] });
    b.clawd(-8, 104, 1, { body: RED, when: 'hot', gate: ['s', LO, HI], dx: ['s', 225], track: bob });
    b.clawd(202, 104, 1, { body: RED, when: 'hot', gate: ['s', HI], track: bob });
    b.clawd(6, 104, 1, { body: DIM, when: NO });
    // asleep: the eye holes are filled in (a closed lid), z z
    b.R(12, 116, 2, 2, DIM, { when: NO }); b.R(30, 116, 2, 2, DIM, { when: NO });
    b.R(10, 118, 4, 1, DIM, { when: NO }); b.R(30, 118, 4, 1, DIM, { when: NO });
    b.T(42, 100, 'z', 'S14', G, 'l', { when: NO, track: b.blink(500) });
    b.T(50, 112, 'z', 'S12', G, 'l', { when: NO, track: b.blink(500), phase: 500 });
    b.Sp(SPR.spark, 42, 106, 2, MINT, { when: FRESH, track: b.blink(500) });

    // the 5-hour rail: 6 hour ticks, usage fill, a white "now" tick for the time elapsed
    b.R(8, 146, 225, 1, RAIL);
    for (const x of [8, 53, 98, 143, 188, 232]) b.R(x, 149, 1, 3, G);
    b.R(8, 145, 0, 2, MINT, { when: COOL, x1: ['s', 225] });
    b.R(8, 145, 0, 2, RED, { when: 'hot', x1: ['s', 225] });
    b.R(8, 148, 1, 8, W, { when: OK, dx: ['ts', 225] });
    b.P([[227, 137], [234, 137], [230, 143]], RED, { when: 'hot', track: b.blink(500) });
    b.T(8, 160, 'start', 'S12', G, 'l', { when: OK });
    b.T(8, 160, '--:--', 'S12', G, 'l', { when: NO });
    b.T(232, 160, '{sr@}', 'S12', G, 'r', { when: OK });
    b.T(232, 160, '--:--', 'S12', G, 'r', { when: NO });

    b.T(8, 180, 'WEEK', 'S12', G);
    b.T(7, 196, '{w}%', 'S24', W, 'l', { when: COOL });
    b.T(7, 196, '{w}%', 'S24', RED, 'l', { when: 'hot' });
    b.T(7, 196, '--%', 'S24', DIM, 'l', { when: NO });
    b.T(232, 182, 'resets {wr@d}', 'S12', G, 'r', { when: OK });
    b.T(232, 182, 'resets --', 'S12', G, 'r', { when: NO });
    b.T(232, 198, '{wr~}', 'M18', MINT, 'r', { when: COOL });
    b.T(232, 198, '{wr~}', 'M18', RED, 'r', { when: 'hot' });
    b.T(232, 198, '--', 'M18', DIM, 'r', { when: NO });

    b.R(8, 228, 225, 1, RAIL);
    for (const x of [8, 40, 72, 104, 136, 168, 200, 232]) b.R(x, 224, 1, 3, G);
    b.R(8, 227, 0, 2, MINT, { when: COOL, x1: ['w', 225] });
    b.R(8, 227, 0, 2, RED, { when: 'hot', x1: ['w', 225] });
    b.R(8, 222, 1, 10, W, { when: OK, dx: ['tw', 225] });
  },
});
