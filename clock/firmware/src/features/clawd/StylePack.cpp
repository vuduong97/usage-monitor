// StylePack.cpp — interpreter for the style packs (see StylePack.h, docs/STYLE_PACK.md).
#include "StylePack.h"
#include "ClawdAnim.h"
#include "ClawdDraw.h"
#include <stdio.h>
#include <string.h>

// ---- pack layout (little-endian, offsets from the pack start) ---------------------
enum : uint8_t { K_RECT = 1, K_TEXT = 2, K_LINE = 3, K_TRI = 4, K_MASK = 5 };
enum : uint8_t { SRC_NONE = 0, SRC_S = 1, SRC_W = 2, SRC_TS = 3, SRC_TW = 4, SRC_COUNT = 5, SRC_FIXED = 255 };
enum : uint8_t { F_ZERO_OK = 1, F_UPPER = 2, F_CLAMP = 4 };
enum : uint8_t { P_SOLID = 0, P_DIAG = 1, P_COLS = 2, P_ROWS = 3 };
enum : uint16_t {
  B_OK = 1 << 0, B_HOT = 1 << 1, B_WHOT = 1 << 2, B_FRESH = 1 << 3, B_OVER = 1 << 4,
  B_PACEOVER = 1 << 5, B_OUT = 1 << 6, B_CLOCK = 1 << 7, B_SR = 1 << 8, B_WR = 1 << 9,
};
#define TOKEN_COUNT 13
#define TOKEN_LEN 16
#define BUILTIN_MAX 4
#define FONT_MAX 8

struct Header {                     // 96 bytes
  char     magic[4];
  uint8_t  api, flags;
  uint16_t headerSize;
  uint32_t size, crc;
  char     id[16];
  char     name[24];
  uint16_t bg;
  uint8_t  fontCount, trackCount;
  uint16_t opCount, paletteCount;
  uint8_t  maskCount, pad[3];
  uint32_t paletteOff, fontsOff, tracksOff, opsOff, masksOff, stringsOff, stringsLen;
};
static_assert(sizeof(Header) == STYLE_HEADER_BYTES, "header layout");

struct FontDesc {                   // 24 bytes
  uint32_t glyphsOff, bitmapOff, bitmapLen, kernsOff;
  uint16_t kernCount;
  uint8_t  glyphCount, lineHeight, baseLine, pad[2], builtin;
};
static_assert(sizeof(FontDesc) == 24, "font layout");

struct TrackDesc { uint16_t stepMs; uint8_t steps, pad; uint32_t stepsOff; };
struct Step { int8_t dx, dy; uint8_t vis, color; };
struct MaskDesc { uint8_t w, h, pad[2]; uint32_t bitsOff; };

struct Op {                         // 36 bytes
  uint8_t  kind, color;
  uint16_t need, notMask;
  uint8_t  track, flags;
  int16_t  x, y, w, h;
  uint8_t  dxSrc, x1Src;
  int16_t  dxSpan, x1Span;
  uint16_t phase;
  uint8_t  gateSrc, a;
  uint16_t gateLo, gateHi;
  uint8_t  b, c;
  int16_t  x3, y3;
};
static_assert(sizeof(Op) == 36, "op layout");

// ---- state -----------------------------------------------------------------------
static const ClawdFont* const* g_builtin = nullptr;
static uint8_t   g_builtinCount = 0;
static const uint8_t* g_base = nullptr;
static Header    g_h;
static StyleInfo g_info;
static ClawdFont g_fonts[FONT_MAX];
static bool      g_fontBuiltin[FONT_MAX];
static uint32_t  g_gen = 0;

static uint16_t g_bits = 0;
static uint16_t g_val[SRC_COUNT];            // 0..1000 per source
static char     g_tok[TOKEN_COUNT][TOKEN_LEN];

static uint8_t  g_last[STYLE_MAX_ANIMATED];  // step each animated op was last drawn at
static uint32_t g_frameUs = 0, g_frameMaxUs = 0, g_paintUs = 0, g_paintMaxUs = 0;
__attribute__((weak)) void styleDrawing(bool) {}

void styleSetBuiltinFonts(const ClawdFont* const* fonts, uint8_t count) {
  g_builtin = fonts;
  g_builtinCount = count > BUILTIN_MAX ? BUILTIN_MAX : count;
}

uint32_t styleCrc32(const uint8_t* p, uint32_t n, uint32_t crc) {
  crc = ~crc;
  while (n--) {
    crc ^= pgm_read_byte(p++);
    for (int k = 0; k < 8; k++) crc = (crc >> 1) ^ (0xEDB88320u & (0u - (crc & 1u)));
  }
  return ~crc;
}

