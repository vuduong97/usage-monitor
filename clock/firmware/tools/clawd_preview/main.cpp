// Desktop preview of the Clawd screens: runs the exact firmware renderer
// (src/features/clawd) against an in-memory 240x240 panel with a fake clock
// and writes PPM screenshots. Build + run: tools/clawd_preview/run.sh
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "ClawdAnim.h"
#include "ClawdDraw.h"
#include "ClawdUi.h"
#include "StylePack.h"
#include <chrono>
#include <vector>

static uint32_t g_now = 1000;
uint32_t clawdNow() { return g_now; }
uint32_t clawdMicros() { return g_now * 1000u; }
uint32_t clawdRandom(uint32_t n) { return (uint32_t)rand() % n; }   // srand(1): repeatable runs

static uint16_t fb[CLAWD_W * CLAWD_H];
static long g_pushed = 0;

class MemSurface : public ClawdSurface {
 public:
  void fill(int x, int y, int w, int h, uint16_t c) override {
    for (int r = y; r < y + h; r++)
      for (int k = x; k < x + w; k++)
        if (r >= 0 && r < CLAWD_H && k >= 0 && k < CLAWD_W) fb[r * CLAWD_W + k] = c;
    g_pushed += (long)w * h;
  }
  void push(int x, int y, int w, int h, const uint16_t* px) override {
    for (int r = 0; r < h; r++)
      for (int k = 0; k < w; k++) {
        int X = x + k, Y = y + r;
        if (X >= 0 && X < CLAWD_W && Y >= 0 && Y < CLAWD_H) fb[Y * CLAWD_W + X] = px[r * w + k];
      }
    g_pushed += (long)w * h;
  }
};

static void shot(const char* dir, const char* name) {
  char path[512];
  snprintf(path, sizeof(path), "%s/%s.ppm", dir, name);
  FILE* f = fopen(path, "wb");
  if (!f) { perror(path); exit(1); }
  fprintf(f, "P6\n%d %d\n255\n", CLAWD_W, CLAWD_H);
  for (int i = 0; i < CLAWD_W * CLAWD_H; i++) {
    uint16_t c = fb[i];
    uint8_t rgb[3] = {(uint8_t)(((c >> 11) & 31) * 255 / 31), (uint8_t)(((c >> 5) & 63) * 255 / 63),
                      (uint8_t)((c & 31) * 255 / 31)};
    fwrite(rgb, 1, 3, f);
  }
  fclose(f);
  printf("%s\n", path);
}

static uint8_t g_layout = CLAWD_LAYOUT_CLASSIC;   // usage.layout passed to every service

// Advance the fake clock in 10 ms loop() steps.
static void run(uint32_t ms, uint8_t screen, int8_t anim) {
  for (uint32_t t = 0; t < ms; t += 10) {
    g_now += 10;
    clawdUiService(screen, anim, g_layout);
  }
}

static ClawdData base() {
  ClawdData d;
  memset(&d, 0, sizeof(d));
  d.ok = true;
  d.sessionPct = 42;
  d.sessionResetMin = 134;
  d.weeklyPct = 67;
  d.weeklyResetMin = 4000;
  d.clockEpoch = 1760191500;   // 14:05 UTC
  d.clockFmt = 24;
  return d;
}

static ClawdState deskState(const char* const* anims, int n, const char* const* vars, int nv) {
  ClawdState s;
  memset(&s, 0, sizeof(s));
  for (int i = 0; i < n; i++) strncpy(s.anim[s.animCount++], anims[i], sizeof(s.anim[0]) - 1);
  for (int i = 0; i < nv; i++) strncpy(s.var[s.varCount++], vars[i], sizeof(s.var[0]) - 1);
  s.urgent = true;
  s.dim = -1;
  s.ttlSec = 3600;
  return s;
}

// Play until the desk animation on screen changes (or `maxMs` passes); the time it took.
static uint32_t untilChange(uint32_t maxMs) {
  char was[16];
  strncpy(was, clawdUiDeskAnimName(), sizeof(was));
  uint32_t t = 0;
  while (t < maxMs && strcmp(was, clawdUiDeskAnimName()) == 0) { run(10, CLAWD_SCREEN_AUTO, -1); t += 10; }
  return t;
}

