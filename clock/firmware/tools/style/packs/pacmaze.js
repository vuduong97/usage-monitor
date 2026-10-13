// #23 Pac-Man maze: Clawd runs along a corridor as the 5-hour gauge, eating a pellet for every
// 18 px still ahead of him; a ghost follows along the week (Batch C, archetype 6: Clawd is
// the gauge).
const { mock, msk } = require('../mock');
const { outline } = require('./_c1_ring');

const K = '#000000', BL = '#2121de', YE = '#ffff00', PK = '#ffb8ae', WH = '#ffffff', PI = '#ffb8ff', RD = '#ff0000',
  OR = '#d97757', GY = '#5a5a5a';
const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh !hot', COOL = 'ok !hot';
const DROP = msk('..#.. .###. ##### ##### .###.');
const GHOST = ['.....####.....', '...########...', '..##########..', '.############.', ...Array(9).fill('##############'), '##..##..##..##'];
const EYES = ['..####..####..', ...Array(3).fill('..####..####..')];
const PUPIL = ['....##....##..', '....##....##..'];

module.exports = mock({
  id: 'pacmaze', name: 'Pac-Man maze', bg: K,
  draw(b) {
    // the maze: a double wall and two bars, red when the session runs hot
    for (const [color, when] of [[BL, '!hot'], [RD, 'hot']]) {
      outline(b, 6, 6, 228, 228, 10, color, when);
      outline(b, 10, 10, 220, 220, 7, color, when);
      outline(b, 18, 80, 204, 8, 4, color, when);
      outline(b, 18, 164, 204, 8, 4, color, when);
    }
    b.T(18, 18, '5H', 'M18', WH);
    b.T(222, 18, '{clock}', 'M18', WH, 'r');
    b.T(18, 40, '{s}%', 'M32', YE, 'l', { when: 'ok !hot' });
    b.T(18, 40, '{s}%', 'M32', RD, 'l', { when: 'hot' });
    b.T(18, 40, '--', 'M32', GY, 'l', { when: NO });
    b.T(222, 50, 'END ~{end}%', 'M18', PK, 'r', { when: NORMAL });
    b.T(222, 50, 'OUT{out}', 'M18', RD, 'r', { when: 'hot' });
    b.T(222, 50, 'READY!', 'M18', YE, 'r', { when: FRESH });

    // pellet i is still there while Clawd (x = 14 + 176 * 5H) is short of it
    for (let i = 1; i < 10; i++) b.R(30 + 18 * i, 116, 4, 4, PK, { when: OK, gate: ['s', 0, (18 * i - 16.5) / 176] });
    b.C(210, 118, 4, PK, { when: OK, track: b.blink(300) });
    b.T(58, 112, 'WAITING FOR APP', 'S14', WH, 'l', { when: NO });

    // Clawd at (14 + 176 * 5H, 98): bobbing and blinking while calm, a sweat drop when hot
    const run = { dx: ['s', 176] };
    const bobBlink = b.S.track(50, Array.from({ length: 60 }, (_, k) => ({ dy: k % 10 < 5 ? 0 : -2, color: k === 57 || k === 58 || k === 59 ? OR : undefined })));
    b.clawd(14, 98, 1, { body: OR, when: COOL, track: b.bob(-2, 250), ...run });
    b.eyes(14, 98, 1, K, { when: COOL, track: bobBlink, ...run });
    b.clawd(14, 98, 1, { body: OR, when: 'hot', ...run });
    b.eyes(14, 98, 1, RD, { when: 'hot', ...run });
    b.Sp(DROP, 48, 92, 2, WH, { when: 'hot', track: b.S.track(200, [{ dy: 0 }, { dy: 2 }, { dy: 4 }]), ...run });
    b.clawd(14, 98, 1, { body: GY, eyes: GY, when: NO });
    b.R(18, 113, 4, 1, K, { when: NO });
    b.R(38, 113, 4, 1, K, { when: NO });
    b.T(50, 92, 'z Z', 'S12', GY, 'l', { when: NO });

    b.T(18, 145, 'RESET {sr@}  {sr~}', 'S14', WH, 'l', { when: OK });
    b.T(18, 145, 'RESET --', 'S14', GY, 'l', { when: NO });
    b.T(18, 182, 'WEEK', 'S12', WH);
    b.T(18, 198, '{w}%', 'M18', PI, 'l', { when: 'ok !whot !fresh' });
    b.T(18, 198, '{w}%', 'M18', RD, 'l', { when: 'whot' });
    b.T(18, 198, '{w}%', 'M18', WH, 'l', { when: 'ok fresh !whot' });
    b.T(18, 198, '--', 'M18', GY, 'l', { when: NO });

    // the ghost moves along the week: pink, red when the week is hot, frightened blue when fresh
    const ghost = { dx: ['w', 110] };
    b.Sp(GHOST, 84, 186, 2, PI, { when: 'ok !whot !fresh', ...ghost });
    b.Sp(GHOST, 84, 186, 2, RD, { when: 'whot', ...ghost });
    b.Sp(GHOST, 84, 186, 2, BL, { when: 'ok fresh !whot', ...ghost });
    b.Sp(EYES, 84, 194, 2, WH, { when: OK, ...ghost });
    b.Sp(EYES, 84, 194, 2, GY, { when: NO, ...ghost });
    b.Sp(PUPIL, 84, 196, 2, BL, ghost);
  },
});