// ---- validation ------------------------------------------------------------------
static bool within(uint32_t off, uint32_t len, uint32_t size) { return off <= size && len <= size - off; }
static bool srcOk(uint8_t s) { return s < SRC_COUNT || s == SRC_FIXED; }

const char* styleValidate(const uint8_t* base, uint32_t size) {
  if (!base || size < sizeof(Header) || size > STYLE_MAX_BYTES) return "size";
  Header h;
  memcpy_P(&h, base, sizeof(h));
  if (memcmp(h.magic, "CSTY", 4) != 0) return "magic";
  if (h.api != STYLE_API) return "api";
  if (h.headerSize != sizeof(Header) || h.size != size) return "header";
  if (styleCrc32(base + 16, size - 16) != h.crc) return "crc";
  if (!h.opCount || h.fontCount > FONT_MAX) return "counts";
  if (!within(h.paletteOff, (uint32_t)h.paletteCount * 2, size) || !h.paletteCount || h.paletteCount > 255) return "palette";
  if (!within(h.fontsOff, (uint32_t)h.fontCount * sizeof(FontDesc), size)) return "fonts";
  for (uint8_t i = 0; i < h.fontCount; i++) {
    FontDesc f;
    memcpy_P(&f, base + h.fontsOff + i * sizeof(FontDesc), sizeof(f));
    if (f.builtin != 0xFF) {
      if (f.builtin >= BUILTIN_MAX) return "font id";
      continue;
    }
    if (!f.glyphCount || !f.lineHeight || !within(f.glyphsOff, (uint32_t)f.glyphCount * sizeof(ClawdGlyph), size) ||
        !within(f.bitmapOff, f.bitmapLen, size) || (f.kernCount && !within(f.kernsOff, (uint32_t)f.kernCount * 3, size)))
      return "font";
    for (uint8_t g = 0; g < f.glyphCount; g++) {
      ClawdGlyph q;
      memcpy_P(&q, base + f.glyphsOff + g * sizeof(ClawdGlyph), sizeof(q));
      if ((uint32_t)q.off + ((uint32_t)q.w * q.h + 1) / 2 > f.bitmapLen) return "glyph";
    }
  }
  if (!within(h.tracksOff, (uint32_t)h.trackCount * sizeof(TrackDesc), size)) return "tracks";
  for (uint8_t i = 0; i < h.trackCount; i++) {
    TrackDesc t;
    memcpy_P(&t, base + h.tracksOff + i * sizeof(TrackDesc), sizeof(t));
    if (t.stepMs < 10 || !t.steps || !within(t.stepsOff, (uint32_t)t.steps * sizeof(Step), size)) return "track";
    for (uint8_t k = 0; k < t.steps; k++) {
      Step s;
      memcpy_P(&s, base + t.stepsOff + k * sizeof(Step), sizeof(s));
      if (s.color != 0xFF && s.color >= h.paletteCount) return "step";
    }
  }
  if (!within(h.masksOff, (uint32_t)h.maskCount * sizeof(MaskDesc), size)) return "masks";
  for (uint8_t i = 0; i < h.maskCount; i++) {
    MaskDesc m;
    memcpy_P(&m, base + h.masksOff + i * sizeof(MaskDesc), sizeof(m));
    if (!m.w || !m.h || !within(m.bitsOff, (uint32_t)((m.w + 7) / 8) * m.h, size)) return "mask";
  }
  if (!h.stringsLen || !within(h.stringsOff, h.stringsLen, size) || pgm_read_byte(base + h.stringsOff + h.stringsLen - 1) != 0)
    return "strings";
  if (!within(h.opsOff, (uint32_t)h.opCount * sizeof(Op), size)) return "ops";
  uint16_t animated = 0;
  for (uint16_t i = 0; i < h.opCount; i++) {
    Op o;
    memcpy_P(&o, base + h.opsOff + i * sizeof(Op), sizeof(o));
    if (o.kind < K_RECT || o.kind > K_MASK || o.color >= h.paletteCount) return "op";
    if (o.track != 0xFF && (o.track >= h.trackCount || ++animated > STYLE_MAX_ANIMATED)) return "op track";
    if (!srcOk(o.dxSrc) || !srcOk(o.x1Src) || o.gateSrc >= SRC_COUNT) return "op source";
    if (o.kind == K_TEXT && (o.a >= h.fontCount || (uint16_t)o.w >= h.stringsLen || o.b > 2)) return "op text";
    if (o.kind == K_MASK && (o.a >= h.maskCount || !o.b || o.b > 8)) return "op mask";
    if (o.kind == K_RECT && ((o.y3 >> 8) > P_ROWS || ((o.y3 >> 8) && !o.b))) return "op pattern";
  }
  return nullptr;
}