#define CHECK(c, ...) do { if (!(c)) { printf("FAIL: " __VA_ARGS__); printf("\n"); bad++; } } while (0)

// Non-background pixels in the splash's bottom band (the IP caption).
static int bandInk() {
  int n = 0;
  for (int y = CLAWD_H - CLAWD_SPLASH_BAND_PX; y < CLAWD_H; y++)
    for (int x = 0; x < CLAWD_W; x++) n += fb[y * CLAWD_W + x] != CLAWD_BG;
  return n;
}

// The waiting splash (auto screen, no fresh data) captions the IP in its bottom
// band, the animations never paint over it, a new IP is redrawn, and a splash
// picked on purpose stays full height. Leaves the view on a stale auto splash.
static int checkAddressBand(const char* dir) {
  int bad = 0;
  clawdUiSetAddress("192.168.1.48");
  run(2000, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "09-splash-auto");
  const int ink = bandInk();
  CHECK(ink > 0, "no IP under the waiting splash");
  run(45000, CLAWD_SCREEN_AUTO, -1);                 // two rotations: other animations
  CHECK(bandInk() == ink, "the splash painted over the IP (%d -> %d px)", ink, bandInk());
  clawdUiSetAddress("10.97.44.122");
  run(100, CLAWD_SCREEN_AUTO, -1);
  CHECK(bandInk() != ink && bandInk() > 0, "a new IP was not redrawn");
  run(2500, CLAWD_SCREEN_SPLASH, -1);
  CHECK(bandInk() == 0, "screen = splash still shows the IP");
  clawdUiSetAddress("");
  run(2500, CLAWD_SCREEN_AUTO, -1);
  CHECK(bandInk() == 0, "an empty IP left ink in the band");
  printf("address band: %s\n", bad ? "FAILED" : "ok");
  return bad;
}

// Variants (`v`): the animation plays CLAWD_VAR_EVERY passes, then one variant at
// random (never the same one twice in a row), then the animation again; without
// `v` it just loops, and a change of `v` alone is taken over.
static int checkVariants() {
  int bad = 0;
  srand(1);
  clawdUiPayload(base());
  static const char* const typing[] = {"typing"};
  static const char* const typingHappy[] = {"typing", "happy"};
  static const char* const vars[] = {"building", "carrying", "typing boss"};

  clawdUiState(deskState(typingHappy, 2, nullptr, 0));      // one pass of typing, then happy
  run(20, CLAWD_SCREEN_AUTO, -1);
  const uint32_t pass = untilChange(60000);
  CHECK(!strcmp(clawdUiDeskAnimName(), "happy"), "typing,happy ends in %s", clawdUiDeskAnimName());
  printf("variants: one typing pass = %u ms\n", (unsigned)pass);

  clawdUiState(deskState(typing, 1, nullptr, 0));           // no v: typing only, like before
  run(20, CLAWD_SCREEN_AUTO, -1);
  CHECK(untilChange(pass * 8) >= pass * 8, "typing without v changed to %s", clawdUiDeskAnimName());

  clawdUiState(deskState(typing, 1, vars, 3));              // same a, new v: taken over
  run(20, CLAWD_SCREEN_AUTO, -1);
  int seen[3] = {0, 0, 0}, last = -1;
  for (int cycle = 0; cycle < 20; cycle++) {
    CHECK(!strcmp(clawdUiDeskAnimName(), "typing"), "cycle %d starts on %s", cycle, clawdUiDeskAnimName());
    const uint32_t primary = untilChange(pass * 8);
    // cycle 0 may start just after the previous pass ended (the switch waits for it)
    // (10 ms loop steps: each pass measures within a step or two)
    if (cycle > 0) CHECK(primary + 50 >= pass * CLAWD_VAR_EVERY && primary <= pass * CLAWD_VAR_EVERY + 50,
                         "cycle %d: typing for %u ms, want %u", cycle, (unsigned)primary,
                         (unsigned)(pass * CLAWD_VAR_EVERY));
    int v = -1;
    for (int i = 0; i < 3; i++) if (!strcmp(clawdUiDeskAnimName(), vars[i])) v = i;
    CHECK(v >= 0, "cycle %d: variant %s is not in v", cycle, clawdUiDeskAnimName());
    if (v < 0) break;
    CHECK(v != last, "cycle %d: %s twice in a row", cycle, vars[v]);
    seen[v]++;
    last = v;
    printf("variants: cycle %2d typing %5u ms -> %s\n", cycle, (unsigned)primary, vars[v]);
    untilChange(30000);                                     // the variant's one pass
  }
  CHECK(seen[0] && seen[1] && seen[2], "variants seen %d/%d/%d", seen[0], seen[1], seen[2]);

  static const char* const idleLook[] = {"idle", "look"};
  ClawdState rep = deskState(idleLook, 2, vars, 3);         // v only mixes into one looping animation
  rep.repeat = true;
  clawdUiState(rep);
  run(20, CLAWD_SCREEN_AUTO, -1);
  for (int i = 0; i < 6; i++) {
    untilChange(30000);
    const char* n = clawdUiDeskAnimName();
    CHECK(!strcmp(n, "idle") || !strcmp(n, "look"), "rep sequence played %s", n);
  }
  printf("variants: %s\n", bad ? "FAILED" : "ok");
  return bad;
}

