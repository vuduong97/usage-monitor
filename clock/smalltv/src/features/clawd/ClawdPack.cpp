#include "ClawdPack.h"
#include <string.h>

#define PAK_HEADER 12
#define PAK_DESC   72

#if defined(ARDUINO)
#include <LittleFS.h>
static File g_file;
static bool backendOpen() {
  if (g_file) g_file.close();
  g_file = LittleFS.open(CLAWD_PACK_PATH, "r");
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
  const char* p = getenv("CLAWD_PACK_PATH");
  g_file = fopen(p ? p : "data/clawd.pak", "rb");
  return g_file != nullptr;
}
static bool backendRead(uint32_t off, void* dst, size_t n) {
  if (!g_file || fseek(g_file, (long)off, SEEK_SET) != 0) return false;
  return fread(dst, 1, n, g_file) == n;
}
#endif

static bool     g_ok = false;
static uint16_t g_count = 0;

static uint16_t rdU16(const uint8_t* p) { return (uint16_t)(p[0] | (p[1] << 8)); }
static uint32_t rdU32(const uint8_t* p) {
  return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

bool clawdPackOpen() {
  g_ok = false;
  g_count = 0;
  uint8_t h[PAK_HEADER];
  if (!backendOpen() || !backendRead(0, h, sizeof(h))) return false;
  if (memcmp(h, "CLWD", 4) != 0 || rdU16(h + 4) != 1) return false;
  g_count = rdU16(h + 6);
  g_ok = g_count > 0 && g_count < 64;
  if (!g_ok) g_count = 0;
  return g_ok;
}

bool clawdPackOk() { return g_ok; }
int  clawdPackCount() { return g_count; }

bool clawdPackRead(uint32_t off, void* dst, size_t n) { return g_ok && backendRead(off, dst, n); }

bool clawdPackAnim(int i, ClawdAnim& a) {
  memset(&a, 0, sizeof(a));
  if (!g_ok || i < 0 || i >= g_count) return false;
  uint8_t d[PAK_DESC];
  if (!backendRead(PAK_HEADER + (uint32_t)i * PAK_DESC, d, sizeof(d))) return false;
  memcpy(a.name, d, 15);
  a.name[15] = 0;
  a.w = d[16]; a.h = d[17]; a.ox = d[18]; a.oy = d[19];
  a.frameCount = rdU16(d + 20);
  a.loopStart  = rdU16(d + 22);
  a.loopEnd    = rdU16(d + 24);
  a.frameBytes = rdU16(d + 26);
  for (int k = 0; k < 16; k++) a.palette[k] = rdU16(d + 28 + 2 * k);
  a.framesOff = rdU32(d + 60);
  a.holdsOff  = rdU32(d + 64);
  return a.frameCount > 0 && a.frameBytes <= CLAWD_MAX_FRAME;
}

bool clawdPackFrame(const ClawdAnim& a, uint16_t frame, uint8_t* buf) {
  if (!buf || frame >= a.frameCount || a.frameBytes > CLAWD_MAX_FRAME) return false;
  return clawdPackRead(a.framesOff + (uint32_t)frame * a.frameBytes, buf, a.frameBytes);
}

uint16_t clawdPackHold(const ClawdAnim& a, uint16_t frame) {
  uint8_t b[2];
  if (frame >= a.frameCount || !clawdPackRead(a.holdsOff + 2u * frame, b, 2)) return 100;
  uint16_t h = rdU16(b);
  return h ? h : 100;
}