// ---- attach ----------------------------------------------------------------------
bool styleAttach(const uint8_t* base, uint32_t size) {
  styleDetach();
  if (styleValidate(base, size)) return false;
  memcpy_P(&g_h, base, sizeof(g_h));
  for (uint8_t i = 0; i < g_h.fontCount; i++) {
    FontDesc f;
    memcpy_P(&f, base + g_h.fontsOff + i * sizeof(FontDesc), sizeof(f));
    g_fontBuiltin[i] = f.builtin != 0xFF;
    if (g_fontBuiltin[i]) {
      if (!g_builtin || f.builtin >= g_builtinCount) return false;
      g_fonts[i] = *g_builtin[f.builtin];
      continue;
    }
    g_fonts[i].bitmap = base + f.bitmapOff;
    g_fonts[i].glyphs = reinterpret_cast<const ClawdGlyph*>(base + f.glyphsOff);
    g_fonts[i].kerns = f.kernCount ? reinterpret_cast<const ClawdKern*>(base + f.kernsOff) : nullptr;
    g_fonts[i].glyphCount = f.glyphCount;
    g_fonts[i].kernCount = f.kernCount;
    g_fonts[i].lineHeight = f.lineHeight;
    g_fonts[i].baseLine = f.baseLine;
  }
  memset(&g_info, 0, sizeof(g_info));
  memcpy(g_info.id, g_h.id, sizeof(g_info.id) - 1);
  memcpy(g_info.name, g_h.name, sizeof(g_info.name) - 1);
  g_info.size = g_h.size;
  g_info.crc = g_h.crc;
  g_info.api = g_h.api;
  g_info.light = g_h.flags & 1;
  memset(g_last, 0xFF, sizeof(g_last));
  g_frameUs = g_frameMaxUs = g_paintUs = g_paintMaxUs = 0;
  g_base = base;
  g_gen++;
  return true;
}

void styleDetach() {
  if (g_base) g_gen++;
  g_base = nullptr;
}
bool styleActive() { return g_base != nullptr; }
const StyleInfo* styleInfo() { return g_base ? &g_info : nullptr; }
uint32_t styleGeneration() { return g_gen; }
uint32_t styleLastFrameUs() { return g_frameUs; }
uint32_t styleMaxFrameUs() { return g_frameMaxUs; }
uint32_t styleLastPaintUs() { return g_paintUs; }
uint32_t styleMaxPaintUs() { return g_paintMaxUs; }

// ---- data ------------------------------------------------------------------------
static float clamp100(float v) { return v < 0 ? 0 : (v > 100 ? 100 : v); }
static int   roundi(float v) { return (int)(v + (v < 0 ? -0.5f : 0.5f)); }

static void hhmm(char* out, int minuteOfDay, int fmt, size_t n = TOKEN_LEN) {   // at most "12:00 PM"
  const int h = minuteOfDay / 60, m = minuteOfDay % 60;
  if (fmt == 12) snprintf(out, n, "%d:%02d %s", h % 12 ? h % 12 : 12, m, h < 12 ? "AM" : "PM");
  else snprintf(out, n, "%02d:%02d", h, m);
}

static void span(char* out, int mins) {      // "35m", "2h 14m", "1d 4h", "---"
  if (mins < 0) snprintf(out, TOKEN_LEN, "---");
  else if (mins < 60) snprintf(out, TOKEN_LEN, "%dm", mins);
  else if (mins < 1440) snprintf(out, TOKEN_LEN, "%dh %dm", mins / 60, mins % 60);
  else snprintf(out, TOKEN_LEN, "%dd %dh", mins / 1440, (mins % 1440) / 60);
}

static const char DAYS[] PROGMEM = "SunMonTueWedThuFriSat";
static void dayName(char* out, int dow) {
  for (int i = 0; i < 3; i++) out[i] = (char)pgm_read_byte(DAYS + ((dow % 7 + 7) % 7) * 3 + i);
  out[3] = 0;
}

