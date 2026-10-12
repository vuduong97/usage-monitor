// clawd_render: draw the clock's usage screen on this computer, with the clock's own
// code. The settings window runs it to show what the clock will look like.
//
//   clawd_render [--style file.pak] [--pak clawd.pak] [--ms 3000] [--clip] < payload.json > out
//
// --style    draw the usage screen from this style pack (firmware/styles/*.pak), as
//            the clock does once it is installed; without it, the classic screen
//
// stdin  the usage payload the app POSTs to the clock's /api/usage (flat JSON:
//        s sr w wr acct tp rd t tf ok), or nothing at all: no data yet, which
//        draws the waiting screen
// stdout 240x240 RGB, 3 bytes a pixel, no header (172,800 bytes)
//        --clip: one loop of the screen as it animates (see writeClip below):
//          "CLP1" u16 frames, then per frame u16 delay_ms, u16 x y w h, w*h*3 RGB
//          (little-endian). Frame 0 is the full screen; each later one is the
//          rectangle that changed, painted over the one before.
// exit   0 ok, 1 bad arguments, 2 the clock would reject the payload
//
// Built by tools/build_preview.py together with the firmware's src/features/clawd
// (ClawdDraw, ClawdAnim, ClawdRate, ClawdUi, ClawdPack, DeskPack, StylePack), so the pixels are
// the firmware's. Only the payload -> ClawdData step below is written out here; it
// copies forwardToClawd() in the firmware's UsageClient.cpp, field for field.
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <vector>
#include "ClawdAnim.h"
#include "ClawdDraw.h"
#include "ClawdUi.h"
#include "StylePack.h"

static uint32_t g_now = 1000;
uint32_t clawdNow() { return g_now; }
uint32_t clawdMicros() { return g_now * 1000u; }
uint32_t clawdRandom(uint32_t n) { return (uint32_t)rand() % n; }   // srand(1): the same frame every run

static uint16_t fb[CLAWD_W * CLAWD_H];

class MemSurface : public ClawdSurface {
 public:
  void fill(int x, int y, int w, int h, uint16_t c) override {
    for (int r = y; r < y + h; r++)
      for (int k = x; k < x + w; k++)
        if (r >= 0 && r < CLAWD_H && k >= 0 && k < CLAWD_W) fb[r * CLAWD_W + k] = c;
  }
  void push(int x, int y, int w, int h, const uint16_t* px) override {
    for (int r = 0; r < h; r++)
      for (int k = 0; k < w; k++) {
        int X = x + k, Y = y + r;
        if (X >= 0 && X < CLAWD_W && Y >= 0 && Y < CLAWD_H) fb[Y * CLAWD_W + X] = px[r * w + k];
      }
  }
};

// ---- a flat JSON object: {"key": number | "text" | true | false | null, ...} ----------
struct Val {
  bool present = false, isNum = false, isBool = false, isStr = false;
  double num = 0;
  bool b = false;
  std::string str;
};
struct Doc {
  std::string keys[16];
  Val vals[16];
  int n = 0;
  const Val* get(const char* k) const {
    for (int i = 0; i < n; i++) if (keys[i] == k) return &vals[i];
    return nullptr;
  }
};

