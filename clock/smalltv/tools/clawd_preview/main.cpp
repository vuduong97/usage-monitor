// Desktop preview of the Clawdmeter screens: runs the exact firmware renderer
// (src/features/clawd) against an in-memory 240x240 panel with a fake clock
// and writes PPM screenshots. Build + run: tools/clawd_preview/run.sh
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "ClawdAnim.h"
#include "ClawdDraw.h"
#include "ClawdUi.h"

static uint32_t g_now = 1000;
uint32_t clawdNow() { return g_now; }
uint32_t clawdMicros() { return g_now * 1000u; }

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

// Advance the fake clock in 10 ms loop() steps.
static void run(uint32_t ms, uint8_t screen, int8_t anim) {
  for (uint32_t t = 0; t < ms; t += 10) {
    g_now += 10;
    clawdUiService(screen, anim);
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

int main(int argc, char** argv) {
  const char* dir = argc > 1 ? argv[1] : ".";
  MemSurface surf;
  clawdSetSurface(&surf);
  clawdUiBegin();

  run(3000, CLAWD_SCREEN_USAGE, -1);
  shot(dir, "01-waiting");

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
  run(2000, CLAWD_SCREEN_AUTO, -1);
  shot(dir, "09-splash-auto");

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
  printf("pixels pushed: %ld\n", g_pushed);
  return 0;
}