uint32_t styleUpdate(const ClawdData& d, bool fresh, long localEpoch, int clockFmt) {
  const float s = clamp100(d.sessionPct), w = clamp100(d.weeklyPct);
  const int sr = d.sessionResetMin, wr = d.weeklyResetMin;
  const bool ok = fresh;
  const bool clock = localEpoch > 0;
  const int nowMin = clock ? (int)((localEpoch % 86400L + 86400L) % 86400L / 60) : 0;
  const int dow = clock ? (int)(((localEpoch / 86400L) + 4) % 7) : 0;    // 1970-01-01 was a Thursday
  const float ts = sr < 0 ? 0.0f : (sr >= 300 ? 0.0f : (300 - sr) / 300.0f);
  const float tw = wr < 0 ? 0.0f : (wr >= 10080 ? 0.0f : (10080 - wr) / 10080.0f);
  const int el = 300 - sr;

  uint16_t b = 0;
  if (ok) b |= B_OK;
  if (ok && s >= 80) b |= B_HOT;
  if (ok && w >= 80) b |= B_WHOT;
  if (sr >= 0 && ts < 0.05f) b |= B_FRESH;
  if (ok && s / 100.0f > ts + 0.05f) b |= B_OVER;
  if (sr >= 0 && ts >= 0.05f && s / ts > 100.0f) b |= B_PACEOVER;
  int left = -1;                              // minutes to 100% at the current rate
  if (sr >= 0 && s < 100 && el >= 15 && s >= 1) {
    left = roundi((100 - s) * el / s);
    if (left < sr && clock) b |= B_OUT;
  }
  if (clock) b |= B_CLOCK;
  if (sr >= 0) b |= B_SR;
  if (wr >= 0) b |= B_WR;
  g_bits = b;
  g_val[SRC_NONE] = 0;
  g_val[SRC_S] = (uint16_t)roundi(s * 10);
  g_val[SRC_W] = (uint16_t)roundi(w * 10);
  g_val[SRC_TS] = (uint16_t)roundi(ts * 1000);
  g_val[SRC_TW] = (uint16_t)roundi(tw * 1000);

  const int si = roundi(s), wi = roundi(w);
  snprintf(g_tok[1], TOKEN_LEN, "%d", si);
  snprintf(g_tok[2], TOKEN_LEN, "%d", wi);
  snprintf(g_tok[3], TOKEN_LEN, "%d", 100 - si < 0 ? 0 : 100 - si);
  if (clock) hhmm(g_tok[4], nowMin, clockFmt); else snprintf(g_tok[4], TOKEN_LEN, "--:--");
  if (clock) dayName(g_tok[5], dow); else g_tok[5][0] = 0;
  if (clock && sr >= 0) hhmm(g_tok[6], (nowMin + sr) % 1440, clockFmt); else snprintf(g_tok[6], TOKEN_LEN, "--:--");
  if (clock && wr >= 0) {
    const long tot = (long)nowMin + wr;
    hhmm(g_tok[12], (int)(tot % 1440), clockFmt);
    if (tot >= 1440) {                            // "Thu 16:19": the day, a space, the time
      dayName(g_tok[7], dow + (int)(tot / 1440));
      g_tok[7][3] = ' ';
      hhmm(g_tok[7] + 4, (int)(tot % 1440), clockFmt, TOKEN_LEN - 4);
    } else {
      memcpy(g_tok[7], g_tok[12], TOKEN_LEN);
    }
  } else {
    snprintf(g_tok[7], TOKEN_LEN, "--:--");
    snprintf(g_tok[12], TOKEN_LEN, "--:--");
  }
  span(g_tok[8], sr);
  span(g_tok[9], wr);
  if (clock && left >= 0) { g_tok[10][0] = '~'; hhmm(g_tok[10] + 1, (nowMin + left) % 1440, clockFmt, TOKEN_LEN - 1); }
  else snprintf(g_tok[10], TOKEN_LEN, "~--:--");
  float tt = ts < 0.05f ? 0.05f : ts;
  int end = roundi(s / tt);
  snprintf(g_tok[11], TOKEN_LEN, "%d", end > 99 ? 99 : end);

  // FNV-1a over everything a paint shows
  uint32_t hsh = 2166136261u;
  const int32_t v[] = {b, si, wi, sr, wr, clock ? nowMin : -1, dow, clockFmt, end, left < sr ? left : -2};
  for (size_t i = 0; i < sizeof(v) / sizeof(v[0]); i++) hsh = (hsh ^ (uint32_t)v[i]) * 16777619u;
  return hsh;
}

// ---- resolving one op ----------------------------------------------------------------
static void readOp(uint16_t i, Op& o) { memcpy_P(&o, g_base + g_h.opsOff + (uint32_t)i * sizeof(Op), sizeof(Op)); }
static uint16_t color(uint8_t i) { return pgm_read_word(g_base + g_h.paletteOff + (uint32_t)i * 2); }

static int bindPx(uint8_t src, int16_t span) {
  if (src == SRC_NONE || src == SRC_FIXED) return 0;
  const int32_t v = (int32_t)span * g_val[src];
  return (int)((v + (v < 0 ? -500 : 500)) / 1000);
}

// Is the op on screen at all (state + gate), ignoring its animation track?
static bool shown(const Op& o) {
  if ((g_bits & o.need) != o.need || (g_bits & o.notMask)) return false;
  if (o.gateSrc) {
    const uint16_t v = g_val[o.gateSrc];
    if (v < o.gateLo || v >= o.gateHi) return false;
  }
  return true;
}