// ---- style packs ---------------------------------------------------------------------
static std::vector<uint8_t> readFile(const char* path) {
  std::vector<uint8_t> v;
  FILE* f = fopen(path, "rb");
  if (!f) return v;
  uint8_t buf[4096];
  for (size_t n; (n = fread(buf, 1, sizeof(buf), f)) > 0;) v.insert(v.end(), buf, buf + n);
  fclose(f);
  return v;
}

static double hostUs() {
  using namespace std::chrono;
  return (double)duration_cast<nanoseconds>(steady_clock::now().time_since_epoch()).count() / 1000.0;
}

// The four states of the design sheet (design/usage-screen-preview): Monday 14:05.
static ClawdData styleState(int k) {
  ClawdData d = base();
  d.clockEpoch = 1760364300;                        // Mon 14:05 (local wall clock)
  if (k == 1) { d.sessionPct = 88; d.sessionResetMin = 38; d.weeklyPct = 93; d.weeklyResetMin = 540; }
  if (k == 2) { d.sessionPct = 4; d.sessionResetMin = 291; d.weeklyPct = 18; d.weeklyResetMin = 8200; d.clockEpoch = 1760346720; }
  if (k == 0) { d.weeklyResetMin = 3960; }
  return d;
}

// Every pack in $CLAWD_STYLE_DIR: the four states, a stretch of animation (frame
// timing on this machine), the fallbacks (alert, Enterprise -> classic) and the
// guards (a corrupt or future pack is refused).
static int checkStyles(const char* dir) {
  int bad = 0;
  const char* sdir = getenv("CLAWD_STYLE_DIR");
  if (!sdir) { printf("styles: CLAWD_STYLE_DIR not set, skipped\n"); return 0; }
  // the packs: every id in index.json, in its order
  static char ids[64][16];
  int count = 0;
  {
    char path[512];
    snprintf(path, sizeof(path), "%s/index.json", sdir);
    std::vector<uint8_t> js = readFile(path);
    js.push_back(0);
    for (const char* q = (const char*)js.data(); count < 64 && (q = strstr(q, "\"id\""));) {
      q = strchr(q + 4, '"');
      if (!q) break;
      const char* e = strchr(++q, '"');
      if (!e) break;
      snprintf(ids[count++], sizeof(ids[0]), "%.*s", (int)(e - q), q);
      q = e + 1;
    }
    CHECK(count > 0, "%s: no packs", path);
  }
  const char* only = getenv("CLAWD_STYLE_ONLY");   // one pack id: just its shots
  static const char* const STATES[] = {"normal", "hot", "fresh", "stale"};
  static std::vector<uint8_t> packs[64];
  const char* IDS[64];
  for (int p = 0; p < count; p++) IDS[p] = ids[p];
  for (int p = 0; p < count; p++) {
    if (only && *only && strcmp(only, IDS[p])) continue;
    char path[512], n[64];
    snprintf(path, sizeof(path), "%s/%s.pak", sdir, IDS[p]);
    packs[p] = readFile(path);
    std::vector<uint8_t>& pk = packs[p];
    const char* why = styleValidate(pk.data(), (uint32_t)pk.size());
    CHECK(!why, "%s: %s", path, why ? why : "");
    if (why || !styleAttach(pk.data(), (uint32_t)pk.size())) { bad++; continue; }
    double paintUs = 0;            // the slowest state's full paint on this machine (best of 15)
    for (int k = 0; k < 4; k++) {
      if (k < 3) {
        clawdUiPayload(styleState(k));
        run(600, CLAWD_SCREEN_USAGE, -1);
      } else {
        run(CLAWD_DATA_FRESH_MS + 1000, CLAWD_SCREEN_USAGE, -1);   // the data goes stale
      }
      snprintf(n, sizeof(n), "30-%s-%d-%s", IDS[p], k, STATES[k]);
      shot(dir, n);
      double best = 1e18;
      for (int i = 0; i < 15; i++) {
        const double t0 = hostUs();
        stylePaint(g_now);
        const double dt = hostUs() - t0;
        if (dt < best) best = dt;
      }
      if (best > paintUs) paintUs = best;
    }
    // animation: 3 s at 10 ms steps, host time per frame that repainted something
    clawdUiPayload(styleState(1));
    run(600, CLAWD_SCREEN_USAGE, -1);
    double worst = 0, total = 0;
    int frames = 0;
    uint32_t lastFrame = styleLastFrameUs();
    for (int t = 0; t < 300; t++) {
      g_now += 10;
      const long before = g_pushed;
      const double t0 = hostUs();
      clawdUiService(CLAWD_SCREEN_USAGE, -1, g_layout);
      const double dt = hostUs() - t0;
      if (g_pushed != before) { frames++; total += dt; if (dt > worst) worst = dt; }
      if (t % 25 == 0 && p == 1) { snprintf(n, sizeof(n), "31-%s-anim-%02d", IDS[p], t / 25); shot(dir, n); }
      (void)lastFrame;
    }
    const StyleInfo* info = styleInfo();
    // ~clock: a fit of the six original packs' paints measured on the clock (80 MHz,
    // 2026-10-12) against this preview's numbers: 43 ms + 1.54 ms per host us
    // (within ~12 ms for all six). A rough guide for the 500 ms budget, not a measurement.
    printf("style %-9s %5u B  '%s'  anim frames %d in 3 s, host avg %.0f us, worst %.0f us, paint %.0f us (~clock %.0f ms)\n",
           IDS[p], (unsigned)pk.size(), info ? info->name : "?", frames, frames ? total / frames : 0.0, worst, paintUs,
           43 + 1.54 * paintUs);
    // an alert brings the classic screen back; clearing it returns to the pack
    ClawdData al = styleState(0);
    strcpy(al.alertMsg, "Claude needs your permission to run: npm test");
    clawdUiPayload(al);
    run(800, CLAWD_SCREEN_USAGE, -1);
    CHECK(!clawdUiOnStyle(), "%s: an alert left the pack on screen", IDS[p]);
    if (p == 0) shot(dir, "32-style-alert");
    ClawdData cl = styleState(0);
    cl.alertClear = true;
    clawdUiPayload(cl);
    run(800, CLAWD_SCREEN_USAGE, -1);
    CHECK(clawdUiOnStyle(), "%s: the pack did not come back after the alert", IDS[p]);
    if (p == 0) shot(dir, "32-style-alert-cleared");
  }
  // Enterprise keeps the classic cards
  ClawdData ent = base();
  ent.enterprise = true; ent.sessionPct = 38; ent.timePct = 61;
  strcpy(ent.resetDate, "Nov 1");
  clawdUiPayload(ent);
  run(800, CLAWD_SCREEN_AUTO, -1);
  CHECK(!clawdUiOnStyle(), "an Enterprise account got the pack");
  shot(dir, "32-style-enterprise");

  // guards: a flipped byte, a future API, a truncated pack
  std::vector<uint8_t> pk = packs[0];
  if (only && *only) pk.clear();
  if (!pk.empty()) {
    pk[pk.size() / 2] ^= 0x40;
    CHECK(styleValidate(pk.data(), (uint32_t)pk.size()), "a corrupt pack validated");
    CHECK(!styleAttach(pk.data(), (uint32_t)pk.size()) && !styleActive(), "a corrupt pack attached");
    pk = packs[0];
    pk[4] = STYLE_API + 1;
    CHECK(styleValidate(pk.data(), (uint32_t)pk.size()), "a future-API pack validated");
    CHECK(styleValidate(packs[0].data(), (uint32_t)packs[0].size() - 4), "a truncated pack validated");
  }
  styleDetach();
  clawdUiPayload(base());
  run(800, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "33-style-removed");
  printf("styles: %s\n", bad ? "FAILED" : "ok");
  return bad;
}

