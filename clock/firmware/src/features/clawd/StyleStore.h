// StyleStore.h — where the installed style pack lives on the clock (device only).
//
// /style.pak on LittleFS is the copy that persists (OTA updates never touch the
// filesystem). StylePack reads a pack in place through the flash cache, so the
// file is mirrored into the free sketch flash right after the running image (the
// "slot", memory-mapped at 0x40200000 + offset): fonts and ops then cost no RAM.
// An OTA stages the next image in that same free space, so the mirror is checked
// (CRC) at every boot and copied again from /style.pak when it no longer matches.
// Flash is only written when a pack is installed or after a firmware update.
#pragma once
#include <stdint.h>

#define STYLE_PACK_PATH "/style.pak"
#define STYLE_TMP_PATH  "/style.pak.tmp"

void        styleStoreBegin();       // boot: mirror /style.pak if needed and use it
// Install STYLE_TMP_PATH (just uploaded): mirror, check, use it, keep it as
// STYLE_PACK_PATH. nullptr on success, else why (the previous pack is put back).
const char* styleStoreInstall();
void        styleStoreClear();       // back to the classic screen, pack deleted
void        styleStoreSuspend();     // an OTA is about to write over the slot
void        styleStoreResume();      // the OTA failed: use the pack again
uint32_t    styleStoreSlotBytes();   // room in the slot (0: none)
const char* styleStoreError();

// Debug trail for pack installs: one line per step appended to /style.log (kept
// under 4 KB), read with /api/fs/get?path=/style.log. Survives a power cycle.
void styleLog(const char* fmt, ...) __attribute__((format(printf, 1, 2)));       // why the last boot/install left no style, or ""
