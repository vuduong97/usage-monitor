// StyleStore.cpp — /style.pak and its memory-mapped mirror (see StyleStore.h).
#include "StyleStore.h"
#include "StylePack.h"

#if defined(CLAWD_ESP8266)
#include <Arduino.h>
#include <LittleFS.h>
#include <flash_hal.h>
#include <user_interface.h>
#include "../../CrashLog.h"

#define FLASH_MAP_BASE 0x40200000u   // the first MB of flash, through the cache

static bool        g_suspended = false;
static const char* g_error = "";

// "The pack is drawing" in RTC user memory (survives a crash reset, not a power
// cut). CrashLog uses words 96-117, eboot the first ones.
#define RTC_DRAW_WORD 124
#define RTC_DRAW_MAGIC 0x5717E000u
static bool g_drawing = false;

static void rtcDrawing(bool on) {
  uint32_t v = RTC_DRAW_MAGIC | (on ? 1u : 0u);
  ESP.rtcUserMemoryWrite(RTC_DRAW_WORD, &v, sizeof(v));
}

void styleDrawing(bool on) {
  if (on == g_drawing) return;
  g_drawing = on;
  rtcDrawing(on);
}

// The last reset was a crash while the pack drew: an exception, a soft or hardware
// watchdog, or a panic/stack overflow (which this core reports as a plain restart;
// CrashLog tells it apart).
static bool crashedWhileDrawing() {
  uint32_t v = 0;
  ESP.rtcUserMemoryRead(RTC_DRAW_WORD, &v, sizeof(v));
  const rst_info* ri = ESP.getResetInfoPtr();
  const bool crash = crashLogBootWasCrash() ||
                     (ri && (ri->reason == REASON_WDT_RST || ri->reason == REASON_EXCEPTION_RST ||
                             ri->reason == REASON_SOFT_WDT_RST));
  return v == (RTC_DRAW_MAGIC | 1u) && crash;
}

#define STYLE_LOG_PATH "/style.log"
void styleLog(const char* fmt, ...) {
  char line[96];
  int n = snprintf(line, sizeof(line), "%lu ", (unsigned long)millis());
  va_list ap;
  va_start(ap, fmt);
  vsnprintf(line + n, sizeof(line) - n, fmt, ap);
  va_end(ap);
  File f = LittleFS.open(STYLE_LOG_PATH, "a");
  if (!f) return;
  if (f.size() > 4096) {                          // keep the newest 2 KB
    f.close();
    File in = LittleFS.open(STYLE_LOG_PATH, "r");
    File out = LittleFS.open(STYLE_LOG_PATH ".tmp", "w");
    if (in && out) {
      in.seek(in.size() - 2048);
      in.readStringUntil('\n');                   // from the start of a line
      uint8_t buf[128];
      for (int n; (n = in.read(buf, sizeof(buf))) > 0;) out.write(buf, n);
    }
    in.close();
    out.close();
    LittleFS.remove(STYLE_LOG_PATH);
    LittleFS.rename(STYLE_LOG_PATH ".tmp", STYLE_LOG_PATH);
    f = LittleFS.open(STYLE_LOG_PATH, "a");
    if (!f) return;
  }
  f.println(line);
  f.close();
}

static uint32_t slotOffset() {
  return (ESP.getSketchSize() + FLASH_SECTOR_SIZE - 1) & ~(uint32_t)(FLASH_SECTOR_SIZE - 1);
}

uint32_t styleStoreSlotBytes() {
  const uint32_t end = FS_start - FLASH_MAP_BASE, off = slotOffset();
  if (end <= off) return 0;
  return end - off < STYLE_MAX_BYTES ? end - off : STYLE_MAX_BYTES;
}

static const uint8_t* slot() { return reinterpret_cast<const uint8_t*>(FLASH_MAP_BASE + slotOffset()); }

