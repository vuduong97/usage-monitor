// mock.js — the style-pack mock vocabulary (R/O/C/P/T/S, "Clawd style packs - Batch A")
// on top of the Style builder, so a pack source reads like its design artboard.
//
// The mock draws each of its four states separately with plain JS conditions; a pack
// is one op list, so each mock `if` becomes a `when` gate (ok, hot, fresh, ...) or a
// data binding (gate/dx/x1 on s, w, ts, tw) and each data string becomes a token.
//
// Geometry is the mock's, with two conversions:
// - Text: the mock's y is the top of a line box as tall as the font size (CSS
//   line-height = size). The firmware's y is the top of the LVGL line box. Y_OFF is
//   the difference of the two baselines, measured per font (mock fonts in Chrome vs the
//   LVGL fonts' lineHeight - baseLine), so the glyphs sit on the mock's baseline.
// - Stripes: the mock's repeating gradients start at the element's own edge; the
//   firmware's patterns run on screen coordinates, so the phase is shifted to match.
'use strict';

const FONT = { M18: 'm18', M32: 'm32', S12: 's12', S14: 's14', S16: 's16', S20: 's20', S24: 's24', S28: 's28', S48: 's48' };
const Y_OFF = { M18: 0, M32: 1, S12: -1, S14: 1, S16: 0, S20: 0, S24: 0, S28: 0, S48: 0 };

// The sprites every Batch A artboard shares.
const msk = (t) => t.trim().split(/\s+/);
const SPR = {
  drop: msk('..#.. ..#.. .###. .###. ##### ##### .###.'),
  spark: msk('..#.. ..#.. ##### ..#.. ..#..'),
  z: msk('##### ...#. ..#.. .#... #####'),
};

const mod = (a, m) => ((a % m) + m) % m;

// Wrap a pack source: `draw(b)` gets the mock-style builder.
function mock(src) {
  return {
    id: src.id, name: src.name, bg: src.bg, light: !!src.light,
    draw(S) { src.draw(builder(S)); },
  };
}

function builder(S) {
  const masks = new Map();
  const tracks = new Map();
  const maskOf = (rows) => {
    const key = rows.join('/');
    if (!masks.has(key)) {
      const name = `m${masks.size}`;
      S.defMask(name, rows[0].length, rows.length, (x, y) => rows[y]?.[x] === '#');
      masks.set(key, name);
    }
    return masks.get(key);
  };
  const once = (key, make) => { if (!tracks.has(key)) tracks.set(key, make()); return tracks.get(key); };
  const opt = (o) => {
    const r = {};
    for (const k of ['when', 'gate', 'dx', 'x1', 'track', 'phase', 'zeroOk', 'minW', 'upper', 'clamp']) if (o[k] !== undefined) r[k] = o[k];
    return r;
  };

  const b = {
    S,
    // A rect; p = [dir, period, on] like the mock's stripes: 'h' rows, 'v' columns, 'd' diagonal.
    R(x, y, w, h, c, o = {}) {
      const a = opt(o);
      if (o.r) a.r = o.r;
      if (o.p) {
        const [d, per, on] = o.p;
        if (d === 'h') a.pattern = ['rows', per, mod(-y, per), on];
        else if (d === 'v') a.pattern = ['cols', per, mod(-x, per), on];
        else a.pattern = ['diag', o.dmod || 4, o.dphase || 0];   // the mock's 45deg 1-in-3 hatch, ~4 px apart
      }
      return S.rect(x, y, w, h, c, a);
    },
    // The mock's 45deg hatch ("\" lines, x - y = phase mod period) inside a rect, as
    // clipped line ops: the firmware's diag pattern only runs the other way ("/").
    hatch(x0, y0, w, h, c, period = 4, phase = 0, o = {}) {
      const x1 = x0 + w - 1, y1 = y0 + h - 1;
      for (let k = x0 - y1; k <= x1 - y0; k++) {
        if (mod(k - phase, period)) continue;
        const ya = Math.max(y0, x0 - k), yb = Math.min(y1, x1 - k);
        if (ya <= yb) S.line(ya + k, ya, yb + k, yb, c, opt(o));
      }
    },
    // A 1 px outline.
    O(x, y, w, h, c, o = {}) { S.strokeRect(x, y, w, h, c, opt(o)); },
    C(cx, cy, r, c, o = {}) { return S.disc(cx, cy, r, c, opt(o)); },
    P(pts, c, o = {}) { return S.tri(pts, c, opt(o)); },
    // Text: al 'l' | 'c' | 'r' at x, like the mock.
    T(x, y, tx, f, c, al = 'l', o = {}) {
      if (!(f in FONT)) throw new Error(`font ${f} not mapped`);
      const a = opt(o);
      a.align = { l: 'left', c: 'center', r: 'right' }[al];
      return S.text(String(tx), x, y + Y_OFF[f], FONT[f], c, a);
    },
    // A sprite given as mock rows ('#' on), scaled by s.
    Sp(rows, x, y, s, c, o = {}) { return S.mask(maskOf(rows), x, y, s, c, opt(o)); },
    // Clawd: o.body, or o.edge + o.inner (outlined), plus o.eyes. o.when gates all of it;
    // o.track moves the whole sprite, o.eyeTrack recolours the eyes.
    clawd(x, y, s, o) {
      const g = opt(o);
      if (o.edge) {
        S.mask('clawd_inner', x, y, s, o.inner, g);
        S.mask('clawd_edge', x, y, s, o.edge, g);
      } else {
        S.mask('clawd', x, y, s, o.body, g);
      }
      if (o.eyes) S.mask('clawd_eyes', x, y, s, o.eyes, { ...g, track: o.eyeTrack ?? g.track });
    },
    // Just Clawd's eyes (the holes), e.g. recoloured or blinking in one state only.
    eyes(x, y, s, c, o = {}) { return S.mask('clawd_eyes', x, y, s, c, opt(o)); },
    // One built-in Clawd layer: 'clawd', 'clawd_eyes', 'clawd_edge' or 'clawd_inner'.
    M(name, x, y, s, c, o = {}) { return S.mask(name, x, y, s, c, opt(o)); },
    // The mock's animations (CSS steps()), shared per pack.
    // Eyes blink: `on` for the last 150 ms of every 3 s.
    eyeBlink(on) { return once(`eye${on}`, () => S.track(150, [...Array(19).fill({}), { color: on }])); },
    blink(ms = 500) { return once(`blink${ms}`, () => S.track(ms, [{ vis: true }, { vis: false }])); },
    bob(dy, ms = 300) { return once(`bob${dy},${ms}`, () => S.track(ms, [{ dy: 0 }, { dy }])); },
  };
  return b;
}

module.exports = { mock, SPR, msk, FONT, Y_OFF };
