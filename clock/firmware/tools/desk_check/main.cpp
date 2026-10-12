// desk_check — decode every frame of a desk.pak (CLDK v2 or v3) with the clock's own
// DeskPack.cpp and compare each crop state with the encoder's CRCs
// (build/desk/desk.crc.json from tools/desk/desk_to_pak.js). Also reports the
// SPI work per frame: spans (address windows) and pixels pushed.
//   tools/desk_check/run.sh [desk.pak] [desk.crc.json]
#include "DeskPack.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <vector>

static uint32_t crc32(const uint8_t* p, size_t n) {
  uint32_t c = 0xFFFFFFFFu;
  for (size_t i = 0; i < n; i++) {
    c ^= p[i];
    for (int k = 0; k < 8; k++) c = (c >> 1) ^ (0xEDB88320u & (0u - (c & 1u)));
  }
  return ~c;
}

struct Canvas {
  int w = 0;
  std::vector<uint8_t> px;
  long spans = 0, pixels = 0;
};

static void onSpan(void* ctx, int x, int y, int len, const uint8_t* idx) {
  Canvas* c = (Canvas*)ctx;
  memcpy(&c->px[(size_t)y * c->w + x], idx, len);
  c->spans++;
  c->pixels += len;
}

// Minimal reader for {"name":[crc,...],...}.
static bool crcsFor(const std::string& json, const char* name, std::vector<uint32_t>& out) {
  const std::string key = std::string("\"") + name + "\":[";
  size_t p = json.find(key);
  if (p == std::string::npos) return false;
  p += key.size();
  out.clear();
  while (p < json.size() && json[p] != ']') {
    char* end = nullptr;
    out.push_back((uint32_t)strtoul(json.c_str() + p, &end, 10));
    p = (size_t)(end - json.c_str());
    if (json[p] == ',') p++;
  }
  return true;
}

int main(int argc, char** argv) {
  if (argc > 1) setenv("DESK_PACK_PATH", argv[1], 1);
  const char* crcPath = argc > 2 ? argv[2] : "build/desk/desk.crc.json";
  FILE* f = fopen(crcPath, "rb");
  if (!f) { fprintf(stderr, "no %s\n", crcPath); return 2; }
  std::string json;
  char buf[4096];
  for (size_t n; (n = fread(buf, 1, sizeof(buf), f)) > 0;) json.append(buf, n);
  fclose(f);
  if (!deskPackOpen()) { fprintf(stderr, "desk.pak missing or not CLDK v2/v3\n"); return 2; }
  int bad = 0;
  for (int i = 0; i < deskPackCount(); i++) {
    DeskAnim a;
    if (!deskPackAnim(i, a)) { printf("#%d: bad descriptor\n", i); bad++; continue; }
    std::vector<uint32_t> want;
    if (!crcsFor(json, a.name, want) || want.size() != (size_t)a.frameCount + 1) {
      printf("%-14s no/odd CRC list\n", a.name); bad++; continue;
    }
    Canvas c;
    c.w = a.w;
    c.px.assign((size_t)a.w * a.h, 0);
    long maxSpans = 0, maxPx = 0, holdMs = 0;
    bool ok = true;
    for (uint16_t d = 0; d <= a.frameCount; d++) {
      const long s0 = c.spans, p0 = c.pixels;
      if (!deskPackDraw(a, d, onSpan, &c)) { printf("%-14s decode error at %u\n", a.name, d); ok = false; break; }
      if (c.spans - s0 > maxSpans) maxSpans = c.spans - s0;
      if (c.pixels - p0 > maxPx) maxPx = c.pixels - p0;
      if (crc32(c.px.data(), c.px.size()) != want[d]) { printf("%-14s CRC mismatch at %u\n", a.name, d); ok = false; break; }
      if (d < a.frameCount) holdMs += deskPackHold(a, d);
    }
    if (!ok) { bad++; continue; }
    printf("%-14s ok  %4u frames %6ld ms  per frame: avg %5ld px / %4ld spans, max %6ld px / %4ld spans\n",
           a.name, a.frameCount, holdMs, c.pixels / (a.frameCount + 1), c.spans / (a.frameCount + 1),
           maxPx, maxSpans);
  }
  printf(bad ? "FAILED: %d animation(s)\n" : "all %d animations decode to the encoder's frames\n",
         bad ? bad : deskPackCount());
  return bad ? 1 : 0;
}