// Cheap checks on the file before the slot (and the pack on screen) is given up
// for it: magic, API, size and the CRC, streamed. The full check runs on the
// mapped copy (styleValidate) once it is in flash.
static const char* precheck(File& f, uint32_t n) {
  uint8_t head[16];
  f.seek(0);
  if (f.read(head, sizeof(head)) != (int)sizeof(head)) return "read";
  if (memcmp(head, "CSTY", 4) != 0) return "magic";
  if (head[4] != STYLE_API) return "api";
  uint32_t size, crc;
  memcpy(&size, head + 8, 4);
  memcpy(&crc, head + 12, 4);
  if (size != n) return "size";
  uint8_t buf[128];
  uint32_t c = 0;
  for (uint32_t done = 16; done < n;) {
    const uint32_t want = n - done < sizeof(buf) ? n - done : (uint32_t)sizeof(buf);
    if (f.read(buf, want) != (int)want) return "read";
    c = styleCrc32(buf, want, c);                 // pgm_read_byte reads RAM too on this core
    done += want;
    yield();
  }
  return c == crc ? nullptr : "crc";
}

// Copy file `f` into the slot and use it. nullptr when the mapped copy validates.
static const char* mirror(File& f) {
  const uint32_t n = f.size();
  if (n < STYLE_HEADER_BYTES || n > styleStoreSlotBytes()) return "size";
  const char* bad = precheck(f, n);
  if (bad) return bad;
  styleDetach();                                  // never draw from a slot being rewritten
  const uint32_t off = slotOffset();
  const uint32_t sectors = (n + FLASH_SECTOR_SIZE - 1) / FLASH_SECTOR_SIZE;
  styleLog("mirror n=%u off=0x%x sectors=%u heap=%u", (unsigned)n, (unsigned)off, (unsigned)sectors, (unsigned)ESP.getFreeHeap());
  for (uint32_t s = 0; s < sectors; s++) {
    if (!ESP.flashEraseSector(off / FLASH_SECTOR_SIZE + s)) return "erase";
    yield();
  }
  styleLog("erased");
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
  styleLog("written");
  const char* why = styleValidate(slot(), n);     // reads back through the cache
  styleLog("validate %s", why ? why : "ok");
  if (why) return why;
  const bool ok = styleAttach(slot(), n);
  styleLog("attach %d", ok);
  return ok ? nullptr : "attach";
}

void styleStoreBegin() {
  if (g_suspended) return;
  static bool checked = false;
  if (!checked) {                                 // first call, at boot
    checked = true;
    const bool skip = crashedWhileDrawing();
    styleLog("boot %s skip=%d", ESP.getResetReason().c_str(), skip);
    rtcDrawing(false);
    if (skip) {                                   // it would crash again: back to classic
      LittleFS.remove(STYLE_PACK_PATH);
      g_error = "the clock crashed drawing this pack; it was removed";
      return;
    }
  }
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
  styleLog("install size=%u", (unsigned)f.size());
  const char* why = mirror(f);
  f.close();
  styleLog("install %s", why ? why : "ok");
  if (why) {
    LittleFS.remove(STYLE_TMP_PATH);
    if (!styleActive()) styleStoreBegin();        // the slot was rewritten: the previous pack again
    return why;
  }
  LittleFS.remove(STYLE_PACK_PATH);
  if (!LittleFS.rename(STYLE_TMP_PATH, STYLE_PACK_PATH)) {
    styleDetach();                                // would not survive a reboot: do not pretend
    LittleFS.remove(STYLE_TMP_PATH);
    return "rename";
  }
  g_error = "";
  styleLog("renamed");
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
void        styleLog(const char*, ...) {}
void        styleStoreBegin() {}
const char* styleStoreInstall() { return "unsupported"; }
void        styleStoreClear() {}
void        styleStoreSuspend() {}
void        styleStoreResume() {}
uint32_t    styleStoreSlotBytes() { return 0; }
const char* styleStoreError() { return ""; }
#endif
