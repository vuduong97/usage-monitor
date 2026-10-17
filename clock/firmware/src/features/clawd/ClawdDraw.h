// ClawdDraw.h — the drawing layer for the Clawd screens on the clock.
//
// The ESP8266 has no room for a 240x240 framebuffer (115 KB), and filling a
// background and then printing over it flickers. Instead every widget is
// composed off-screen one 4-row band at a time — background, anti-aliased
// rounded shapes, then 4 bpp anti-aliased text blended over whatever is below
// — and each band is pushed to the panel in a single write. A full panel
// repaint therefore never shows an intermediate state.
//
// The pixel sink is a tiny interface so the same code renders on the device
// (Arduino_GFX) and on a desktop (tools/clawd_preview, PNG screenshots).
#pragma once
#include <stdint.h>
#include "ClawdAssets.h"

#define CLAWD_W 240
#define CLAWD_H 240
#define CLAWD_BAND_ROWS 4          // rows composed per push (1,920 B buffer)
#define CLAWD_SCRATCH_PX (CLAWD_W * CLAWD_BAND_ROWS)

// RGB565 design tokens — the original firmware's theme.h, converted.
#define CLAWD_BG      0x0000   // 0x000000
#define CLAWD_PANEL   0x18E3   // 0x1f1f1e card fill
#define CLAWD_TEXT    0xFFDE   // 0xfaf9f5
#define CLAWD_DIM     0xB574   // 0xb0aea5
#define CLAWD_ACCENT  0xDBAA   // 0xd97757 terra-cotta
#define CLAWD_GREEN   0x7C6B   // 0x788c5d
#define CLAWD_AMBER   0xDBAA   // 0xd97757
#define CLAWD_RED     0xC1C5   // 0xc0392b
#define CLAWD_BAR_BG  0x2945   // 0x2a2a28 unfilled bar track

class ClawdSurface {
 public:
  virtual ~ClawdSurface() {}
  virtual void fill(int x, int y, int w, int h, uint16_t color) = 0;
  virtual void push(int x, int y, int w, int h, const uint16_t* pixels) = 0;
  // One row of pixels; the desk player sends hundreds a frame, so the device
  // version skips push()'s per-call yield (the player yields once a frame).
  // `pixels` is scratch the surface may modify.
  virtual void span(int x, int y, int w, uint16_t* pixels) { push(x, y, w, 1, pixels); }
  // Device colour correction (the Display tab's per-channel gain); identity on
  // the desktop. Applied to every colour on its way to the panel.
  virtual uint16_t tint(uint16_t c) { return c; }
};

void          clawdSetSurface(ClawdSurface* s);
ClawdSurface* clawdSurface();
uint16_t*     clawdScratch();      // CLAWD_SCRATCH_PX pixels, shared by all renderers

struct ClawdShape {                // a filled rounded rectangle
  int16_t  x, y, w, h;
  int16_t  r;                      // corner radius (clamped to min(w,h)/2)
  uint16_t color;
  int16_t  clipRight = 0;          // if > 0: nothing at or right of this x
};

// An anti-aliased arc stroke with round caps, or a filled disc (r == 0, diameter
// lw). Angles are degrees clockwise from 12 o'clock. sweepDeg >= 360 is a full
// ring, <= 0 draws nothing. Drawn after the shapes, in list order, so a gauge sits
// on its card and a marker dot on its ring.
struct ClawdArc {
  int16_t  cx, cy;                 // centre
  int16_t  r;                      // radius to the middle of the stroke
  uint8_t  lw;                     // stroke width
  int16_t  startDeg, sweepDeg;
  uint16_t color;
};

struct ClawdText {                 // one run of text, pen at (x, y = line top)
  const ClawdFont* font;
  const char*      s;              // UTF-8
  int16_t          x, y;
  uint16_t         color;
};

// Compose and push region (x, y, w, h): `bg`, then shapes in order, then text.
// Shape and text coordinates are absolute screen pixels; anything outside the
// region is clipped.
void clawdRender(int x, int y, int w, int h, uint16_t bg,
                 const ClawdShape* shapes, int shapeCount,
                 const ClawdText* texts, int textCount);

// clawdRender plus arcs (layer order: background, shapes, arcs, text).
void clawdRenderArcs(int x, int y, int w, int h, uint16_t bg,
                     const ClawdShape* shapes, int shapeCount,
                     const ClawdArc* arcs, int arcCount,
                     const ClawdText* texts, int textCount);

float clawdSin(int deg);                 // whole degrees, table based (no libm trig)
float clawdCos(int deg);

int  clawdTextWidth(const ClawdFont& f, const char* s);   // px, kerning included

// Building blocks for renderers that compose their own bands into clawdScratch()
// (StylePack): blend one text run into band rows [by0, by1) of a region whose
// columns are [rx, rx + rw) (row stride rw), and a rounded rect's AA coverage.
void    clawdBlendText(const ClawdText& t, int rx, int rw, int by0, int by1);
uint8_t clawdShapeCoverage(const ClawdShape& s, int px, int py);   // 0..255

// 4 bpp cells -> panel. Renders the grid window [gx0, gx0+gw) x [gy0, gy0+gh)
// with `cell` px per cell at screen (x0, y0). `cellAt` returns a palette index.
// With `prev` (gw*gh nibbles), only grid rows that changed are pushed and
// `prev` is updated; `force` repaints everything.
typedef uint8_t (*ClawdCellFn)(void* ctx, int gx, int gy);
void clawdRenderCells(ClawdCellFn cellAt, void* ctx, int gx0, int gy0, int gw, int gh,
                      int cell, int x0, int y0, const uint16_t* palette,
                      uint8_t* prev, bool force);

uint16_t clawdBlend(uint16_t fg, uint16_t bg, uint8_t alpha);   // alpha 0..255
