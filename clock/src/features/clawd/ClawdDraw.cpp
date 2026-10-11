#include "ClawdDraw.h"
#include <math.h>

static ClawdSurface* g_surface = nullptr;
static uint16_t      g_scratch[CLAWD_SCRATCH_PX];

void          clawdSetSurface(ClawdSurface* s) { g_surface = s; }
ClawdSurface* clawdSurface() { return g_surface; }
uint16_t*     clawdScratch() { return g_scratch; }

uint16_t clawdBlend(uint16_t fg, uint16_t bg, uint8_t a) {
  if (a == 0) return bg;
  if (a == 255) return fg;
  uint32_t r = (((fg >> 11) & 0x1F) * a + ((bg >> 11) & 0x1F) * (255 - a) + 127) / 255;
  uint32_t g = (((fg >> 5) & 0x3F) * a + ((bg >> 5) & 0x3F) * (255 - a) + 127) / 255;
  uint32_t b = ((fg & 0x1F) * a + (bg & 0x1F) * (255 - a) + 127) / 255;
  return (uint16_t)((r << 11) | (g << 5) | b);
}

// ---- text -------------------------------------------------------------------
static uint32_t nextCodepoint(const char*& p) {
  uint8_t c = (uint8_t)*p++;
  if (c < 0x80) return c;
  int extra = (c >= 0xF0) ? 3 : (c >= 0xE0) ? 2 : (c >= 0xC0) ? 1 : 0;
  uint32_t cp = c & (0x3F >> extra);
  for (int i = 0; i < extra && (*p & 0xC0) == 0x80; i++) cp = (cp << 6) | (*p++ & 0x3F);
  return cp;
}

// Glyph index for `cp`, or -1. Glyphs are sorted by code point.
static int findGlyph(const ClawdFont& f, uint32_t cp, ClawdGlyph& g) {
  int lo = 0, hi = f.glyphCount - 1;
  while (lo <= hi) {
    int mid = (lo + hi) / 2;
    memcpy_P(&g, &f.glyphs[mid], sizeof(g));
    if (g.cp == cp) return mid;
    if (g.cp < cp) lo = mid + 1; else hi = mid - 1;
  }
  return -1;
}

static int kern16(const ClawdFont& f, int left, int right) {
  if (!f.kerns || left < 0) return 0;
  int lo = 0, hi = f.kernCount - 1;
  while (lo <= hi) {
    int mid = (lo + hi) / 2;
    ClawdKern k;
    memcpy_P(&k, &f.kerns[mid], sizeof(k));
    int cmp = (k.left != left) ? (k.left - left) : (k.right - right);
    if (cmp == 0) return k.adv;
    if (cmp < 0) lo = mid + 1; else hi = mid - 1;
  }
  return 0;
}

// Walk a string, calling fn(glyph, glyphIndex, penX) for each drawable glyph.
// Missing glyphs fall back to '?', then to nothing.
// Advances follow LVGL's text layout exactly: each glyph advances by its own
// width plus the kerning against the NEXT glyph, rounded to whole pixels per
// glyph (lv_font_get_glyph_dsc_fmt_txt), so strings measure the same.
template <typename Fn>
static int walkText(const ClawdFont& f, const char* s, Fn fn) {
  int pen = 0, prev = -1, prevAdv = 0;
  ClawdGlyph g;
  for (const char* p = s; *p;) {
    uint32_t cp = nextCodepoint(p);
    int idx = findGlyph(f, cp, g);
    if (idx < 0) idx = findGlyph(f, '?', g);
    if (idx < 0) continue;
    if (prev >= 0) pen += (prevAdv + kern16(f, prev, idx) + 8) >> 4;
    fn(g, pen);
    prev = idx;
    prevAdv = g.adv;
  }
  if (prev >= 0) pen += (prevAdv + 8) >> 4;
  return pen;
}

int clawdTextWidth(const ClawdFont& f, const char* s) {
  return walkText(f, s, [](const ClawdGlyph&, int) {});
}

// Blend the part of one text run that falls into the band rows [by0, by1).
static void blendText(const ClawdText& t, int rx, int rw, int by0, int by1) {
  const ClawdFont& f = *t.font;
  const int top = t.y;
  const int base = top + f.lineHeight - f.baseLine;   // baseline y
  if (top >= by1 || top + f.lineHeight + 8 <= by0) return;
  uint16_t* buf = g_scratch;
  walkText(f, t.s, [&](const ClawdGlyph& g, int pen) {
    if (!g.w || !g.h) return;
    const int gx = t.x + pen + g.ox;
    const int gy = base - g.h - g.oy;
    if (gy >= by1 || gy + g.h <= by0 || gx >= rx + rw || gx + g.w <= rx) return;
    const uint8_t* bmp = f.bitmap + g.off;
    for (int row = 0; row < g.h; row++) {
      const int py = gy + row;
      if (py < by0 || py >= by1) continue;
      uint16_t* line = buf + (py - by0) * rw;
      uint32_t bit = (uint32_t)row * g.w;
      for (int col = 0; col < g.w; col++, bit++) {
        const int px = gx + col;
        if (px < rx || px >= rx + rw) continue;
        uint8_t b = pgm_read_byte(bmp + (bit >> 1));
        uint8_t a4 = (bit & 1) ? (b & 0x0F) : (b >> 4);
        if (!a4) continue;
        uint16_t& dst = line[px - rx];
        dst = clawdBlend(t.color, dst, (uint8_t)(a4 * 17));
      }
    }
  });
}