static uint8_t stepAt(const Op& o, uint32_t now, Step& s) {
  TrackDesc t;
  memcpy_P(&t, g_base + g_h.tracksOff + (uint32_t)o.track * sizeof(TrackDesc), sizeof(t));
  const uint8_t k = (uint8_t)(((now + o.phase) / t.stepMs) % t.steps);
  memcpy_P(&s, g_base + t.stepsOff + (uint32_t)k * sizeof(Step), sizeof(s));
  return k;
}

static Step stepNumber(const Op& o, uint8_t k) {
  TrackDesc t;
  Step s;
  memcpy_P(&t, g_base + g_h.tracksOff + (uint32_t)o.track * sizeof(TrackDesc), sizeof(t));
  memcpy_P(&s, g_base + t.stepsOff + (uint32_t)(k % t.steps) * sizeof(Step), sizeof(s));
  return s;
}

// An op placed on screen: its kind's geometry with bindings and the step applied.
struct Placed {
  int x0, y0, x1, y1;              // rect: [x0,x1) x [y0,y1); line/tri: points
  int x2, y2;                      // tri: third point
  uint16_t color;
  bool on;
};

static const ClawdFont* opFont(const Op& o) { return &g_fonts[o.a]; }

// Expand the op's template into out (tokens, upper case); returns its length.
static int expand(const Op& o, char* out, int n) {
  const uint8_t* p = g_base + g_h.stringsOff + (uint16_t)o.w;
  int len = 0;
  for (uint8_t c; len < n - 1 && (c = pgm_read_byte(p++));) {
    if (c < TOKEN_COUNT) {
      for (const char* t = g_tok[c]; *t && len < n - 1; t++) out[len++] = *t;
    } else {
      out[len++] = (char)c;
    }
  }
  out[len] = 0;
  if (o.flags & F_UPPER)
    for (int i = 0; i < len; i++) if (out[i] >= 'a' && out[i] <= 'z') out[i] = (char)(out[i] - 32);
  return len;
}

static int textX(const Op& o, int x, int w) {
  if (o.b == 1) x -= w / 2;
  else if (o.b == 2) x -= w;
  if (o.flags & F_CLAMP) {
    if (x + w > CLAWD_W - 4) x = CLAWD_W - 4 - w;
    if (x < 4) x = 4;
  }
  return x;
}

static Placed place(const Op& o, const Step* st) {
  Placed p;
  const int sdx = st ? st->dx : 0, sdy = st ? st->dy : 0;
  const int bdx = bindPx(o.dxSrc, o.dxSpan);
  p.color = color(st && st->color != 0xFF ? st->color : o.color);
  p.on = !st || st->vis;
  p.x2 = p.y2 = 0;
  switch (o.kind) {
    case K_RECT: {
      p.x0 = o.x + bdx + sdx;
      if (o.x1Src == SRC_FIXED) p.x1 = o.x + o.w + sdx;
      else if (o.x1Src) p.x1 = o.x + o.w + bindPx(o.x1Src, o.x1Span) + sdx;
      else p.x1 = p.x0 + o.w;
      if (p.x1 - p.x0 < o.c) {
        if (p.x1 <= p.x0 && !(o.flags & F_ZERO_OK)) p.on = false;
        p.x1 = p.x0 + o.c;
      }
      if (p.x1 <= p.x0) p.on = false;
      p.y0 = o.y + sdy;
      p.y1 = p.y0 + o.h;
      break;
    }
    case K_LINE:
      p.x0 = o.x + bdx + sdx;
      p.y0 = o.y + sdy;
      p.x1 = o.w + (o.x1Src == SRC_FIXED ? 0 : o.x1Src ? bindPx(o.x1Src, o.x1Span) : bdx) + sdx;
      p.y1 = o.h + sdy;
      break;
    case K_TRI:
      p.x0 = o.x + bdx + sdx; p.y0 = o.y + sdy;
      p.x1 = o.w + bdx + sdx; p.y1 = o.h + sdy;
      p.x2 = o.x3 + bdx + sdx; p.y2 = o.y3 + sdy;
      break;
    case K_MASK: {
      MaskDesc m;
      memcpy_P(&m, g_base + g_h.masksOff + (uint32_t)o.a * sizeof(MaskDesc), sizeof(m));
      p.x0 = o.x + bdx + sdx; p.y0 = o.y + sdy;
      p.x1 = p.x0 + m.w * o.b; p.y1 = p.y0 + m.h * o.b;
      break;
    }
    default: {                     // K_TEXT: the line box (x from the expanded text)
      char buf[64];
      expand(o, buf, sizeof(buf));
      const ClawdFont* f = opFont(o);
      const int w = clawdTextWidth(*f, buf);
      p.x0 = textX(o, o.x + bdx + sdx, w);
      p.x1 = p.x0 + w;
      p.y0 = o.y + sdy;
      p.y1 = p.y0 + f->lineHeight;
      break;
    }
  }
  return p;
}

