// #36 Server rack panel: four rack units on perforated rails, status-LED bars for the
// two windows, an LCD clock and Clawd peeking out of the vent unit (Batch D,
// archetype 8: card stack of rack units).
const { mock } = require('../mock');
const { SPD, tracks, clawd } = require('./_d2_kit');

const BG = '#0e0f11', RAIL = '#3a3e45', FACE = '#2a2d33', SCREW = '#5c626b', UNLIT = '#181a1e', WHITE = '#edeff2';
const GREY = '#8e959f', GREEN = '#44d46a', AMBER = '#f2b230', RED = '#ee4036', CLAWD = '#d97757';
const OK = 'ok', NO = '!ok', COOL = 'ok !hot', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot';

module.exports = mock({
  id: 'rack', name: 'Server rack', bg: BG,
  draw(b) {
    const T = tracks(b);
    b.R(0, 0, 8, 240, RAIL);
    b.R(2, 0, 4, 240, BG, { p: ['h', 12, 6] });
    b.R(232, 0, 8, 240, RAIL);
    b.R(234, 0, 4, 240, BG, { p: ['h', 12, 6] });
    const unit = (y, h) => {
      b.R(10, y, 220, h, FACE, { r: 2 });
      b.C(15, y + Math.round(h / 2), 2, SCREW);
      b.C(225, y + Math.round(h / 2), 2, SCREW);
    };

    // U1: 5-hour session
    unit(6, 92);
    b.T(20, 12, '5H SESSION', 'S12', GREY);
    b.C(216, 18, 3, UNLIT, { when: NO });
    b.C(216, 18, 3, GREEN, { when: FRESH });
    b.C(216, 18, 3, GREEN, { when: NORMAL, track: b.blink(500) });
    b.C(216, 18, 3, RED, { when: 'hot', track: b.blink(200) });
    b.T(18, 32, '{s}%', 'S48', WHITE, 'l', { when: COOL });
    b.T(18, 32, '{s}%', 'S48', RED, 'l', { when: 'hot' });
    b.T(18, 32, '--%', 'S48', GREY, 'l', { when: NO });
    b.T(122, 24, 'LOAD', 'S12', GREY);
    b.R(122, 40, 98, 7, UNLIT, { p: ['v', 10, 7] });
    b.R(122, 52, 98, 7, UNLIT, { p: ['v', 10, 7] });
    for (let k = 0; k < 20; k++) {
      b.R(122 + 10 * (k % 10), 40 + 12 * Math.floor(k / 10), 7, 7, k < 12 ? GREEN : k < 16 ? AMBER : RED,
        { when: OK, gate: ['s', (k + 0.5) * 5 / 100] });
    }
    b.T(122, 72, 'OUT AT {out}', 'S12', RED, 'l', { when: 'hot' });
    b.T(122, 72, 'RST {sr@}', 'S12', GREY, 'l', { when: COOL });
    b.T(122, 72, 'RST --:--', 'S12', GREY, 'l', { when: NO });
    b.T(20, 82, 'IN {sr~}', 'S12', GREY, 'l', { when: OK });
    b.T(20, 82, 'IN --', 'S12', GREY, 'l', { when: NO });

    // U2: week
    unit(102, 46);
    b.T(20, 108, 'WEEK', 'S12', GREY);
    b.T(20, 124, '{w}%', 'S20', WHITE, 'l', { when: 'ok !whot' });
    b.T(20, 124, '{w}%', 'S20', RED, 'l', { when: 'whot' });
    b.T(20, 124, '--%', 'S20', GREY, 'l', { when: NO });
    b.T(80, 108, 'RST {wr@d}', 'S12', GREY, 'l', { when: OK });
    b.T(80, 108, 'RST --', 'S12', GREY, 'l', { when: NO });
    b.R(80, 128, 138, 10, UNLIT, { p: ['v', 7, 5] });
    for (let k = 0; k < 20; k++) {
      const g = { gate: ['w', (k + 0.5) * 5 / 100] };
      if (k < 16) b.R(80 + 7 * k, 128, 5, 10, GREEN, { when: OK, ...g });
      else {
        b.R(80 + 7 * k, 128, 5, 10, AMBER, { when: 'ok !whot', ...g });
        b.R(80 + 7 * k, 128, 5, 10, RED, { when: 'whot', ...g });
      }
    }

    // U3: LCD clock, forecast line, ports
    unit(152, 40);
    b.R(20, 158, 84, 28, BG, { r: 2 });
    b.T(62, 163, '{clock}', 'M18', GREEN, 'c');
    b.R(176, 174, 46, 10, BG, { p: ['v', 12, 9] });
    b.R(177, 170, 44, 2, UNLIT, { p: ['v', 12, 3], when: NO });
    b.R(177, 170, 44, 2, GREEN, { p: ['v', 12, 3], when: OK });
    // the words go on top of the ports: the clock's S12 runs a pixel or two wider than the sheet's
    b.T(112, 159, 'WAITING FOR APP', 'S12', GREY, 'l', { when: NO });
    b.T(112, 159, 'PACE OVER', 'S12', RED, 'l', { when: 'hot' });
    b.T(112, 159, 'NEW WINDOW', 'S12', GREEN, 'l', { when: FRESH });
    b.T(112, 159, 'END ~{end}%', 'S12', WHITE, 'l', { when: NORMAL });
    b.T(112, 175, 'LINK DOWN', 'S12', AMBER, 'l', { when: NO });
    b.T(112, 175, '{dow}', 'S12', GREY, 'l', { when: OK, upper: true });

    // U4: vents, with Clawd between them
    unit(196, 38);
    b.R(22, 202, 74, 26, BG, { p: ['v', 4, 2] });
    b.R(146, 202, 74, 26, BG, { p: ['v', 4, 2] });
    clawd(b, 104, 194, 1, CLAWD, SCREW, FACE, COOL, ['hot']);
    b.Sp(SPD.drop, 138, 194, 1, WHITE, { when: 'hot', track: T.drip });
    b.Sp(SPD.spark, 138, 196, 1, GREEN, { when: FRESH });
    b.Sp(SPD.zz, 138, 194, 1, GREY, { when: NO });
  },
});
