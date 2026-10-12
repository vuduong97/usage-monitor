#include "UsageMode.h"
#include <Arduino_GFX_Library.h>
#include "Gfx.h"
#include "UsageClient.h"
#include "ClawdDraw.h"
#include "ClawdUi.h"

UsageMode g_usageMode;

uint32_t clawdNow() { return millis(); }
uint32_t clawdMicros() { return micros(); }
// Arduino's random() reads the hardware RNG (RANDOM_REG32) until randomSeed() is called.
uint32_t clawdRandom(uint32_t n) { return (uint32_t)random((long)n); }

// The panel as a ClawdSurface. Composed bands are pushed in one SPI burst
// each; the Display tab's colour correction (gfxTint) is applied on the way
// out, in place, since the band buffer is rebuilt for every push anyway.
class GfxSurface : public ClawdSurface {
 public:
  void fill(int x, int y, int w, int h, uint16_t c) override {
    gfxDev()->fillRect(x, y, w, h, c);
  }
  void push(int x, int y, int w, int h, const uint16_t* px) override {
    uint16_t* p = const_cast<uint16_t*>(px);   // the shared band buffer, ours to edit
    for (int i = 0; i < w * h; i++) p[i] = gfxTint(p[i]);   // identity fast path inside
    gfxDev()->draw16bitRGBBitmap(x, y, p, w, h);
    yield();   // a full-screen repaint is ~30 bands: keep WiFi + the watchdog fed
  }
  void span(int x, int y, int w, uint16_t* px) override {
    for (int i = 0; i < w; i++) px[i] = gfxTint(px[i]);
    gfxDev()->draw16bitRGBBitmap(x, y, px, w, 1);
  }
  uint16_t tint(uint16_t c) override { return gfxTint(c); }
};

static GfxSurface g_surface;
static bool       g_clawdReady = false;

static void ensureClawd() {
  if (g_clawdReady) return;
  clawdSetSurface(&g_surface);
  clawdUiBegin();
  g_clawdReady = true;
}

void UsageMode::begin(const Settings& s) {
  usageInit(s);
  ensureClawd();
}

void UsageMode::invalidate(const Settings& s) {
  usageInit(s);
  usageForceRefresh();
  ensureClawd();
  clawdUiInvalidate();
}

void UsageMode::wake(const Settings& s) {
  (void)s;
  ensureClawd();
  clawdUiInvalidate();
}

void UsageMode::service(const Settings& s) {
  // Pull mode: poll the daemon when a Usage URL is set. Push mode (Clawdmeter's
  // daemon with http_targets): leave it blank and the daemon POSTs /api/usage.
  if (s.usage.usageUrl.length() >= 8) usageService(s);
  ensureClawd();
  clawdUiService(s.usage.screen, s.usage.anim);
}