// ---- shapes -----------------------------------------------------------------
// Coverage (0..255) of pixel (px, py) by a rounded rect, anti-aliased corners.
static uint8_t shapeCoverage(const ClawdShape& s, int px, int py) {
  if (px < s.x || py < s.y || px >= s.x + s.w || py >= s.y + s.h) return 0;
  if (s.clipRight > 0 && px >= s.clipRight) return 0;
  int r = s.r;
  int lim = (s.w < s.h ? s.w : s.h) / 2;
  if (r > lim) r = lim;
  if (r <= 0) return 255;
  float cx, cy;
  if (px < s.x + r) cx = s.x + r;
  else if (px >= s.x + s.w - r) cx = s.x + s.w - r;
  else return 255;
  if (py < s.y + r) cy = s.y + r;
  else if (py >= s.y + s.h - r) cy = s.y + s.h - r;
  else return 255;
  float dx = (px + 0.5f) - cx, dy = (py + 0.5f) - cy;
  float cov = r - sqrtf(dx * dx + dy * dy) + 0.5f;
  if (cov <= 0) return 0;
  if (cov >= 1) return 255;
  return (uint8_t)(cov * 255.0f);
}

void clawdRender(int x, int y, int w, int h, uint16_t bg,
                 const ClawdShape* shapes, int shapeCount,
                 const ClawdText* texts, int textCount) {
  if (!g_surface || w <= 0 || h <= 0) return;
  if (w > CLAWD_W) w = CLAWD_W;
  const int rowsPerBand = CLAWD_SCRATCH_PX / w;
  for (int by = y; by < y + h; by += rowsPerBand) {
    const int rows = (by + rowsPerBand <= y + h) ? rowsPerBand : (y + h - by);
    for (int r = 0; r < rows; r++) {
      uint16_t* line = g_scratch + r * w;
      const int py = by + r;
      for (int c = 0; c < w; c++) {
        uint16_t px = bg;
        for (int k = 0; k < shapeCount; k++) {
          uint8_t a = shapeCoverage(shapes[k], x + c, py);
          if (a) px = clawdBlend(shapes[k].color, px, a);
        }
        line[c] = px;
      }
    }
    for (int k = 0; k < textCount; k++)
      if (texts[k].s && texts[k].s[0]) blendText(texts[k], x, w, by, by + rows);
    g_surface->push(x, by, w, rows, g_scratch);
  }
}

// ---- pixel-art cells ------------------------------------------------------------
void clawdRenderCells(ClawdCellFn cellAt, void* ctx, int gx0, int gy0, int gw, int gh,
                      int cell, int x0, int y0, const uint16_t* palette,
                      uint8_t* prev, bool force) {
  if (!g_surface) return;
  const int bandW = gw * cell;
  if (bandW * cell > CLAWD_SCRATCH_PX) return;   // never happens for the SmallTV layouts
  uint8_t rowCells[64];
  for (int gy = 0; gy < gh; gy++) {
    bool changed = force || !prev;
    for (int gx = 0; gx < gw && gx < 64; gx++) {
      uint8_t v = cellAt(ctx, gx0 + gx, gy0 + gy) & 0x0F;
      rowCells[gx] = v;
      if (prev && !changed) {
        uint32_t i = (uint32_t)gy * gw + gx;
        uint8_t old = (i & 1) ? (prev[i >> 1] & 0x0F) : (prev[i >> 1] >> 4);
        if (old != v) changed = true;
      }
    }
    if (!changed) continue;
    if (prev) {
      for (int gx = 0; gx < gw; gx++) {
        uint32_t i = (uint32_t)gy * gw + gx;
        uint8_t& b = prev[i >> 1];
        b = (i & 1) ? (uint8_t)((b & 0xF0) | rowCells[gx]) : (uint8_t)((b & 0x0F) | (rowCells[gx] << 4));
      }
    }
    uint16_t* line = g_scratch;
    for (int gx = 0; gx < gw; gx++) {
      uint16_t c = palette[rowCells[gx]];
      for (int k = 0; k < cell; k++) line[gx * cell + k] = c;
    }
    for (int r = 1; r < cell; r++) memcpy(g_scratch + r * bandW, line, bandW * sizeof(uint16_t));
    g_surface->push(x0, y0 + gy * cell, bandW, cell, g_scratch);
  }
}
