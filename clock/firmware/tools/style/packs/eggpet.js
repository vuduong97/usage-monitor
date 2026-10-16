// #27 Tamagotchi: an orange egg toy on a purple ground, a green LCD with the 5-hour % and two
// ten-segment meters (5H, WK), and Clawd as the pet that hops side to side
// (Batch C, archetype 3: two vertical meters).
const { mock } = require('../mock');
const { rectRows, eggFan } = require('./_c2_util');

const BG = '#2e2150', RIM = '#b86e10', SHELL = '#f5b82e', LCD = '#b7c4a1', DK = '#1e2a1a', GH = '#9fad8a', OFF = '#5e6a55',
  PALE = '#c8d4b2', BTN = '#d97757', RED = '#c0182a';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot';

const BANG = rectRows([[1, 0, 3, 5], [1, 6, 3, 2]], 5, 8);
const HEART = rectRows([[1, 0, 2, 1], [4, 0, 2, 1], [0, 1, 7, 2], [1, 3, 5, 1], [2, 4, 3, 1], [3, 5, 1, 1]], 7, 6);
const DROP = rectRows([[2, 0, 1, 1], [1, 1, 3, 1], [0, 2, 5, 2], [1, 4, 3, 1]], 5, 5);

module.exports = mock({
  id: 'eggpet', name: 'Tamagotchi', bg: BG,
  draw(b) {
    // the egg: a hanging loop, a rim 2 px wider than the shell (stadium on top, disc below), the shell
    b.R(112, 0, 16, 12, RIM, { r: 6 });
    b.R(113, 1, 14, 10, BG, { r: 5 });
    for (const t of eggFan(2)) b.P(t, RIM);
    for (const t of eggFan(0)) b.P(t, SHELL);
    b.T(120, 22, 'CLAWD', 'S12', BG, 'c');

    // the LCD: frame, glass (lights off with no data), the 5H % (inverted while hot)
    b.R(48, 38, 144, 136, BG, { r: 12 });
    b.R(52, 42, 136, 128, LCD, { r: 6, when: OK });
    b.R(52, 42, 136, 128, OFF, { r: 6, when: NO });
    b.R(56, 46, 66, 36, DK, { when: 'hot' });
    b.T(58, 48, '{s}%', 'M32', LCD, 'l', { when: 'hot' });
    b.T(58, 48, '{s}%', 'M32', DK, 'l', { when: COOL });
    b.T(58, 48, '--', 'M32', PALE, 'l', { when: NO });
    b.T(58, 86, '5H USED', 'S12', DK, 'l', { when: OK });
    b.T(58, 86, '5H USED', 'S12', PALE, 'l', { when: NO });
    b.T(182, 48, '{clock}', 'S14', DK, 'r', { when: OK });
    b.T(182, 48, '{clock}', 'S14', PALE, 'r', { when: NO });

    // meters: ghost segments, 10 per column, 8 px apart; lit from the bottom as the value reaches k x 10 %
    for (const [x, src] of [[150, 's'], [168, 'w']]) {
      b.R(x, 68, 12, 78, GH, { p: ['h', 8, 6], when: OK });
      b.R(x, 68, 12, 78, DK, { p: ['h', 8, 6], when: NO });
      for (let k = 0; k < 10; k++) b.R(x, 140 - 8 * k, 12, 6, DK, { when: OK, gate: [src, (k + 1) / 10] });
    }
    b.T(155, 152, '5H', 'S12', DK, 'c', { when: OK });
    b.T(155, 152, '5H', 'S12', PALE, 'c', { when: NO });
    b.T(176, 152, 'WK', 'S12', DK, 'c', { when: OK });
    b.T(176, 152, 'WK', 'S12', PALE, 'c', { when: NO });
    b.T(58, 150, 'WK {w}%', 'S14', DK, 'l', { when: OK });
    b.T(58, 150, 'WK --', 'S14', PALE, 'l', { when: NO });

    // the pet: hops dx +-2 every 400 ms (calm only). Its eyes follow the hop and take the body
    // colour for 150 ms about every 3 s, so they share one 50 ms track of two hop cycles.
    const hop = b.S.track(400, [{ dx: 0 }, { dx: 2 }, { dx: 0 }, { dx: -2 }]);
    const hopBlink = b.S.track(50, Array.from({ length: 64 }, (_, i) => ({
      dx: [0, 2, 0, -2][(i >> 3) & 3], color: i >= 61 ? DK : undefined,
    })));
    b.M('clawd', 66, 104, 1, DK, { when: COOL, track: hop });
    b.M('clawd', 66, 104, 1, DK, { when: 'hot' });
    b.M('clawd', 66, 104, 1, GH, { when: NO });
    b.M('clawd_eyes', 66, 104, 1, LCD, { when: COOL, track: hopBlink });
    b.M('clawd_eyes', 66, 104, 1, LCD, { when: 'hot' });
    b.M('clawd_eyes', 66, 104, 1, GH, { when: NO });
    b.R(70, 119, 4, 1, OFF, { when: NO });
    b.R(90, 119, 4, 1, OFF, { when: NO });

    // hot: "!" and a sweat drop; fresh: a heart and YAY; no data: Z z
    const blink = b.blink(500);
    b.Sp(BANG, 112, 98, 2, DK, { when: 'hot', track: blink });
    b.Sp(DROP, 104, 120, 2, DK, { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]) });
    b.Sp(HEART, 108, 100, 2, DK, { when: 'ok fresh !hot', track: blink });
    b.T(104, 118, 'YAY', 'S12', DK, 'l', { when: 'ok fresh !hot' });
    b.T(106, 98, 'Z', 'S16', PALE, 'l', { when: NO, track: blink });
    b.T(116, 112, 'z', 'S12', PALE, 'l', { when: NO });

    // the shell line and the three buttons
    b.T(120, 180, 'RESET {sr@}  IN {sr~}', 'S12', BG, 'c', { when: COOL });
    b.T(120, 180, 'OUT AT {out}  IN {sr~}', 'S12', RED, 'c', { when: 'hot' });
    b.T(120, 180, 'WAITING FOR APP', 'S12', BG, 'c', { when: NO });
    b.R(83, 197, 18, 18, BTN, { r: 9 });
    b.R(111, 203, 18, 18, BTN, { r: 9 });
    b.R(139, 197, 18, 18, BTN, { r: 9 });
  },
});
