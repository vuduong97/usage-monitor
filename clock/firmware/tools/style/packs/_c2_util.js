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

module.exports = { rectRows };
