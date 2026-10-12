#include "DeskPack.h"
#include <string.h>

// uzlib's inflate (zlib licence, uzlib/LICENSE), copied from the ESP8266
// core's tools/sdk/uzlib. Compiled here rather than as its own file so the
// host builds (tools/clawd_preview, tools/desk_check, the desktop renderer)
// need no extra source; the .inc name keeps PlatformIO from building it twice.
#include "uzlib/uzlib.h"
#include "uzlib/tinflate.inc"

#define DESK_HEADER  16
#define DESK_DESC    64
#define DESK_STORED  0x8000        // v3 index: delta kept uncompressed
#define DESK_RING    512           // v3 deltas use a 512 B deflate window

#if defined(ARDUINO)
#include <LittleFS.h>
static File g_file;
static bool backendOpen() {
  if (g_file) g_file.close();
  g_file = LittleFS.open(DESK_PACK_PATH, "r");
  return (bool)g_file;
}
static void backendClose() {
  if (g_file) g_file.close();
}
static bool backendRead(uint32_t off, void* dst, size_t n) {
  if (!g_file) return false;
  if (g_file.position() != off && !g_file.seek(off, SeekSet)) return false;
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
static void backendClose() {
  if (g_file) fclose(g_file);
  g_file = nullptr;
}
static bool backendRead(uint32_t off, void* dst, size_t n) {
  if (!g_file || fseek(g_file, (long)off, SEEK_SET) != 0) return false;
  return fread(dst, 1, n, g_file) == n;
}
#endif

static uint16_t g_count = 0;
static uint16_t g_version = 0;     // 2: raw deltas; 3: per-delta raw deflate (DeskPack.h)

// Inflate state for v3, static: ~1.3 KB of Huffman tables + the ring is too
// much for the 4 KB loop stack. One delta is decoded at a time.
static struct uzlib_uncomp g_inf;
static uint8_t  g_ring[DESK_RING];
static uint8_t  g_in[128];
static uint32_t g_inOff = 0, g_inEnd = 0;

static uint16_t rdU16(const uint8_t* p) { return (uint16_t)(p[0] | (p[1] << 8)); }
static uint32_t rdU32(const uint8_t* p) {
  return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

bool deskPackOpen() {
  g_count = 0;
  uint8_t h[DESK_HEADER];
  if (!backendOpen() || !backendRead(0, h, sizeof(h))) return false;
  const uint16_t v = rdU16(h + 4);
  if (memcmp(h, "CLDK", 4) != 0 || (v != 2 && v != 3) || rdU16(h + 8) != DESK_SIZE) return false;
  const uint16_t n = rdU16(h + 6);
  if (n == 0 || n >= 64) return false;
  g_count = n;
  g_version = v;
  return true;
}

void deskPackClose() {
  g_count = 0;
  backendClose();
}

int deskPackCount() { return g_count; }

bool deskPackAnim(int i, DeskAnim& a) {
  memset(&a, 0, sizeof(a));
  if (i < 0 || i >= g_count) return false;
  uint8_t d[DESK_DESC];
  if (!backendRead(DESK_HEADER + (uint32_t)i * DESK_DESC, d, sizeof(d))) return false;
  memcpy(a.name, d, 15);
  a.x = d[16]; a.y = d[17]; a.w = d[18]; a.h = d[19];
  a.frameCount = rdU16(d + 20);
  a.loopStart = rdU16(d + 22);
  a.flags = d[26];
  for (int k = 0; k < 16; k++) a.palette[k] = rdU16(d + 28 + 2 * k);
  a.indexOff = rdU32(d + 60);
  a.holdsOff = a.indexOff + (g_version == 3 ? 8u * (a.frameCount + 1u) : 4u * (a.frameCount + 2u));
  return a.frameCount > 0 && a.loopStart < a.frameCount && a.w && a.h &&
         a.x + a.w <= DESK_SIZE && a.y + a.h <= DESK_SIZE;
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

// Buffered reader over [off, end) of the pack.
// uzlib pulls compressed bytes through this when g_in runs dry.
static int infRead(struct uzlib_uncomp* d) {
  if (g_inOff >= g_inEnd) return -1;
  const uint32_t n = g_inEnd - g_inOff > sizeof(g_in) ? sizeof(g_in) : g_inEnd - g_inOff;
  if (!backendRead(g_inOff, g_in, n)) return -1;
  g_inOff += n;
  d->source = g_in + 1;
  d->source_limit = g_in + n;
  return g_in[0];
}

// Buffered reader over the ops of one delta: raw bytes [off, end) of the
// pack, or (v3, zip) `raw` bytes inflated from the stream at g_inOff.
struct OpReader {
  uint32_t off, end;
  uint8_t  buf[128];
  uint16_t pos = 0, len = 0;
  bool     err = false;
  bool     zip = false;
  uint32_t raw = 0;                 // zip: op bytes not yet inflated
  bool more() { return pos < len || (zip ? raw > 0 : off < end); }
  int next() {
    if (pos >= len && zip) {
      if (!raw) { err = true; return -1; }
      g_inf.dest_start = g_inf.dest = buf;
      g_inf.dest_limit = buf + (raw > sizeof(buf) ? sizeof(buf) : raw);
      const int res = uzlib_uncompress(&g_inf);
      const uint32_t n = (uint32_t)(g_inf.dest - buf);
      if (res < 0 || n == 0 || g_inf.eof) { err = true; return -1; }
      raw -= n;
      len = (uint16_t)n;
      pos = 0;
    } else if (pos >= len) {
      if (off >= end) { err = true; return -1; }
      const uint32_t n = end - off > sizeof(buf) ? sizeof(buf) : end - off;
      if (!backendRead(off, buf, n)) { err = true; return -1; }
      off += n;
      len = (uint16_t)n;
      pos = 0;
    }
    return buf[pos++];
  }
};

// Collects one row's changed run and hands it out on a skip or at the row end.
struct SpanOut {
  DeskSpanFn fn;
  void*      ctx;
  int        w;
  uint32_t   start = 0;            // crop index of span[0]
  int        n = 0;
  uint8_t    span[DESK_SIZE];
  void flush() {
    if (n) fn(ctx, (int)(start % w), (int)(start / w), n, span);
    n = 0;
  }
  void put(uint32_t i, uint8_t v) {
    if (n && (i % w == 0)) flush();          // a run never wraps onto the next row
    if (!n) start = i;
    span[n++] = v;
  }
};

bool deskPackDraw(const DeskAnim& a, uint16_t d, DeskSpanFn fn, void* ctx, bool full) {
  if (!fn || d > a.frameCount) return false;
  uint8_t ix[8];
  OpReader r;
  if (g_version == 3) {
    if (!backendRead(a.indexOff + 8u * d, ix, 8)) return false;
    const uint32_t at = rdU32(ix);
    const uint16_t stored = rdU16(ix + 4), raw = rdU16(ix + 6);
    r.off = at;
    r.end = at + stored;
    if (stored && !(raw & DESK_STORED)) {
      r.zip = true;
      r.raw = raw;
      g_inOff = at;
      g_inEnd = at + stored;
      uzlib_uncompress_init(&g_inf, g_ring, sizeof(g_ring));
      g_inf.source = g_inf.source_limit = nullptr;
      g_inf.source_read_cb = infRead;
    }
  } else {
    if (!backendRead(a.indexOff + 4u * d, ix, 8)) return false;
    r.off = rdU32(ix);
    r.end = rdU32(ix + 4);
  }
  if (r.end < r.off) return false;
  SpanOut out;
  out.fn = fn;
  out.ctx = ctx;
  out.w = a.w;
  const uint32_t cells = (uint32_t)a.w * a.h;
  uint32_t i = 0;
  while (r.more()) {
    const int op = r.next();
    if (op < 0) return false;
    if (op < 0x40) {                                   // skip
      uint32_t k = (uint32_t)op + 1;
      if (op == 0x3F) {
        const int lo = r.next(), hi = r.next();
        if (lo < 0 || hi < 0) return false;
        k = (uint32_t)lo | ((uint32_t)hi << 8);
      }
      if (full) {
        if (i + k > cells) return false;
        while (k--) out.put(i++, 0);
      } else {
        out.flush();
        i += k;
      }
    } else if (op < 0x80) {                            // literal pixels
      const int k = (op & 0x3F) + 1;
      if (i + k > cells) return false;
      for (int j = 0; j < k; j += 2) {
        const int b = r.next();
        if (b < 0) return false;
        out.put(i++, (uint8_t)(b >> 4));
        if (j + 1 < k) out.put(i++, (uint8_t)(b & 0x0F));
      }
    } else {                                           // colour run
      const uint8_t c = (uint8_t)((op >> 3) & 0x0F);
      uint32_t k = (uint32_t)(op & 7) + 1;
      if (k == 8) {
        const int e = r.next();
        if (e < 0) return false;
        k = 8 + (uint32_t)e;
      }
      if (i + k > cells) return false;
      while (k--) out.put(i++, c);
    }
    if (i > cells) return false;
  }
  if (full) while (i < cells) out.put(i++, 0);
  out.flush();
  return !r.err;
}

uint16_t deskPackHold(const DeskAnim& a, uint16_t f) {
  uint8_t b[2];
  if (f >= a.frameCount || !backendRead(a.holdsOff + 2u * f, b, 2)) return 50;
  uint16_t h = rdU16(b);
  return h ? h : 50;
}
