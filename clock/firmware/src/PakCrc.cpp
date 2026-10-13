// PakCrc.cpp — see PakCrc.h.
#include "PakCrc.h"
#include <Arduino.h>
#include <LittleFS.h>
#include <string.h>
#include "ClawdPack.h"
#include "DeskPack.h"
#include "StylePack.h"   // styleCrc32: zlib CRC-32, chainable

#define PAK_CRC_START_MS 30000UL   // leave the first seconds after boot to WiFi and the web UI
#define PAK_CRC_SLICE_MS 3          // background checksum time per loop()

static const char* const PATHS[2] = {CLAWD_PACK_PATH, DESK_PACK_PATH};
static const char* const SIDECARS[2] = {CLAWD_PACK_PATH ".crc", DESK_PACK_PATH ".crc"};

struct PakState {
  bool     known = false;   // hex holds the pack's CRC
  bool     absent = false;  // no pack: nothing to do until it changes
  bool     working = false; // checksumming in the background
  uint32_t crc = 0, size = 0, off = 0;
  char     hex[9] = "";
};
static PakState g_st[2];
static File g_work;         // the pack being checksummed (one at a time)
static int8_t g_workIdx = -1;

static void stopWork() {
  if (g_work) g_work.close();
  g_workIdx = -1;
}

static void setKnown(uint8_t i, uint32_t crc) {
  PakState& s = g_st[i];
  s.known = true;
  s.working = false;
  s.absent = false;
  s.crc = crc;
  snprintf(s.hex, sizeof(s.hex), "%08x", (unsigned)crc);
}

static void writeSidecar(uint8_t i, uint32_t crc, uint32_t size) {
  File f = LittleFS.open(SIDECARS[i], "w");
  if (!f) return;
  f.printf("%08x %u", (unsigned)crc, (unsigned)size);
  f.close();
}

void pakCrcBegin() {
  stopWork();
  for (auto& s : g_st) s = PakState();
}

void pakCrcForget(uint8_t which) {
  if (which > PAK_DESK) return;
  if (g_workIdx == which) stopWork();
  g_st[which] = PakState();
  LittleFS.remove(SIDECARS[which]);
}

void pakCrcSet(uint8_t which, uint32_t crc) {
  if (which > PAK_DESK) return;
  if (g_workIdx == which) stopWork();
  File f = LittleFS.open(PATHS[which], "r");
  if (!f) { pakCrcForget(which); return; }
  const uint32_t size = f.size();
  f.close();
  setKnown(which, crc);
  writeSidecar(which, crc, size);
}

const char* pakCrcHex(uint8_t which) {
  return which <= PAK_DESK && g_st[which].known ? g_st[which].hex : "";
}

int pakCrcIndex(const char* path) {
  if (!path) return -1;
  for (int i = 0; i < 2; i++)
    if (strcmp(path, PATHS[i]) == 0) return i;
  return -1;
}

// A sidecar that matches the pack's size: its CRC, without reading the pack.
static bool readSidecar(uint8_t i, uint32_t size) {
  File f = LittleFS.open(SIDECARS[i], "r");
  if (!f) return false;
  char buf[24];
  const size_t n = f.readBytes(buf, sizeof(buf) - 1);
  f.close();
  buf[n] = 0;
  unsigned crc = 0, sz = 0;
  if (sscanf(buf, "%8x %u", &crc, &sz) != 2 || sz != size) return false;
  setKnown(i, crc);
  return true;
}

void pakCrcService() {
  if (millis() < PAK_CRC_START_MS) return;
  if (g_workIdx < 0) {
    for (uint8_t i = 0; i < 2; i++) {
      PakState& s = g_st[i];
      if (s.known || s.absent) continue;
      File f = LittleFS.open(PATHS[i], "r");
      if (!f) { s.absent = true; continue; }
      s.size = f.size();
      if (readSidecar(i, s.size)) { f.close(); continue; }
      g_work = f;
      g_workIdx = (int8_t)i;
      s.working = true;
      s.off = 0;
      s.crc = 0;
      break;
    }
    if (g_workIdx < 0) return;
  }
  PakState& s = g_st[g_workIdx];
  uint8_t buf[256];
  const uint32_t t0 = millis();
  while (s.off < s.size && millis() - t0 < PAK_CRC_SLICE_MS) {
    const int n = g_work.read(buf, sizeof(buf));
    if (n <= 0) break;
    s.crc = styleCrc32(buf, (uint32_t)n, s.crc);
    s.off += (uint32_t)n;
  }
  if (s.off >= s.size || !g_work.available()) {
    const uint8_t i = (uint8_t)g_workIdx;
    const bool whole = s.off == s.size;
    const uint32_t crc = s.crc, size = s.size;
    stopWork();
    if (whole) {
      setKnown(i, crc);
      writeSidecar(i, crc, size);
    } else {
      g_st[i] = PakState();     // short read: give up until the pack changes
      g_st[i].absent = true;
    }
  }
}
