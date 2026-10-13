// Helpers shared by the Batch D packs ported by worker d1 (vumeter, scope, pfd, fuel,
// monitor): the sheet's own sprites (they differ a little from Batch A's), its polar
// geometry helpers and the Clawd layers with the sheet's eye-lid animation.
'use strict';
const { msk } = require('../mock');

const SPR = {
  drop: msk('..#.. ..#.. .###. .###. ##### ##### .###.'),
  zz: msk('#####.... ...#..... ..#...... .#....... #####.... .....#### .......#. ......#.. .....####'),
  spark: msk('..#.. ..#.. ##.## ..#.. ..#..'),
  pump: msk('####... #..#.#. #..#..# ####..# ####..# ####.## ####... ######.'),
  heart: msk('.##.##. ####### ####### .#####. ..###.. ...#...'),
};

const OK = 'ok', NO = '!ok', NORMAL = 'ok !hot !fresh', FRESH = 'ok fresh', COOL = 'ok !hot';

// the sheet's polar(): 0 deg is up, clockwise, rounded to whole pixels
const polar = (cx, cy, r, deg) => {
  const a = deg * Math.PI / 180;
  return [Math.round(cx + r * Math.sin(a)), Math.round(cy - r * Math.cos(a))];
};
// a quad between two radii and two angles, as its two triangles
const quadTris = (cx, cy, r1, r2, a1, a2) => {
  const p = [polar(cx, cy, r1, a1), polar(cx, cy, r2, a1), polar(cx, cy, r2, a2), polar(cx, cy, r1, a2)];
  return [[p[0], p[1], p[2]], [p[0], p[2], p[3]]];
};
// gate for "bucket(value, n) === k" on a 0..100 percentage (the last bucket takes 100 %)
const bucketGate = (src, k, n) => [src, k / n, k === n - 1 ? undefined : (k + 1) / n];

// the sheet's CSS animations: sweat drop (translate 0 -> 12 px in 3 steps / 0.9 s)
const drip = (b) => b.S.track(300, [{ dy: 0 }, { dy: 4 }, { dy: 8 }]);

// Clawd with the sheet's eye holes and lid. body = Clawd colour, grey = no-data colour,
// eye = the hole colour. With `outline`, the sheet draws an outline layer around the body.
// The body mask leaves the eyes transparent, so the eyes are always drawn in `eye`.
// Hot: eyes open, never blink. No data: the lid stays shut (eyes filled grey). Else: the
// lid shuts for 150 ms / 3 s.
function clawd(b, x, y, s, body, grey, eye, outline) {
  for (const [when, c] of [[OK, body], [NO, grey]]) {
    if (outline) b.clawd(x, y, s, { edge: outline, inner: c, when });
    else b.clawd(x, y, s, { body: c, when });
  }
  b.eyes(x, y, s, eye, { when: 'hot' });
  b.eyes(x, y, s, eye, { when: COOL, track: b.eyeBlink(body) });
  b.eyes(x, y, s, grey, { when: NO });
}

module.exports = { SPR, OK, NO, NORMAL, FRESH, COOL, polar, quadTris, bucketGate, drip, clawd };
