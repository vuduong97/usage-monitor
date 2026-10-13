// Helpers shared by the Batch C packs invaders, eggpet, platformer, chessclock and racinghud
// (build time only; nothing here reaches the pack file).
'use strict';

// Rows ('#' on) of a W x H pixel sprite drawn from SVG-style rects [x, y, w, h] in a vw x vh
// viewBox, sampled at pixel centres (what crispEdges does for a non-integer scale).
function rectRows(rects, vw, vh, W = vw, H = vh) {
  const rows = [];
  for (let py = 0; py < H; py++) {
    let row = '';
    for (let px = 0; px < W; px++) {
      const x = ((px + 0.5) * vw) / W, y = ((py + 0.5) * vh) / H;
      row += rects.some(([rx, ry, rw, rh]) => x >= rx && x < rx + rw && y >= ry && y < ry + rh) ? '#' : '.';
    }
    rows.push(row);
  }
  return rows;
}

// The Tamagotchi egg: a stadium on top (centre (120, 92), radius rTop, straight 14 px) merged into a
// disc (centre (120, 148), radius rBot) that runs off the bottom of the screen, as a polygon filled by a
// triangle fan from (120, 120). Returns the triangles (three [x, y] points each). `grow` widens
// both radii (the rim is the shell grown by 2 px). A large rounded rect would cost a square root per
// pixel on the clock; a fan costs one scanline span per row.
function eggFan(grow, stepDeg = 12) {
  const rTop = 84 + grow, rBot = 100 + grow, cx = 120, cyTop = 92, cyBot = 148, bottom = 240;
  const step = (stepDeg * Math.PI) / 180;
  const right = [];                                   // the right half, from the top down to the screen edge
  for (let a = 0; a < Math.PI / 2 - 1e-9; a += step) right.push([cx + rTop * Math.sin(a), cyTop - rTop * Math.cos(a)]);
  right.push([cx + rTop, cyTop]);                     // the stadium's straight side ...
  right.push([cx + rTop, cyBot - Math.sqrt(rBot * rBot - rTop * rTop)]);   // ... until the disc is as wide
  const aStart = Math.asin(rTop / rBot), aEnd = Math.PI - Math.acos((bottom - cyBot) / rBot);   // angles from straight up
  for (let a = aStart + step; a < aEnd - 1e-9; a += step) right.push([cx + rBot * Math.sin(a), cyBot - rBot * Math.cos(a)]);
  right.push([cx + rBot * Math.sin(aEnd), bottom]);
  const ring = [...right, ...right.slice().reverse().map(([x, y]) => [2 * cx - x, y])];
  return ring.map((p, i) => [[cx, 120], p, ring[(i + 1) % ring.length]]);
}

// A filled polygon (points [x, y]) as sprite rows ('#' on, pixel centres inside) over its bounding box.
function polyRows(pts) {
  const x0 = Math.floor(Math.min(...pts.map((p) => p[0]))), x1 = Math.ceil(Math.max(...pts.map((p) => p[0])));
  const y0 = Math.floor(Math.min(...pts.map((p) => p[1]))), y1 = Math.ceil(Math.max(...pts.map((p) => p[1])));
  const inside = (x, y) => {
    let c = false;
    for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
      const [xi, yi] = pts[i], [xj, yj] = pts[j];
      if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) c = !c;
    }
    return c;
  };
  const rows = [];
  for (let y = y0; y < y1; y++) {
    let row = '';
    for (let x = x0; x < x1; x++) row += inside(x + 0.5, y + 0.5) ? '#' : '.';
    rows.push(row);
  }
  return { x: x0, y: y0, rows };
}

module.exports = { rectRows, eggFan, polyRows };
