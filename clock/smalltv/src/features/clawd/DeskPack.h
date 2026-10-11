// DeskPack.h — the full-screen "desk" animations, /desk.pak on the filesystem.
//
// Rendered at build time from a local clawd-on-desk clone by
// tools/desk/desk_to_pak.js in the clawd-home repo (personal use only; the
// file is never committed). Every animation sits on one 60x60-cell grid (4 px
// per cell on the 240 px panel), cropped to its union box, frames run-length
// encoded: byte = colour << 4 | (run - 1).
//
// Header "CLDK" v1, 16 B; descriptors 64 B (see the tool for the full layout).
#pragma once
#include <stddef.h>
#include <stdint.h>

#define DESK_PACK_PATH "/desk.pak"
#define DESK_GRID      60

struct DeskAnim {
  char     name[16];
  uint8_t  w, h, ox, oy;        // crop on the 60x60 grid
  uint16_t frameCount;
  uint16_t palette[16];         // RGB565, index 0 = background
  uint32_t indexOff;            // u32 offsets[frameCount + 1]
  uint32_t holdsOff;            // u16 ms[frameCount]
};

bool deskPackOpen();                       // (re)open; false if missing/invalid
int  deskPackCount();                      // 0 if none
int  deskPackFind(const char* name);       // index or -1
bool deskPackAnim(int index, DeskAnim& out);
// Decode frame `f` into `buf` as 4 bpp cells (row-major over the crop, high
// nibble first — the layout clawdFrameCell reads). False on error or if the
// crop does not fit `cap` bytes.
bool     deskPackFrame(const DeskAnim& a, uint16_t f, uint8_t* buf, size_t cap);
uint16_t deskPackHold(const DeskAnim& a, uint16_t f);   // ms; 100 on error
