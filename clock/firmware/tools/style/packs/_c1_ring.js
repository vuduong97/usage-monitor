// _c1_ring.js: a 1 px rounded outline for the Batch C packs, as rects (the straight edges) and one
// small mask per corner. It draws it the way a browser draws border-radius: the pixels whose
// centres lie between the outer rounded rect and the same rect inset by 1 px.
'use strict';

const inside = (px, py, x, y, w, h, r) => {
  const cx = px + 0.5, cy = py + 0.5;
  if (cx < x || cx > x + w || cy < y || cy > y + h) return false;
  const dx = Math.max(x + r - cx, cx - (x + w - r), 0), dy = Math.max(y + r - cy, cy - (y + h - r), 0);
  return dx * dx + dy * dy <= r * r;
};
const ring = (px, py, r) => inside(px, py, 0, 0, 40, 40, r) && !inside(px, py, 1, 1, 38, 38, r - 1);
const madeFor = new WeakMap();   // the Style being built -> corner masks already defined

function outline(b, x, y, w, h, r, color, when) {
  if (!madeFor.has(b.S)) madeFor.set(b.S, new Set());
  const made = madeFor.get(b.S);
  const o = { when };
  b.R(x + r, y, w - 2 * r, 1, color, o);
  b.R(x + r, y + h - 1, w - 2 * r, 1, color, o);
  if (h > 2 * r) { b.R(x, y + r, 1, h - 2 * r, color, o); b.R(x + w - 1, y + r, 1, h - 2 * r, color, o); }
  for (const [name, ox, oy, cx, cy] of [['a', 0, 0, x, y], ['b', 40 - r, 0, x + w - r, y], ['c', 0, 40 - r, x, y + h - r], ['d', 40 - r, 40 - r, x + w - r, y + h - r]]) {
    const key = `rc${r}${name}`;
    if (!made.has(key)) { made.add(key); b.S.defMask(key, r, r, (mx, my) => ring(ox + mx, oy + my, r)); }
    b.S.mask(key, cx, cy, 1, color, o);
  }
}


module.exports = { outline };
