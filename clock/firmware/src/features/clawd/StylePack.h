// StylePack.h — the usage screen drawn from a style pack (docs/STYLE_PACK.md).
//
// A pack is a fixed list of primitives (rectangles, text with data tokens,
// lines, triangles, 1-bit masks) compiled by firmware/tools/style from a pack
// source. Each primitive can be gated on the usage state and moved/recoloured
// by an animation track, so the clock animates without a framebuffer: a full
// paint when what the screen says changes, otherwise only the rectangles an
// animation step touched, composed band by band like ClawdDraw.
//
// The pack is read in place through the flash cache (memcpy_P / pgm_read_*):
// StyleStore mirrors /style.pak into the free sketch flash, so fonts and ops
// cost no RAM. The host preview attaches a plain buffer instead.
#pragma once
#include <stdint.h>
#include "ClawdAssets.h"
#include "ClawdUi.h"

#define STYLE_API 1
#define STYLE_MAX_BYTES 32768        // largest pack accepted (the compiler caps packs at 20 KB)
#define STYLE_HEADER_BYTES 96
#define STYLE_MAX_ANIMATED 64        // ops with an animation track

struct StyleInfo {
  char     id[16];
  char     name[24];
  uint32_t size;
  uint32_t crc;
  uint8_t  api;
  bool     light;                    // light background (the app may warn about night use)
};

// Fonts the firmware links, by the id a pack uses for them (0 Styrene 14,
// 1 Styrene 24, 2 DejaVu Mono 18, 3 Tiempos 34). ClawdUi registers them.
void styleSetBuiltinFonts(const ClawdFont* const* fonts, uint8_t count);

// CRC-32 (zlib) of n bytes read through pgm_read_byte; `crc` chains calls.
uint32_t styleCrc32(const uint8_t* p, uint32_t n, uint32_t crc = 0);

// Checks a whole pack image of `size` bytes (magic, API, CRC, every offset and
// index): nullptr when it is safe to draw, else a short reason.
const char* styleValidate(const uint8_t* base, uint32_t size);

// Use the pack at `base` (validated again); false and detached when it is bad.
bool styleAttach(const uint8_t* base, uint32_t size);
void styleDetach();
bool styleActive();
const StyleInfo* styleInfo();                // nullptr when detached
uint32_t styleGeneration();                  // bumps on every attach/detach

// What the screen should say. `fresh`: live usage (else the pack's no-data
// state); `localEpoch`: local wall-clock seconds now, <= 0 unknown. Returns a
// signature of everything the paint shows: a change needs stylePaint().
uint32_t styleUpdate(const ClawdData& d, bool fresh, long localEpoch, int clockFmt);
void     stylePaint(uint32_t nowMs);         // the whole screen
void     styleAnimate(uint32_t nowMs);       // only what animation steps changed

// Diagnostics for /api/heap: the last animation repaint and the slowest since attach (us).
uint32_t styleLastFrameUs();
uint32_t styleMaxFrameUs();