// Non-background pixels in the idle screen's IP line.
static int idleAddrInk() {
  int n = 0;
  for (int y = CLAWD_H - 10 - 19 - 24; y < CLAWD_H - 10 - 19 - 24 + 14; y++)
    for (int x = 0; x < CLAWD_W; x++) n += fb[y * CLAWD_W + x] != CLAWD_BG;
  return n;
}

// Before the first payload since boot the usage screen is the classic waiting
// screen (cloud + IP + "Waiting"), even with a style pack installed; the pack
// takes over once data arrives. Must run before any clawdUiPayload.
static int checkWaiting(const char* dir) {
  int bad = 0;
  clawdUiSetAddress("192.168.1.48");
  run(3000, CLAWD_SCREEN_USAGE, -1);
  shot(dir, "01-waiting");
  const int ink = idleAddrInk();
  CHECK(ink > 0, "no IP on the waiting screen");
  clawdUiSetAddress("10.97.44.122");
  run(100, CLAWD_SCREEN_USAGE, -1);
  CHECK(idleAddrInk() != ink && idleAddrInk() > 0, "a new IP was not redrawn on the waiting screen");
  clawdUiSetAddress("192.168.1.48");
  run(100, CLAWD_SCREEN_USAGE, -1);
  const char* sdir = getenv("CLAWD_STYLE_DIR");
  std::vector<uint8_t> pk;
  if (sdir) {
    char path[512];
    snprintf(path, sizeof(path), "%s/synth.pak", sdir);
    pk = readFile(path);
  }
  if (!pk.empty() && styleAttach(pk.data(), (uint32_t)pk.size())) {
    run(1000, CLAWD_SCREEN_USAGE, -1);
    CHECK(!clawdUiOnStyle(), "a style pack drew the screen before any data");
    CHECK(idleAddrInk() == ink, "no IP on the waiting screen with a style pack");
    shot(dir, "01b-waiting-style");
    ClawdData d = base();
    clawdUiPayload(d);
    run(1000, CLAWD_SCREEN_USAGE, -1);
    CHECK(clawdUiOnStyle(), "the style pack did not take over once data came");
    run(CLAWD_DATA_FRESH_MS + 1000, CLAWD_SCREEN_USAGE, -1);
    CHECK(clawdUiOnStyle(), "the style pack left the stale screen after the first data");
    styleDetach();
    run(1000, CLAWD_SCREEN_USAGE, -1);
    CHECK(!clawdUiOnStyle() && idleAddrInk() == 0, "the idle screen kept the IP after the first data");
  } else {
    printf("waiting: synth.pak not found, style part skipped\n");
  }
  printf("waiting screen: %s\n", bad ? "FAILED" : "ok");
  return bad;
}