// Screen box an op may paint, for dirty tracking (generous for text and lines).
static void bounds(const Op& o, const Placed& p, int& x0, int& y0, int& x1, int& y1) {
  switch (o.kind) {
    case K_LINE:
      x0 = p.x0 < p.x1 ? p.x0 : p.x1; x1 = (p.x0 > p.x1 ? p.x0 : p.x1) + 1;
      y0 = p.y0 < p.y1 ? p.y0 : p.y1; y1 = (p.y0 > p.y1 ? p.y0 : p.y1) + 1;
      return;
    case K_TRI: {
      x0 = p.x0; x1 = p.x0; y0 = p.y0; y1 = p.y0;
      const int xs[2] = {p.x1, p.x2}, ys[2] = {p.y1, p.y2};
      for (int i = 0; i < 2; i++) {
        if (xs[i] < x0) x0 = xs[i];
        if (xs[i] > x1) x1 = xs[i];
        if (ys[i] < y0) y0 = ys[i];
        if (ys[i] > y1) y1 = ys[i];
      }
      x1++; y1++;
      return;
    }
    case K_TEXT:
      x0 = p.x0 - 2; x1 = p.x1 + 4; y0 = p.y0 - 4; y1 = p.y1 + 8;
      return;
    default:
      x0 = p.x0; y0 = p.y0; x1 = p.x1; y1 = p.y1;
  }
}

// ---- rasterising into the band ---------------------------------------------------------
struct Band {
  uint16_t* buf;
  int rx, rw;          // region columns
  int y0, y1;          // band rows [y0, y1)
};

static inline uint16_t* px(const Band& b, int x, int y) { return b.buf + (y - b.y0) * b.rw + (x - b.rx); }

static bool patternOn(const Op& o, int x, int y) {
  const uint8_t kind = (uint8_t)(o.y3 >> 8), on = (uint8_t)(o.y3 & 0xFF), mod = o.b;
  switch (kind) {
    case P_DIAG: return (x + y + o.x3) % mod == 0;
    case P_COLS: return (x + o.x3) % mod < on;
    case P_ROWS: return (y + o.x3) % mod < on;
    default: return true;
  }
}

static void drawRect(const Op& o, const Placed& p, const Band& b) {
  const int ya = p.y0 > b.y0 ? p.y0 : b.y0, yb = p.y1 < b.y1 ? p.y1 : b.y1;
  const int xa = p.x0 > b.rx ? p.x0 : b.rx, xb = p.x1 < b.rx + b.rw ? p.x1 : b.rx + b.rw;
  if (ya >= yb || xa >= xb) return;
  const bool pattern = (o.y3 >> 8) != P_SOLID;
  int r = o.a;
  const int lim = ((p.x1 - p.x0) < (p.y1 - p.y0) ? (p.x1 - p.x0) : (p.y1 - p.y0)) / 2;
  if (r > lim) r = lim;
  if (pattern || r <= 0) {
    for (int y = ya; y < yb; y++) {
      uint16_t* d = px(b, xa, y);
      for (int x = xa; x < xb; x++, d++)
        if (!pattern || patternOn(o, x, y)) *d = p.color;
    }
    return;
  }
  ClawdShape s = {(int16_t)p.x0, (int16_t)p.y0, (int16_t)(p.x1 - p.x0), (int16_t)(p.y1 - p.y0), (int16_t)r, p.color};
  for (int y = ya; y < yb; y++) {
    const bool corner = y < p.y0 + r || y >= p.y1 - r;
    uint16_t* d = px(b, xa, y);
    for (int x = xa; x < xb; x++, d++) {
      if (!corner || (x >= p.x0 + r && x < p.x1 - r)) { *d = p.color; continue; }
      const uint8_t a = clawdShapeCoverage(s, x, y);
      if (a) *d = clawdBlend(p.color, *d, a);
    }
  }
}

static void hspan(const Band& b, int y, int xa, int xb, uint16_t c) {   // [xa, xb] inclusive
  if (y < b.y0 || y >= b.y1) return;
  if (xa > xb) { int t = xa; xa = xb; xb = t; }
  if (xa < b.rx) xa = b.rx;
  if (xb >= b.rx + b.rw) xb = b.rx + b.rw - 1;
  if (xa > xb) return;
  uint16_t* d = px(b, xa, y);
  for (; xa <= xb; xa++) *d++ = c;
}

