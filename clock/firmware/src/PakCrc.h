// PakCrc.h — CRC-32 (zlib) of /clawd.pak and /desk.pak, reported in /api/heap
// (pakCrc, deskCrc) so the desktop app only re-sends a pack that differs.
//
// A pack uploaded through /api/clawdpak or /api/deskpak gets its CRC while it
// streams in. Any other pack (an older firmware's, or one put with /api/fs/put)
// is checksummed in the background, a few KB per loop(), so nothing blocks.
// The result is kept next to the pack ("<pack>.crc": "<crc hex> <size>") so it
// is computed once per pack, not on every boot.
#pragma once
#include <stdint.h>

enum : uint8_t { PAK_CLAWD = 0, PAK_DESK = 1 };

void pakCrcBegin();                            // forget both (boot)
void pakCrcForget(uint8_t which);              // the pack changed or went away
void pakCrcSet(uint8_t which, uint32_t crc);   // computed while it was uploaded
void pakCrcService();                          // background work: call from loop()
// "" while unknown (no pack, or still being computed), else 8 hex digits.
const char* pakCrcHex(uint8_t which);
// The pack index for a filesystem path, -1 when it is not a pack.
int pakCrcIndex(const char* path);
