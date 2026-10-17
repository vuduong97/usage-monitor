// #40 Sonar screen: a radar scope whose sweep is the elapsed 5-hour window, 20 week
// dots on an outer arc, and Clawd as the contact closing on your own ship as the
// 5-hour usage rises (Batch D, archetype 10: timeline radar).
const { mock } = require('../mock');
const { SPD, tracks, clawd } = require('./_d2_kit');

const BG = '#031208', RING = '#0f4a26', DIM = '#4e9e68', BRIGHT = '#55f28a', GLOW = '#0c3a1e', WHITE = '#e6ffef';
const RED = '#ff4b3e', REDRING = '#4d1512', CLAWD = '#d97757', CGREY = '#4f5a53', GRING = '#1e2a23';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot';
const CX = 120, CY = 96, BOX = 176, X0 = CX - 88, Y0 = CY - 88;

const polar = (cx, cy, r, deg) => {
  const a = deg * Math.PI / 180;
  return [Math.round(cx + r * Math.sin(a)), Math.round(cy - r * Math.cos(a))];
};
const DOTS = Array.from({ length: 20 }, (_, k) => polar(CX, CY, 77, -128.25 + 13.5 * k));

module.exports = mock({
  id: 'sonar', name: 'Sonar screen', bg: BG,
  draw(b) {
    const S = b.S;
    const T = tracks(b);
    // The scope as two 1-bit layers in the ring colour (one op per state colour): the four
    // rings, 1 px wide, under the sweep; the crosshair and the 20 unlit week dots over it.
    S.defMask('rings', BOX, BOX, (x, y) => {
      const dx = X0 + x + 0.5 - CX, dy = Y0 + y + 0.5 - CY, d2 = dx * dx + dy * dy;
      return [88, 66, 44, 22].some((r) => d2 <= r * r && d2 > (r - 1) * (r - 1));
    });
    S.defMask('cross', BOX, BOX, (x, y) => {
      const px = X0 + x, py = Y0 + y;
      if (py === CY || px === CX) return true;
      return DOTS.some(([qx, qy]) => (px + 0.5 - qx) ** 2 + (py + 0.5 - qy) ** 2 <= 4);
    });
    const layer = (name) => {
      S.mask(name, X0, Y0, 1, GRING, { when: NO });
      S.mask(name, X0, Y0, 1, RING, { when: COOL });
      S.mask(name, X0, Y0, 1, REDRING, { when: 'hot' });
    };
    layer('rings');

    // the sweep: 12 positions of the elapsed 5-hour window, each an afterglow wedge plus the beam
    for (let k = 0; k < 12; k++) {
      const a = (k + 0.5) * 30;
      const gate = k === 11 ? ['ts', k / 12] : ['ts', k / 12, (k + 1) / 12];
      const wedge = [[CX, CY], polar(CX, CY, 86, a), polar(CX, CY, 86, a - 28)];
      S.tri(wedge, GLOW, { when: COOL, gate });
      S.tri(wedge, REDRING, { when: 'hot', gate });
      S.line(CX, CY, ...polar(CX, CY, 86, a), BRIGHT, { when: COOL, gate });
      S.line(CX, CY, ...polar(CX, CY, 86, a), RED, { when: 'hot', gate });
    }
    layer('cross');
    // the week: dot k is lit once the week reaches (k + 0.5) x 5 %
    DOTS.forEach(([qx, qy], k) => {
      const gate = ['w', (k + 0.5) * 5 / 100];
      b.C(qx, qy, 2, BRIGHT, { when: 'ok !whot', gate });
      b.C(qx, qy, 2, WHITE, { when: 'whot', gate });
    });
    b.T(CX, 164, 'WEEK', 'S12', DIM, 'c');
    // own ship: pings every second unless hot or without data
    b.C(CX, CY, 3, WHITE, { when: COOL, track: b.blink(500) });
    b.C(CX, CY, 3, WHITE, { when: 'hot' });
    b.C(CX, CY, 3, WHITE, { when: NO });

    // the contact: Clawd sits at x = 176 - 72 x usage, so it moves left as 5H rises
    const slide = { dx: ['s', -72] };
    clawd(b, 176, 76, 1, CLAWD, CGREY, BG, COOL, ['hot'], slide);
    b.Sp(SPD.drop, 169, 74, 1, WHITE, { when: 'hot', track: T.drip, ...slide });
    b.Sp(SPD.spark, 210, 70, 1, BRIGHT, { when: 'ok fresh', ...slide });
    b.Sp(SPD.zz, 206, 60, 1, DIM, { when: NO });

    // status tag, top left
    b.R(6, 6, 52, 16, RED, { r: 2, when: 'hot', track: b.blink(500) });
    b.T(32, 8, 'ALERT', 'S12', BG, 'c', { when: 'hot', track: b.blink(500) });
    b.T(8, 8, 'WAITING', 'S12', DIM, 'l', { when: NO });
    b.T(8, 8, 'CLEAR', 'S12', BRIGHT, 'l', { when: 'ok fresh !hot' });
    b.T(8, 8, 'TRACKING', 'S12', DIM, 'l', { when: 'ok !hot !fresh' });
    b.T(8, 22, 'FOR APP', 'S12', DIM, 'l', { when: NO });
    b.T(232, 8, '{dow}', 'S12', DIM, 'r', { upper: true });

    // readouts
    b.R(6, 188, 228, 1, GRING, { when: NO });
    b.R(6, 188, 228, 1, RING, { when: COOL });
    b.R(6, 188, 228, 1, REDRING, { when: 'hot' });
    b.T(10, 190, '5H %', 'S12', DIM);
    b.T(10, 202, '{s}%', 'M32', BRIGHT, 'l', { when: COOL });
    b.T(10, 202, '{s}%', 'M32', RED, 'l', { when: 'hot' });
    b.T(10, 202, '--%', 'M32', CGREY, 'l', { when: NO });
    b.T(100, 194, 'WK {w}%', 'M18', BRIGHT, 'l', { when: 'ok !whot' });
    b.T(100, 194, 'WK {w}%', 'M18', RED, 'l', { when: 'whot' });
    b.T(100, 194, 'WK --%', 'M18', CGREY, 'l', { when: NO });
    b.T(230, 194, '{clock}', 'M18', WHITE, 'r');
    b.T(100, 218, '{wr@d}', 'S12', DIM, 'l', { when: OK });
    b.T(100, 218, '--', 'S12', DIM, 'l', { when: NO });
    b.T(230, 218, 'OUT {out}', 'S12', RED, 'r', { when: 'hot' });
    b.T(230, 218, 'RST {sr@}', 'S12', DIM, 'r', { when: COOL });
    b.T(230, 218, 'RST --:--', 'S12', DIM, 'r', { when: NO });
  },
});
