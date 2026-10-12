// StyleStore.cpp — /style.pak and its memory-mapped mirror (see StyleStore.h).
#include "StyleStore.h"
#include "StylePack.h"

#if defined(CLAWD_ESP8266)
#include <Arduino.h>
#include <LittleFS.h>
#include <flash_hal.h>

#define FLASH_MAP_BASE 0x40200000u   // the first MB of flash, through the cache

static bool        g_suspended = false;
static const char* g_error = "";

static uint32_t slotOffset() {
  return (ESP.getSketchSize() + FLASH_SECTOR_SIZE - 1) & ~(uint32_t)(FLASH_SECTOR_SIZE - 1);
}

uint32_t styleStoreSlotBytes() {
  const uint32_t end = FS_start - FLASH_MAP_BASE, off = slotOffset();
  if (end <= off) return 0;
  return end - off < STYLE_MAX_BYTES ? end - off : STYLE_MAX_BYTES;
}

static const uint8_t* slot() { return reinterpret_cast<const uint8_t*>(FLASH_MAP_BASE + slotOffset()); }

// Copy file `f` into the slot and use it. nullptr when the mapped copy validates.
static const char* mirror(File& f) {
  const uint32_t n = f.size();
  if (n < STYLE_HEADER_BYTES || n > styleStoreSlotBytes()) return "size";
  styleDetach();                                  // never draw from a slot being rewritten
  const uint32_t off = slotOffset();
  const uint32_t sectors = (n + FLASH_SECTOR_SIZE - 1) / FLASH_SECTOR_SIZE;
  for (uint32_t s = 0; s < sectors; s++) {
    if (!ESP.flashEraseSector(off / FLASH_SECTOR_SIZE + s)) return "erase";
    yield();
  }
  uint32_t buf[64];                               // flashWrite wants whole aligned words
  f.seek(0);
  for (uint32_t done = 0; done < n;) {
    const uint32_t want = n - done < sizeof(buf) ? n - done : (uint32_t)sizeof(buf);
    memset(buf, 0xFF, sizeof(buf));
    if (f.read(reinterpret_cast<uint8_t*>(buf), want) != (int)want) return "read";
    if (!ESP.flashWrite(off + done, buf, (want + 3) & ~3u)) return "write";
    done += want;
    yield();
  }
  const char* why = styleValidate(slot(), n);     // reads back through the cache
  if (why) return why;
  return styleAttach(slot(), n) ? nullptr : "attach";
}

void styleStoreBegin() {
  if (g_suspended) return;
  File f = LittleFS.open(STYLE_PACK_PATH, "r");
  if (!f) { g_error = ""; return; }
  const uint32_t n = f.size();
  uint8_t head[16], mapped[16];
  // still mirrored (no OTA since): same header and the CRC holds (styleAttach checks it)
  if (n >= STYLE_HEADER_BYTES && n <= styleStoreSlotBytes() && f.read(head, sizeof(head)) == sizeof(head)) {
    memcpy_P(mapped, slot(), sizeof(mapped));
    if (memcmp(head, mapped, sizeof(head)) == 0 && styleAttach(slot(), n)) {
      f.close();
      g_error = "";
      return;
    }
  }
  const char* why = mirror(f);
  f.close();
  g_error = why ? why : "";
  if (why) styleDetach();
}

const char* styleStoreInstall() {
  if (g_suspended) return "busy";
  File f = LittleFS.open(STYLE_TMP_PATH, "r");
  if (!f) return "no upload";
  const char* why = mirror(f);
  f.close();
  if (why) {
    LittleFS.remove(STYLE_TMP_PATH);
    styleStoreBegin();                            // the previous pack, if it is still there
    return why;
  }
  LittleFS.remove(STYLE_PACK_PATH);
  if (!LittleFS.rename(STYLE_TMP_PATH, STYLE_PACK_PATH)) {
    styleDetach();                                // would not survive a reboot: do not pretend
    LittleFS.remove(STYLE_TMP_PATH);
    return "rename";
  }
  g_error = "";
  return nullptr;
}

void styleStoreClear() {
  styleDetach();
  LittleFS.remove(STYLE_TMP_PATH);
  LittleFS.remove(STYLE_PACK_PATH);
  g_error = "";
}

void styleStoreSuspend() {
  g_suspended = true;
  styleDetach();
}

void styleStoreResume() {
  if (!g_suspended) return;
  g_suspended = false;
  styleStoreBegin();
}

const char* styleStoreError() { return g_error; }

#else   // no flash slot on this platform: the classic screen only
void        styleStoreBegin() {}
const char* styleStoreInstall() { return "unsupported"; }
void        styleStoreClear() {}
void        styleStoreSuspend() {}
void        styleStoreResume() {}
uint32_t    styleStoreSlotBytes() { return 0; }
const char* styleStoreError() { return ""; }
#endif
