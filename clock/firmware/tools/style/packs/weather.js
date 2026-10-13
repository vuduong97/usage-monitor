// #37 Weather station: a forecast icon beside the 5-hour hero number, a barometer
// whose needle is the week, and Clawd with sunglasses or an umbrella (Batch D,
// archetype 4: dial with 12 gated needle positions).
const { mock } = require('../mock');
const { SPD, tracks, clawd } = require('./_d2_kit');

const BG = '#0f2340', LINE = '#2c4a72', WHITE = '#f4f6fa', SKY = '#8ec5ff', SUN = '#ffd23f', CLOUD = '#dde6f0';
const STORM = '#6b7c93', RED = '#ff5a45', RAIN = '#4fa3ff', CLAWD = '#d97757', BLACK = '#0a0a0a';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot', FRESH = 'ok fresh';
const BX = 60, BY = 224;

const polar = (cx, cy, r, deg) => {
  const a = deg * Math.PI / 180;
  return [Math.round(cx + r * Math.sin(a)), Math.round(cy - r * Math.cos(a))];
};

module.exports = mock({
  id: 'weather', name: 'Weather station', bg: BG,
  draw(b) {
    const S = b.S;
    const T = tracks(b);
    // the hero is the sheet's Newsreader 56; Tiempos 56 is the nearest face (not in the mock's font map)
    const HERO_DY = -4;
    const hero = (str, c, o) => S.text(str, 150, 30 + HERO_DY, 't56', c, { align: 'right', ...o });

    b.T(12, 13, '5H SESSION', 'S12', SKY);
    b.T(166, 13, '{dow}', 'S12', SKY, 'r', { upper: true });
    b.T(228, 8, '{clock}', 'S20', WHITE, 'r');
    hero('{s}', WHITE, { when: COOL });
    hero('{s}', RED, { when: 'hot' });
    hero('--', STORM, { when: NO });
    b.T(153, 42, '%', 'S24', SKY);

    // forecast icon: sun and cloud, a clear sky when fresh, a storm cloud with rain and a bolt when hot
    b.Sp(SPD.sun, 188, 38, 3, SUN, { when: COOL });
    b.Sp(SPD.cloud, 176, 58, 3, CLOUD, { when: 'ok !hot !fresh' });
    // the rain: the sheet's 45 deg stripes (2 px on, 6 off, measured along the gradient)
    const RW = 34, RH = 22, L = (RW + RH) * Math.SQRT1_2;
    S.defMask('rain', RW, RH, (x, y) => {
      const pos = ((x + 0.5 - RW / 2) - (y + 0.5 - RH / 2)) * Math.SQRT1_2 + L / 2;
      return ((pos % 6) + 6) % 6 < 2;
    });
    S.mask('rain', 182, 66, 1, RAIN, { when: 'hot' });
    b.Sp(SPD.cloud, 180, 40, 3, STORM, { when: 'hot' });
    b.Sp(SPD.cloud, 180, 48, 3, STORM, { when: NO });
    b.Sp(SPD.bolt, 194, 64, 2, SUN, { when: 'hot', track: b.blink(500) });
    b.Sp(SPD.spark, 222, 34, 2, WHITE, { when: FRESH, track: b.blink(500) });

    b.T(14, 96, 'WAITING FOR APP', 'S12', SKY, 'l', { when: NO });
    b.T(14, 96, 'STORM WARNING: OUT {out}', 'S12', RED, 'l', { when: 'hot' });
    b.T(14, 96, 'RESETS {sr@}  IN {sr~}', 'S12', SKY, 'l', { when: COOL });
    b.R(12, 116, 216, 1, LINE);

    // barometer: the face runs off the bottom edge, so only its visible 62 rows are stored
    const R = 46, FH = 240 - (BY - R);
    S.defMask('face', 2 * R, FH, (x, y) => (x + 0.5 - R) ** 2 + (y + 0.5 - R) ** 2 <= R * R);
    S.mask('face', BX - R, BY - R, 1, LINE);
    for (let i = 0; i <= 6; i++) {
      const [x0, y0] = polar(BX, BY, 37, -90 + 30 * i), [x1, y1] = polar(BX, BY, 44, -90 + 30 * i);
      S.line(x0, y0, x1, y1, SKY);
    }
    for (let k = 0; k < 12; k++) {
      const a = -90 + 180 * (k + 0.5) / 12;
      const pts = [polar(BX, BY, 4, a - 90), polar(BX, BY, 4, a + 90), polar(BX, BY, 40, a)];
      const gate = k === 11 ? ['w', k / 12] : ['w', k / 12, (k + 1) / 12];
      S.tri(pts, WHITE, { when: 'ok !whot', gate });
      S.tri(pts, RED, { when: 'whot', gate });
    }
    b.C(BX, BY, 4, WHITE);
    b.T(10, 162, 'FAIR', 'S12', SKY);
    b.T(112, 162, 'STORM', 'S12', SKY, 'r');
    b.T(116, 124, 'WEEK', 'S12', SKY);
    b.T(116, 138, '{w}%', 'S28', WHITE, 'l', { when: 'ok !whot' });
    b.T(116, 138, '{w}%', 'S28', RED, 'l', { when: 'whot' });
    b.T(116, 138, '--%', 'S28', STORM, 'l', { when: NO });
    b.T(116, 172, 'IN {wr~}', 'S12', SKY, 'l', { when: OK });
    b.T(116, 172, 'IN --', 'S12', SKY, 'l', { when: NO });

    clawd(b, 170, 154, 2, CLAWD, STORM, BG, 'ok !hot !fresh', ['hot', FRESH]);
    b.Sp(SPD.glasses, 170, 178, 2, BLACK, { when: FRESH });
    b.Sp(SPD.umbrella, 186, 134, 2, RED, { when: 'hot' });
    b.Sp(SPD.zz, 204, 132, 2, SKY, { when: NO });
  },
});
