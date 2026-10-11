// ClawdPack.h — the Clawd animation pack, /clawd.pak on the filesystem.
//
// The animations (~210 KB) are not linked into the image: the clock keeps its
// stock 4m3m flash layout (so the stock firmware can be flashed back with its
// filesystem intact), and there the OTA budget for the whole image is ~550 KB.
// The pack is uploaded once as a file and read a frame at a time.
//
// Format: see tools/clawd_assets.js. Backends: LittleFS on the device, stdio
// in the desktop preview (CLAWD_PACK_PATH or ./data/clawd.pak).
#pragma once
#include <stddef.h>
#include <stdint.h>
#include "ClawdAssets.h"

#define CLAWD_PACK_PATH     "/clawd.pak"
#define CLAWD_MAX_FRAME     832     // largest packed frame (racing car: 825 B)

bool clawdPackOpen();                          // (re)open; false if missing/invalid
bool clawdPackOk();
int  clawdPackCount();                         // animations in the pack (0 if none)
bool clawdPackAnim(int index, ClawdAnim& out); // read one descriptor
bool clawdPackRead(uint32_t off, void* dst, size_t n);

// Load frame `frame` of `a` into buf (CLAWD_MAX_FRAME bytes). False on error
// or if the frame does not fit.
bool     clawdPackFrame(const ClawdAnim& a, uint16_t frame, uint8_t* buf);
uint16_t clawdPackHold(const ClawdAnim& a, uint16_t frame);   // ms; 100 on error