static void drawLine(const Placed& p, const Band& b) {
  int x0 = p.x0, y0 = p.y0, x1 = p.x1, y1 = p.y1;
  if (y0 == y1) { hspan(b, y0, x0, x1, p.color); return; }
  if (y0 > y1) { int t = y0; y0 = y1; y1 = t; t = x0; x0 = x1; x1 = t; }
  const int ya = y0 > b.y0 ? y0 : b.y0, yb = y1 < b.y1 - 1 ? y1 : b.y1 - 1;
  const float k = (float)(x1 - x0) / (float)(y1 - y0);
  for (int y = ya; y <= yb; y++) {
    const float xa = x0 + k * (y - y0);
    const float xb = y < y1 ? x0 + k * (y + 1 - y0) : xa;
    int a = (int)(xa + 0.5f), c = (int)(xb + 0.5f);
    if (y < y1 && c != a) c += (c > a) ? -1 : 1;     // the next row starts there
    hspan(b, y, a, c, p.color);
  }
}

static void drawTri(const Placed& p, const Band& b) {
  const float xs[3] = {(float)p.x0, (float)p.x1, (float)p.x2}, ys[3] = {(float)p.y0, (float)p.y1, (float)p.y2};
  for (int y = b.y0; y < b.y1; y++) {
    const float yc = y + 0.5f;
    float lo = 1e9f, hi = -1e9f;
    for (int e = 0; e < 3; e++) {
      const int f = (e + 1) % 3;
      const float ya = ys[e], yb = ys[f];
      if ((yc < ya) == (yc < yb)) continue;        // edge does not cross this row
      const float x = xs[e] + (xs[f] - xs[e]) * (yc - ya) / (yb - ya);
      if (x < lo) lo = x;
      if (x > hi) hi = x;
    }
    if (lo > hi) continue;
    const int a = (int)(lo + 0.5f), c = (int)(hi - 0.5f);
    if (a <= c) hspan(b, y, a, c, p.color);
  }
}

static void drawMask(const Op& o, const Placed& p, const Band& b) {
  MaskDesc m;
  memcpy_P(&m, g_base + g_h.masksOff + (uint32_t)o.a * sizeof(MaskDesc), sizeof(m));
  const int sc = o.b, stride = (m.w + 7) / 8;
  const int ya = p.y0 > b.y0 ? p.y0 : b.y0, yb = p.y1 < b.y1 ? p.y1 : b.y1;
  const int xa = p.x0 > b.rx ? p.x0 : b.rx, xb = p.x1 < b.rx + b.rw ? p.x1 : b.rx + b.rw;
  uint8_t row[32];
  for (int y = ya; y < yb; y++) {
    const int my = (y - p.y0) / sc;
    const int n = stride < (int)sizeof(row) ? stride : (int)sizeof(row);
    memcpy_P(row, g_base + m.bitsOff + (uint32_t)my * stride, n);
    uint16_t* d = px(b, xa, y);
    for (int x = xa; x < xb; x++, d++) {
      const int mx = (x - p.x0) / sc;
      if (mx >> 3 < n && (row[mx >> 3] & (0x80 >> (mx & 7)))) *d = p.color;
    }
  }
}

static void drawText(const Op& o, const Placed& p, const Band& b) {
  if (p.y0 >= b.y1 || p.y1 + 8 <= b.y0) return;
  char buf[64];
  expand(o, buf, sizeof(buf));
  ClawdText t = {opFont(o), buf, (int16_t)p.x0, (int16_t)p.y0, p.color};
  clawdBlendText(t, b.rx, b.rw, b.y0, b.y1);
}

// Compose and push region (x, y, w, h) from every op, in order.
static void renderRegion(int x, int y, int w, int h, uint32_t now) {
  ClawdSurface* surf = clawdSurface();
  if (!surf || !g_base) return;
  if (x < 0) { w += x; x = 0; }
  if (y < 0) { h += y; y = 0; }
  if (x + w > CLAWD_W) w = CLAWD_W - x;
  if (y + h > CLAWD_H) h = CLAWD_H - y;
  if (w <= 0 || h <= 0) return;
  const int rows = CLAWD_SCRATCH_PX / w;
  const uint16_t bg = g_h.bg;
  for (int by = y; by < y + h; by += rows) {
    Band b = {clawdScratch(), x, w, by, by + rows < y + h ? by + rows : y + h};
    for (int i = 0; i < w * (b.y1 - b.y0); i++) b.buf[i] = bg;
    for (uint16_t i = 0; i < g_h.opCount; i++) {
      Op o;
      readOp(i, o);
      if (!shown(o)) continue;
      // cheap reject on the op's own rows before resolving it
      if (o.track == 0xFF) {
        if (o.kind == K_RECT && (o.y >= b.y1 || o.y + o.h <= b.y0)) continue;
        if (o.kind == K_TEXT && (o.y - 4 >= b.y1 || o.y + opFont(o)->lineHeight + 8 <= b.y0)) continue;
      }
      Step st;
      const bool anim = o.track != 0xFF;
      if (anim) stepAt(o, now, st);
      const Placed p = place(o, anim ? &st : nullptr);
      if (!p.on) continue;
      switch (o.kind) {
        case K_RECT: drawRect(o, p, b); break;
        case K_LINE: drawLine(p, b); break;
        case K_TRI:  drawTri(p, b); break;
        case K_MASK: drawMask(o, p, b); break;
        case K_TEXT: drawText(o, p, b); break;
      }
    }
    surf->push(x, by, w, b.y1 - b.y0, b.buf);
  }
}

