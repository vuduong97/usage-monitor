// DeskPack.h — the full-screen "desk" animations, /desk.pak on the filesystem.
//
// Rendered at build time from a local clawd-on-desk clone by
// tools/desk/desk_to_pak.js in the clawd-home repo (personal use only; the
// file is never committed). Format CLDK v2: every animation is 1:1 on the
// 240x240 panel, cropped to its union box, ≤15 colours + black. Frame 0 is a
// delta from a black crop, every later frame a delta from the previous one,
// and one extra "wrap" delta leads from the last frame back to loopStart, so
// the player only redraws the pixels that change (the panel itself holds the
// previous frame: no framebuffer). Op codes: tools/desk/desk_codec.js.
#pragma once
#include <stddef.h>
#include <stdint.h>

#define DESK_PACK_PATH "/desk.pak"
#define DESK_SIZE      240

#define DESK_FLAG_ONCE 0x01        // plays once, then holds its last frame

struct DeskAnim {
  char     name[16];
  uint8_t  x, y, w, h;          // crop on the panel
  uint16_t frameCount;          // frames, not counting the wrap delta
  uint16_t loopStart;           // frame the wrap delta lands on
  uint8_t  flags;
  uint16_t palette[16];         // RGB565, index 0 = background
  uint32_t indexOff;            // u32 offsets[frameCount + 2]
  uint32_t holdsOff;            // u16 ms[frameCount]
};

// One horizontal run of changed pixels: `len` palette indexes starting at
// crop column x, crop row y.
typedef void (*DeskSpanFn)(void* ctx, int x, int y, int len, const uint8_t* idx);

bool deskPackOpen();                       // (re)open; false if missing/invalid
void deskPackClose();                      // release the file (before replacing it)
int  deskPackCount();                      // 0 if none
int  deskPackFind(const char* name);       // index or -1
bool deskPackAnim(int index, DeskAnim& out);
// Apply delta `d` (0..frameCount-1 = frames, frameCount = the wrap) by calling
// `fn` for every changed run, top to bottom. False on a read error or bad data
// (what was drawn so far stays drawn). `full` also paints the skipped pixels
// as index 0: for frame 0 (a delta from black) that repaints the whole crop
// over whatever is on screen, without blacking it out first.
bool     deskPackDraw(const DeskAnim& a, uint16_t d, DeskSpanFn fn, void* ctx, bool full = false);
uint16_t deskPackHold(const DeskAnim& a, uint16_t f);   // ms; 50 on error