static void skipWs(const char*& p) { while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') p++; }

static bool readString(const char*& p, std::string& out) {
  if (*p != '"') return false;
  p++;
  while (*p && *p != '"') {
    if (*p == '\\' && p[1]) p++;       // payload text is ASCII; an escape keeps its next character
    out += *p++;
  }
  if (*p != '"') return false;
  p++;
  return true;
}

static bool parseFlat(const char* p, Doc& d) {
  skipWs(p);
  if (*p != '{') return false;
  p++;
  skipWs(p);
  if (*p == '}') return true;
  for (;;) {
    skipWs(p);
    if (d.n >= 16) return false;
    std::string key;
    if (!readString(p, key)) return false;
    skipWs(p);
    if (*p++ != ':') return false;
    skipWs(p);
    Val v;
    v.present = true;
    if (*p == '"') {
      v.isStr = true;
      if (!readString(p, v.str)) return false;
    } else if (!strncmp(p, "true", 4)) { v.isBool = v.b = true; p += 4; }
    else if (!strncmp(p, "false", 5)) { v.isBool = true; v.b = false; p += 5; }
    else if (!strncmp(p, "null", 4)) { v.present = false; p += 4; }
    else {
      char* end = nullptr;
      v.num = strtod(p, &end);
      if (end == p) return false;
      v.isNum = true;
      p = end;
    }
    d.keys[d.n] = key;
    d.vals[d.n++] = v;
    skipWs(p);
    if (*p == ',') { p++; continue; }
    return *p == '}';
  }
}

// double, not float: the wall-clock epoch ("t") is ~1.7e9 and a float loses its seconds.
static double numOr(const Doc& d, const char* k, double dflt) {
  const Val* v = d.get(k);
  return v && v->isNum ? v->num : dflt;
}
static double clampd(double v, double lo, double hi) { return v < lo ? lo : v > hi ? hi : v; }

// UsageClient.cpp: applyUsageDoc() + forwardToClawd(). Returns false when the clock
// would reject the payload (no numeric "s" and not an explicit {"ok": false} beat).
static bool toClawdData(const Doc& d, ClawdData& c) {
  memset(&c, 0, sizeof(c));
  const Val* ok = d.get("ok");
  const bool okFlag = !(ok && ok->isBool && !ok->b);
  const Val* s = d.get("s");
  if (okFlag && !(s && s->isNum)) return false;
  c.ok = okFlag;
  c.sessionPct = (float)clampd(numOr(d, "s", 0), 0, 100);
  c.sessionResetMin = (int)numOr(d, "sr", -1);
  c.weeklyPct = (float)clampd(numOr(d, "w", 0), 0, 100);
  c.weeklyResetMin = (int)numOr(d, "wr", -1);
  const Val* acct = d.get("acct");
  c.enterprise = acct && acct->isStr && acct->str == "ent";
  c.timePct = (int)clampd(numOr(d, "tp", 0), 0, 100);
  const Val* rd = d.get("rd");
  if (rd && rd->isStr) strncpy(c.resetDate, rd->str.c_str(), sizeof(c.resetDate) - 1);
  c.clockEpoch = (long)numOr(d, "t", 0);
  c.clockFmt = (int)numOr(d, "tf", 24);
  return true;
}

#define STEP_MS 10

static void step() {
  g_now += STEP_MS;
  clawdUiService(CLAWD_SCREEN_AUTO, -1, CLAWD_LAYOUT_CLASSIC);
}

static void run(uint32_t ms) {
  for (uint32_t t = 0; t < ms; t += STEP_MS) step();
}

static void toRgb(uint16_t c, uint8_t* o) {        // RGB565 -> RGB888, the preview's expansion
  o[0] = (uint8_t)(((c >> 11) & 31) * 255 / 31);
  o[1] = (uint8_t)(((c >> 5) & 63) * 255 / 63);
  o[2] = (uint8_t)((c & 31) * 255 / 31);
}

// ---- --clip: one seamless loop of the animated screen -------------------------------
// It starts on the blank panel, so the first paint counts as Clawd moving. Clawd's box
// (clawdUiSpriteBox) decides the loop: it starts the moment Clawd comes to rest after moving (the corner mascot back on its still pose after an act, the pixel
// walker standing at one end) and ends when Clawd comes to rest in that same picture
// again (one rest + one act; one round trip). The status line is not part of it, so its
// spinner may skip a beat where the loop wraps. Without a box (the splash, a style pack:
// its tracks loop on their own periods) or when no loop shows up in time, it is a fixed
// stretch.
#define REST_MS     1000      // still this long = at rest (shorter than every rest on the clock)
#define SEARCH_MS   50000     // give up looking for the loop after this (a round trip of the
                              // walker takes ~36 s; the caller keeps the title clock off a
                              // minute change by sending a payload time on the minute)
#define FALLBACK_MS 6000
static uint32_t g_fixedMs = FALLBACK_MS;   // the fixed stretch (a style pack: its loop)

struct Rect { int x, y, w, h; };
struct Change { uint32_t t; Rect r; std::vector<uint16_t> px; };

static Rect diffRect(const uint16_t* a, const uint16_t* b, Rect in) {
  int x0 = in.x + in.w, y0 = in.y + in.h, x1 = -1, y1 = -1;
  for (int y = in.y; y < in.y + in.h; y++) {
    const uint16_t* pa = a + y * CLAWD_W;
    const uint16_t* pb = b + y * CLAWD_W;
    if (!memcmp(pa + in.x, pb + in.x, in.w * 2)) continue;
    for (int x = in.x; x < in.x + in.w; x++)
      if (pa[x] != pb[x]) {
        if (x < x0) x0 = x;
        if (x > x1) x1 = x;
        if (y < y0) y0 = y;
        y1 = y;
      }
  }
  return x1 < 0 ? Rect{0, 0, 0, 0} : Rect{x0, y0, x1 - x0 + 1, y1 - y0 + 1};
}

static std::vector<uint16_t> grab(const uint16_t* src, Rect r) {
  std::vector<uint16_t> v((size_t)r.w * r.h);
  for (int y = 0; y < r.h; y++) memcpy(&v[(size_t)y * r.w], src + (r.y + y) * CLAWD_W + r.x, r.w * 2);
  return v;
}

static void put16(uint16_t v) { fputc(v & 255, stdout); fputc(v >> 8, stdout); }

static bool writeFrame(uint32_t delay, Rect r, const std::vector<uint16_t>& px) {
  put16((uint16_t)(delay > 65535 ? 65535 : delay));
  put16((uint16_t)r.x); put16((uint16_t)r.y); put16((uint16_t)r.w); put16((uint16_t)r.h);
  std::vector<uint8_t> rgb(px.size() * 3);
  for (size_t i = 0; i < px.size(); i++) toRgb(px[i], &rgb[i * 3]);
  return fwrite(rgb.data(), 1, rgb.size(), stdout) == rgb.size();
}

// key: the full screen at `t0`; changes after it until `t1`.
static int writeClip(const std::vector<uint16_t>& key, uint32_t t0, const std::vector<Change>& ch, uint32_t t1) {
  std::vector<const Change*> use;
  for (const Change& c : ch)
    if (c.t > t0 && c.t < t1) use.push_back(&c);
  if (use.size() > 65534) return 1;
  fwrite("CLP1", 1, 4, stdout);
  put16((uint16_t)(use.size() + 1));
  uint32_t prev = t0;
  Rect full = {0, 0, CLAWD_W, CLAWD_H};
  const std::vector<uint16_t>* px = &key;
  Rect r = full;
  for (const Change* c : use) {
    if (!writeFrame(c->t - prev, r, *px)) return 1;
    prev = c->t; r = c->r; px = &c->px;
  }
  return writeFrame(t1 - prev, r, *px) ? 0 : 1;
}

static int clip() {
  std::vector<uint16_t> prev(fb, fb + CLAWD_W * CLAWD_H);
  const std::vector<uint16_t> initial(prev);
  std::vector<Change> changes;
  std::vector<uint16_t> keyCand(prev), key, rest;   // screen at the last move of Clawd's box; the loop's start
  const uint32_t begin = g_now;
  uint32_t moved = 0;          // when Clawd's box last changed (0: not yet)
  bool atRest = true;
  uint32_t t0 = 0;
  bool started = false;
  for (;;) {
    step();
    int bx = 0, by = 0, bw = 0, bh = 0;
    const bool box = clawdUiSpriteBox(bx, by, bw, bh);
    const Rect sprite = {bx, by, bw, bh};
    if (g_now - begin >= (box ? SEARCH_MS : g_fixedMs)) break;
    const Rect all = {0, 0, CLAWD_W, CLAWD_H};
    const Rect d = diffRect(fb, prev.data(), all);
    if (d.w) {
      changes.push_back({g_now, d, grab(fb, d)});
      for (int y = d.y; y < d.y + d.h; y++) memcpy(&prev[y * CLAWD_W + d.x], fb + y * CLAWD_W + d.x, d.w * 2);
      if (box && diffRect(fb, keyCand.data(), sprite).w) {     // Clawd moved
        moved = g_now;
        atRest = false;
      }
      if (box && moved == g_now) keyCand = prev;
    }
    if (!box || atRest || !moved || g_now - moved < REST_MS) continue;
    atRest = true;                                            // Clawd has come to rest at `moved`
    std::vector<uint16_t> here = grab(keyCand.data(), sprite);
    if (!started) {
      started = true; t0 = moved; key = keyCand; rest = here;
      std::vector<Change> keep;                              // drop what the key frame already holds
      for (Change& c : changes) if (c.t > t0) keep.push_back(std::move(c));
      changes.swap(keep);
    } else if (here == rest) {
      return writeClip(key, t0, changes, moved);
    }
  }
  // No box, or no loop in time: a fixed stretch from the loop's start, or from where the
  // search began.
  if (started) return writeClip(key, t0, changes, t0 + g_fixedMs < g_now ? t0 + g_fixedMs : g_now);
  return writeClip(initial, begin, changes, begin + g_fixedMs < g_now ? begin + g_fixedMs : g_now);
}

// One loop of a style pack: the least common multiple of its animation tracks'
// periods (header trackCount @59, tracksOff @76; a track is u16 stepMs, u8 steps, pad,
// u32 stepsOff). 0 when nothing moves, UINT32_MAX when the loop runs past `cap`.
static uint32_t styleLoopMs(const std::vector<uint8_t>& p, uint32_t cap) {
  if (p.size() < STYLE_HEADER_BYTES) return 0;
  const uint8_t n = p[59];
  const uint32_t off = p[76] | p[77] << 8 | p[78] << 16 | (uint32_t)p[79] << 24;
  uint64_t loop = 0;
  for (uint8_t i = 0; i < n && off + 8u * i + 3 <= p.size(); i++) {
    const uint8_t* t = &p[off + 8u * i];
    const uint64_t period = (uint64_t)(t[0] | t[1] << 8) * t[2];
    if (!period) continue;
    uint64_t a = loop ? loop : period, b = period;
    while (b) { const uint64_t r = a % b; a = b; b = r; }
    loop = loop ? loop / a * period : period;
    if (loop > cap) return UINT32_MAX;
  }
  return (uint32_t)loop;
}

static bool readAll(const char* path, std::vector<uint8_t>& out) {
  FILE* f = fopen(path, "rb");
  if (!f) return false;
  uint8_t buf[4096];
  size_t got;
  while ((got = fread(buf, 1, sizeof(buf), f)) > 0 && out.size() <= STYLE_MAX_BYTES) out.insert(out.end(), buf, buf + got);
  fclose(f);
  return true;
}

int main(int argc, char** argv) {
  long ms = 3000;
  const char* pak = nullptr;
  const char* style = nullptr;
  bool wantClip = false;
  for (int i = 1; i < argc; i++) {
    if (!strcmp(argv[i], "--clip")) wantClip = true;
    else if (!strcmp(argv[i], "--ms") && i + 1 < argc) ms = atol(argv[++i]);
    else if (!strcmp(argv[i], "--pak") && i + 1 < argc) pak = argv[++i];
    else if (!strcmp(argv[i], "--style") && i + 1 < argc) style = argv[++i];
    else { fprintf(stderr, "usage: clawd_render [--style file.pak] [--pak file] [--ms 3000] [--clip] < payload.json > frame.rgb\n"); return 1; }
  }
  if (ms < 100 || ms > 60000) { fprintf(stderr, "bad --ms\n"); return 1; }
  if (pak) {
#ifdef _WIN32
    _putenv_s("CLAWD_PACK_PATH", pak);
#else
    setenv("CLAWD_PACK_PATH", pak, 1);
#endif
  }

  std::string body;
  char buf[1024];
  size_t got;
  while (body.size() < 4096 && (got = fread(buf, 1, sizeof(buf), stdin)) > 0) body.append(buf, got);
  Doc doc;
  ClawdData data;
  bool have = false;
  if (body.find_first_not_of(" \t\r\n") != std::string::npos) {
    if (!parseFlat(body.c_str(), doc) || !toClawdData(doc, data)) { fprintf(stderr, "payload rejected\n"); return 2; }
    have = true;
  }

  MemSurface surf;
  clawdSetSurface(&surf);
  clawdUiBegin();
  static std::vector<uint8_t> stylePack;     // read in place for as long as it draws
  if (style) {
    if (!readAll(style, stylePack)) { fprintf(stderr, "cannot read %s\n", style); return 1; }
    const char* why = styleValidate(stylePack.data(), (uint32_t)stylePack.size());
    if (why || !styleAttach(stylePack.data(), (uint32_t)stylePack.size())) {
      fprintf(stderr, "style pack rejected: %s\n", why ? why : "attach");
      return 1;
    }
    // A clip of a pack is its loop, at least 2 s (a still pack: one 1 s frame), starting
    // on the painted screen: the tracks run on absolute time, so any start is seamless.
    const uint32_t loop = styleLoopMs(stylePack, SEARCH_MS);
    g_fixedMs = !loop ? 1000 : loop == UINT32_MAX ? FALLBACK_MS : loop < 2000 ? loop * ((2000 + loop - 1) / loop) : loop;
  }
  if (have) clawdUiPayload(data);
  // A classic clip starts on the blank panel (its first paint counts as Clawd moving);
  // a style clip after the first full paint.
  if (!wantClip) run((uint32_t)ms);
  else if (style) run(600);

#ifdef _WIN32
  fflush(stdout);
  _setmode(_fileno(stdout), _O_BINARY);
#endif
  if (wantClip) return clip() || fflush(stdout) ? 1 : 0;
  static uint8_t out[CLAWD_W * CLAWD_H * 3];
  for (int i = 0; i < CLAWD_W * CLAWD_H; i++) toRgb(fb[i], &out[i * 3]);
  if (fwrite(out, 1, sizeof(out), stdout) != sizeof(out)) return 1;
  return 0;
}