void stylePaint(uint32_t now) {
  if (!g_base) return;
  const uint32_t t0 = clawdMicros();
  styleDrawing(true);
  renderRegion(0, 0, CLAWD_W, CLAWD_H, now);
  styleDrawing(false);
  g_paintUs = clawdMicros() - t0;
  if (g_paintUs > g_paintMaxUs) g_paintMaxUs = g_paintUs;
  uint16_t k = 0;
  for (uint16_t i = 0; i < g_h.opCount && k < STYLE_MAX_ANIMATED; i++) {
    Op o;
    readOp(i, o);
    if (o.track == 0xFF) continue;
    Step st;
    g_last[k++] = stepAt(o, now, st);
  }
}

// ---- animation -----------------------------------------------------------------------
struct Rect { int x0, y0, x1, y1; };
#define DIRTY_MAX 6

static void addDirty(Rect* list, int& n, Rect r) {
  if (r.x0 < 0) r.x0 = 0;
  if (r.y0 < 0) r.y0 = 0;
  if (r.x1 > CLAWD_W) r.x1 = CLAWD_W;
  if (r.y1 > CLAWD_H) r.y1 = CLAWD_H;
  if (r.x0 >= r.x1 || r.y0 >= r.y1) return;
  for (bool merged = true; merged;) {            // fold in everything it touches
    merged = false;
    for (int i = 0; i < n; i++) {
      const Rect& q = list[i];
      if (q.x0 <= r.x1 && r.x0 <= q.x1 && q.y0 <= r.y1 && r.y0 <= q.y1) {
        r = {r.x0 < q.x0 ? r.x0 : q.x0, r.y0 < q.y0 ? r.y0 : q.y0, r.x1 > q.x1 ? r.x1 : q.x1, r.y1 > q.y1 ? r.y1 : q.y1};
        list[i] = list[--n];
        merged = true;
        break;
      }
    }
  }
  if (n < DIRTY_MAX) { list[n++] = r; return; }
  Rect& q = list[n - 1];                          // full: grow the last one
  q = {r.x0 < q.x0 ? r.x0 : q.x0, r.y0 < q.y0 ? r.y0 : q.y0, r.x1 > q.x1 ? r.x1 : q.x1, r.y1 > q.y1 ? r.y1 : q.y1};
}

void styleAnimate(uint32_t now) {
  if (!g_base) return;
  Rect dirty[DIRTY_MAX];
  int n = 0;
  uint16_t k = 0;
  for (uint16_t i = 0; i < g_h.opCount && k < STYLE_MAX_ANIMATED; i++) {
    Op o;
    readOp(i, o);
    if (o.track == 0xFF) continue;
    const uint16_t slot = k++;
    if (!shown(o)) continue;
    Step now_;
    const uint8_t step = stepAt(o, now, now_);
    const uint8_t was = g_last[slot];
    if (step == was) continue;
    g_last[slot] = step;
    const Step old = stepNumber(o, was == 0xFF ? step : was);
    if (was != 0xFF && old.dx == now_.dx && old.dy == now_.dy && old.vis == now_.vis && old.color == now_.color) continue;
    const Step* both[2] = {&old, &now_};
    for (const Step* s : both) {
      const Placed p = place(o, s);
      Rect r;
      bounds(o, p, r.x0, r.y0, r.x1, r.y1);
      addDirty(dirty, n, r);
    }
  }
  if (!n) return;
  const uint32_t t0 = clawdMicros();
  styleDrawing(true);
  for (int i = 0; i < n; i++) renderRegion(dirty[i].x0, dirty[i].y0, dirty[i].x1 - dirty[i].x0, dirty[i].y1 - dirty[i].y0, now);
  styleDrawing(false);
  g_frameUs = clawdMicros() - t0;
  if (g_frameUs > g_frameMaxUs) g_frameMaxUs = g_frameUs;
}
