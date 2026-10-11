#include "DeskPack.h"
#include <string.h>

#define DESK_HEADER 16
#define DESK_DESC   64

#if defined(ARDUINO)
#include <LittleFS.h>
static File g_file;
static bool backendOpen() {
  if (g_file) g_file.close();
  g_file = LittleFS.open(DESK_PACK_PATH, "r");
  return (bool)g_file;
}
static bool backendRead(uint32_t off, void* dst, size_t n) {
  if (!g_file || !g_file.seek(off, SeekSet)) return false;
  return g_file.read((uint8_t*)dst, n) == n;
}
#else
#include <stdio.h>
#include <stdlib.h>
static FILE* g_file = nullptr;
static bool backendOpen() {
  if (g_file) fclose(g_file);
  const char* p = getenv("DESK_PACK_PATH");
  g_file = fopen(p ? p : "data/desk.pak", "rb");
  return g_file != nullptr;
}
static bool backendRead(uint32_t off, void* dst, size_t n) {
  if (!g_file || fseek(g_file, (long)off, SEEK_SET) != 0) return false;
  return fread(dst, 1, n, g_file) == n;
}
#endif

static uint16_t g_count = 0;

static uint16_t rdU16(const uint8_t* p) { return (uint16_t)(p[0] | (p[1] << 8)); }
static uint32_t rdU32(const uint8_t* p) {
  return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

bool deskPackOpen() {
  g_count = 0;
  uint8_t h[DESK_HEADER];
  if (!backendOpen() || !backendRead(0, h, sizeof(h))) return false;
  if (memcmp(h, "CLDK", 4) != 0 || rdU16(h + 4) != 1 || h[8] != DESK_GRID) return false;
  const uint16_t n = rdU16(h + 6);
  if (n == 0 || n >= 64) return false;
  g_count = n;
  return true;
}

int deskPackCount() { return g_count; }

bool deskPackAnim(int i, DeskAnim& a) {
  memset(&a, 0, sizeof(a));
  if (i < 0 || i >= g_count) return false;
  uint8_t d[DESK_DESC];
  if (!backendRead(DESK_HEADER + (uint32_t)i * DESK_DESC, d, sizeof(d))) return false;
  memcpy(a.name, d, 15);
  a.w = d[16]; a.h = d[17]; a.ox = d[18]; a.oy = d[19];
  a.frameCount = rdU16(d + 20);
  for (int k = 0; k < 16; k++) a.palette[k] = rdU16(d + 28 + 2 * k);
  a.indexOff = rdU32(d + 60);
  a.holdsOff = a.indexOff + 4u * (a.frameCount + 1u);
  return a.frameCount > 0 && a.w && a.h && a.ox + a.w <= DESK_GRID && a.oy + a.h <= DESK_GRID;
}

int deskPackFind(const char* name) {
  if (!name || !name[0]) return -1;
  uint8_t n[16];
  for (int i = 0; i < g_count; i++) {
    if (!backendRead(DESK_HEADER + (uint32_t)i * DESK_DESC, n, 16)) return -1;
    n[15] = 0;
    if (strcmp((const char*)n, name) == 0) return i;
  }
  return -1;
}

bool deskPackFrame(const DeskAnim& a, uint16_t f, uint8_t* buf, size_t cap) {
  const uint32_t cells = (uint32_t)a.w * a.h;
  if (!buf || f >= a.frameCount || (cells + 1) / 2 > cap) return false;
  uint8_t ix[8];
  if (!backendRead(a.indexOff + 4u * f, ix, 8)) return false;
  uint32_t off = rdU32(ix), end = rdU32(ix + 4);
  if (end < off) return false;
  memset(buf, 0, (cells + 1) / 2);
  uint32_t c = 0;
  uint8_t chunk[64];
  while (off < end && c < cells) {
    size_t n = end - off > sizeof(chunk) ? sizeof(chunk) : (size_t)(end - off);
    if (!backendRead(off, chunk, n)) return false;
    off += n;
    for (size_t k = 0; k < n && c < cells; k++) {
      const uint8_t v = chunk[k] >> 4;
      uint32_t run = (chunk[k] & 0x0F) + 1u;
      if (v == 0) { c += run; continue; }        // background: already zeroed
      for (; run && c < cells; run--, c++) buf[c >> 1] |= (c & 1) ? v : (uint8_t)(v << 4);
    }
  }
  return true;
}

uint16_t deskPackHold(const DeskAnim& a, uint16_t f) {
  uint8_t b[2];
  if (f >= a.frameCount || !backendRead(a.holdsOff + 2u * f, b, 2)) return 100;
  uint16_t h = rdU16(b);
  return h ? h : 100;
}
