// clawd_render: draw the clock's usage screen on this computer, with the clock's own
// code. The settings window runs it to show what the clock will look like.
//
//   clawd_render --layout N [--pak clawd.pak] [--ms 3000] < payload.json > frame.rgb
//
// stdin  the usage payload the app POSTs to the clock's /api/usage (flat JSON:
//        s sr w wr acct tp rd t tf ok), or nothing at all: no data yet, which
//        draws the waiting screen
// stdout 240x240 RGB, 3 bytes a pixel, no header (172,800 bytes)
// exit   0 ok, 1 bad arguments, 2 the clock would reject the payload
//
// Built by tools/build_preview.py together with the firmware's src/features/clawd
// (ClawdDraw, ClawdAnim, ClawdRate, ClawdUi, ClawdPack, DeskPack), so the pixels are
// the firmware's. Only the payload -> ClawdData step below is written out here; it
// copies forwardToClawd() in the firmware's UsageClient.cpp, field for field.
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include "ClawdAnim.h"
#include "ClawdDraw.h"
#include "ClawdUi.h"

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

static void run(uint32_t ms, uint8_t layout) {
  for (uint32_t t = 0; t < ms; t += 10) {
    g_now += 10;
    clawdUiService(CLAWD_SCREEN_AUTO, -1, layout);
  }
}

int main(int argc, char** argv) {
  int layout = 0;
  long ms = 3000;
  const char* pak = nullptr;
  for (int i = 1; i < argc; i++) {
    if (!strcmp(argv[i], "--layout") && i + 1 < argc) layout = atoi(argv[++i]);
    else if (!strcmp(argv[i], "--ms") && i + 1 < argc) ms = atol(argv[++i]);
    else if (!strcmp(argv[i], "--pak") && i + 1 < argc) pak = argv[++i];
    else { fprintf(stderr, "usage: clawd_render --layout N [--pak file] [--ms 3000] < payload.json > frame.rgb\n"); return 1; }
  }
  if (layout < 0 || layout > 255 || ms < 100 || ms > 60000) { fprintf(stderr, "bad --layout or --ms\n"); return 1; }
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
  if (have) clawdUiPayload(data);
  run((uint32_t)ms, (uint8_t)layout);

#ifdef _WIN32
  fflush(stdout);
  _setmode(_fileno(stdout), _O_BINARY);
#endif
  static uint8_t out[CLAWD_W * CLAWD_H * 3];
  for (int i = 0; i < CLAWD_W * CLAWD_H; i++) {      // RGB565 -> RGB888, the preview's expansion
    const uint16_t c = fb[i];
    out[i * 3 + 0] = (uint8_t)(((c >> 11) & 31) * 255 / 31);
    out[i * 3 + 1] = (uint8_t)(((c >> 5) & 63) * 255 / 63);
    out[i * 3 + 2] = (uint8_t)((c & 31) * 255 / 31);
  }
  if (fwrite(out, 1, sizeof(out), stdout) != sizeof(out)) return 1;
  return 0;
}
