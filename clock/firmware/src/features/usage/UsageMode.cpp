#include "UsageMode.h"
#include "StyleStore.h"
#include <Arduino_GFX_Library.h>
#include "Gfx.h"
#include "UsageClient.h"
#include "ClawdDraw.h"
#include "ClawdUi.h"
#include "Net.h"

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
  styleStoreBegin();      // the installed style pack, mirrored into flash if an OTA moved it
  g_clawdReady = true;
}

void UsageMode::begin(const Settings& s) {
  usageInit(s);
  ensureClawd();
}

void UsageMode::invalidate(const Settings& s) {
  usageInit(s);
  ensureClawd();
  clawdUiInvalidate();
}

void UsageMode::service(const Settings& s) {
  // Usage and state arrive pushed (POST /api/usage, /api/state: WebPortal.cpp).
  ensureClawd();
  // The IP for the waiting splash's caption; a String per call, so not every loop.
  static uint32_t addrAt = 0;
  static bool addrSet = false;
  if (!addrSet || millis() - addrAt >= 2000) {
    addrSet = true;
    addrAt = millis();
    const bool up = netMode() == NET_AP || netConnected();
    clawdUiSetAddress(up ? netIP().c_str() : "");
  }
  clawdUiService(s.usage.screen, s.usage.anim, s.usage.layout);
}
