// Shared bits of the Batch D ports (rack, weather, powerbank, lab, sonar): the sheet's
// sprites (they differ from Batch A's), its animations and its Clawd with the "lid" trick.
const { msk } = require('../mock');

// Batch D sheet sprites ("SPR" in the artboard kit).
const SPD = {
  drop: msk('..#.. ..#.. .###. .###. ##### ##### .###.'),
  zz: msk('#####.... ...#..... ..#...... .#....... #####.... .....#### .......#. ......#.. .....####'),
  spark: msk('..#.. ..#.. ##.## ..#.. ..#..'),
  glasses: msk('...##########################... ...##########......##########... ...##########......##########... ....########........########....'),
  bolt: msk('...## ..##. .##.. ##### ..##. .##.. ##... #....'),
  sun: msk('.....#..... .#...#...#. ..#.....#.. ....###.... ...#####... ##.#####.## ...#####... ....###.... ..#.....#.. .#...#...#. .....#.....'),
  cloud: msk('.....###..... ...#######... ..#########.. .###########. ############# ############# .###########.'),
  umbrella: msk('.....#####..... ...#########... .#############. ############### #.#.#.#.#.#.#.# .......#....... .......#....... .....###.......'),
};

// CSS ckDrip: translateY 0 -> 12 px in steps(3) over 0.9 s; ckRise: 0 -> -12 px over 1.2 s.
function tracks(b) {
  return {
    drip: b.S.track(300, [{ dy: 0 }, { dy: 4 }, { dy: 8 }]),
    rise: b.S.track(400, [{ dy: 0 }, { dy: -4 }, { dy: -8 }]),
  };
}

// Clawd (32x40 x sc) the way the sheet draws it: body colour `C` (grey `G` without data),
// eye holes in the face colour `F`, and the "lid": an eye-coloured-as-body overlay that
// shows for 150 ms every 3 s (blink) while `blinkWhen` holds. Without data the lid is
// always on (eyes shut, sleeping). `openWhens` lists the states with the eyes simply open.
// `o` (e.g. a dx binding) applies to the with-data layers only; the no-data Clawd stays put.
function clawd(b, x, y, sc, C, G, F, blinkWhen, openWhens, o = {}) {
  b.M('clawd', x, y, sc, C, { ...o, when: 'ok' });
  b.M('clawd', x, y, sc, G, { when: '!ok' });
  b.eyes(x, y, sc, F, { ...o, when: blinkWhen, track: b.eyeBlink(C) });
  for (const w of openWhens) b.eyes(x, y, sc, F, { ...o, when: w });
  b.eyes(x, y, sc, G, { when: '!ok' });
}

module.exports = { SPD, tracks, clawd };