int main(int argc, char** argv) {
  const char* dir = argc > 1 ? argv[1] : ".";
  MemSurface surf;
  clawdSetSurface(&surf);
  clawdUiBegin();

  const int waitBad = checkWaiting(dir);

  ClawdData d = base();
  clawdUiPayload(d);
  run(8000, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "02-usage");

  d.sessionPct = 91; d.sessionResetMin = 12; d.weeklyPct = 55; d.clockFmt = 12;
  d.clockEpoch = 1760220000;   // 22:00 -> 10:00 PM
  clawdUiPayload(d);
  run(12000, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "03-usage-high-12h");

  ClawdData e = base();
  e.enterprise = true; e.sessionPct = 38; e.timePct = 61;
  strcpy(e.resetDate, "Nov 1");
  clawdUiPayload(e);
  run(1000, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "04-enterprise");

  ClawdData a = base();
  strcpy(a.alertMsg, "Claude needs your permission to run: npm test -- --watch");
  clawdUiPayload(a);
  run(1500, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "05-alert");
  ClawdData x = base();
  x.alertClear = true;
  clawdUiPayload(x);
  run(1000, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "06-alert-cleared");

  g_layout = 9;                      // a layout this build lacks: classic
  run(1000, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "06f-unknown-layout");
  g_layout = CLAWD_LAYOUT_CLASSIC;

  const int styleBad = checkStyles(dir);
  if (getenv("CLAWD_STYLES_ONLY")) return (styleBad + waitBad) ? 1 : 0;   // the style-pack loop only

  // mascot acts over time
  for (int i = 0; i < 4; i++) {
    clawdUiPayload(base());
    run(4000, CLAWD_SCREEN_USAGE, -1);
    char n[32];
    snprintf(n, sizeof(n), "07-mascot-%d", i);
    shot(dir, n);
  }

  // stale -> idle view in usage mode, splash in auto mode
  run(95000, CLAWD_SCREEN_USAGE, -1);
  shot(dir, "08-idle");
  const int bandBad = checkAddressBand(dir);

  for (int i = 0; i < clawdAnimCount(); i++) {
    run(2500, CLAWD_SCREEN_SPLASH, (int8_t)i);
    char name[16], n[48];
    clawdAnimName(i, name, sizeof(name));
    for (char* p = name; *p; p++) if (*p == ' ') *p = '_';
    snprintf(n, sizeof(n), "10-splash-%02d-%s", i, name);
    shot(dir, n);
  }
  // desk view (POST /api/state): plain, with a text line, with a countdown,
  // marquee text, then expiry back to the usage screen
  {
    clawdUiPayload(base());
    // anim: comma-separated sequence; rep: cycle it; ms: how long to play before the shot
    struct { const char* anim; const char* text; int cd; bool rep; uint32_t ms; } cases[] = {
        {"typing", "", 0, false, 1700},
        {"juggling", "2 subagents", 0, false, 1700},
        {"notification", "Allow Bash: npm test -- --watch in clawd-home?", 0, false, 1700},
        {"reading", "Focus", 1500, false, 1700},
        {"yawn,doze,collapse,sleeping", "", 0, false, 24000},   // 7 + 4 + 11 s: in "sleeping"
        {"wake,happy", "Welcome back!", 0, false, 2500},       // mid "happy"
        {"idle,look", "", 0, true, 1700},
        {"error", "Test failed", 0, false, 1700},
    };
    int k = 0;
    for (auto& c : cases) {
      ClawdState s;
      memset(&s, 0, sizeof(s));
      char list[64];
      strncpy(list, c.anim, sizeof(list) - 1);
      list[sizeof(list) - 1] = 0;
      for (char* tok = strtok(list, ","); tok && s.animCount < CLAWD_SEQ_MAX; tok = strtok(nullptr, ","))
        strncpy(s.anim[s.animCount++], tok, sizeof(s.anim[0]) - 1);
      s.repeat = c.rep;
      s.urgent = true;
      strncpy(s.text, c.text, sizeof(s.text) - 1);
      s.countdownSec = c.cd;
      s.dim = -1;
      s.ttlSec = 30;
      clawdUiState(s);
      run(c.ms, CLAWD_SCREEN_AUTO, -1);
      char n[64];
      snprintf(n, sizeof(n), "11-desk-%d-%s", k++, clawdUiAnimName());
      for (char* p = n; *p; p++) if (*p == ' ') *p = '_';
      shot(dir, n);
    }
    clawdUiPayload(base());
    run(31000, CLAWD_SCREEN_AUTO, -1);
    shot(dir, "12-desk-expired");
    ClawdDeskStats ds;
    clawdUiDeskStats(ds);
    printf("desk frames drawn: %u\n", (unsigned)ds.total);
  }
  // Usage from Codex CLI labels the cards; "off" shows the time and Clawd, never "No data".
  ClawdData cx = base();
  strcpy(cx.source, "codex");
  clawdUiPayload(cx);
  run(1500, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "13-codex");
  ClawdData off = base();
  off.ok = false;
  strcpy(off.source, "off");
  clawdUiPayload(off);
  run(1500, CLAWD_SCREEN_USAGE, -1);
  shot(dir, "14-usage-off");
  run(CLAWD_DATA_FRESH_MS + 1000, CLAWD_SCREEN_USAGE, -1);
  shot(dir, "14b-usage-off-later");
  clawdUiPayload(base());
  run(1500, CLAWD_SCREEN_AUTO, -1);

  const int bad = checkVariants() + bandBad + styleBad + waitBad;
  printf("pixels pushed: %ld\n", g_pushed);
  return bad ? 1 : 0;
}
