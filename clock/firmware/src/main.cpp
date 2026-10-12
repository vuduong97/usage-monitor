// Clawd desk clock firmware (ESP-12F / ESP8266).
//
// One feature, a self-contained DisplayMode (see Mode.h): Usage plus the Clawd
// animations (features/usage, features/clawd), fed by the desktop app's HTTP
// push. Shared plumbing (WiFi, web UI, OTA, display core, settings) lives at
// src root.
//
// License: WTFPL
#include <Arduino.h>
#include "Platform.h"
#include "config.h"
#include "Settings.h"
#include "Net.h"
#include "Gfx.h"
#include "WebPortal.h"
#include "Mode.h"
#include "Clock.h"
#include "CrashLog.h"
#include "UsageMode.h"
#include "ClawdUi.h"
#include "features/clawd/StyleStore.h"

// ---- mode -------------------------------------------------------------------
// The one compiled-in feature owns its fetch/render/dirty tracking; main.cpp
// holds no per-feature state of its own.
static DisplayMode* const kMode = &g_usageMode;

static Settings g_settings;
static String   g_resetReason;        // why the chip last reset (diagnostics)
static bool     g_safeMode = false;   // last reset was an exception -> don't re-enter the crash
static char     g_epcStr[16] = "";
static char     g_addrStr[16] = "";
static int g_lastBr = -1;        // last effective brightness written (-1 = none yet)
#if HAS_LDR
static uint32_t g_lastAutoBr = 0;
static uint8_t  g_ldrCache   = DEFAULT_BRIGHTNESS;   // last LDR reading (2 s cadence)
#endif

// Single brightness resolver: night mode overrides auto-brightness overrides the
// manual level. Only writes the PWM when the effective target changes.
static uint8_t appBaseBrightness();
static uint8_t appEffectiveBrightness() {
  uint8_t b = appBaseBrightness();
  const int cap = clawdUiDim();       // daemon's quiet-hours cap (POST /api/state)
  if (cap >= 0 && cap < b) b = (uint8_t)cap;
  return b;
}
static uint8_t appBaseBrightness() {
  if (clockNightActive()) return g_settings.clock.nightLevel;
#if HAS_LDR
  if (g_settings.autoBrightness) {
    if (millis() - g_lastAutoBr > 2000) {
      g_lastAutoBr = millis();
      int raw = analogRead(LDR_PIN);
      g_ldrCache = (uint8_t)constrain(raw * 100 / ADC_MAX, 5, 100);
    }
    return g_ldrCache;
  }
#endif
  return g_settings.brightness;
}

void appApplyBrightness() {
  uint8_t t = appEffectiveBrightness();
  if ((int)t != g_lastBr) {
    g_lastBr = t;
    gfxSetBrightness(t, g_settings.backlightInverted);
  }
}

// Exposed to the web portal (/api/status) so the last reset reason is visible.
const char* appResetReason() { return g_resetReason.c_str(); }

// Called by the web portal after settings are applied: re-init every mode and
// force a fresh repaint so a mode/URL/symbol change takes effect immediately.
void appInvalidate() {
  kMode->invalidate(g_settings);
}

static void bootProgress(const char* msg) {
  gfxBoot("Clawd", msg);
}

void setup() {
  Serial.begin(115200);
  Serial.println();
  Serial.println(FW_NAME " " FW_VERSION);

  // Capture why we (re)booted. On a reboot loop this is the key clue, and the
  // device's UART isn't exposed — so we also show it on screen below. On the
  // ESP8266 we also keep the crash PC (epc1) for addr2line decoding; the
  // ESP32-C2 (RISC-V) doesn't expose it, so epc/addr come back empty there.
  crashLogBegin();
  PlatformReset pr = platformResetInfo();
  // A panic/abort/stack overflow also reads "Software/System restart": the
  // crash log tells it apart (no safe mode for those, only the record).
  if (!pr.wasCrash && crashLogBootWasCrash()) pr.reason = "Crash (panic/abort/stack)";
  Serial.print("[boot] reset reason: ");
  Serial.println(pr.reason);

  if (pr.wasCrash) {
    g_safeMode = true;                   // crashed last boot -> stay out of the crash path
    strlcpy(g_epcStr,  pr.epc,  sizeof(g_epcStr));
    strlcpy(g_addrStr, pr.addr, sizeof(g_addrStr));
    char rich[80];
    snprintf(rich, sizeof(rich), "%s epc %s addr %s", pr.reason.c_str(),
             g_epcStr[0] ? g_epcStr : "-", g_addrStr[0] ? g_addrStr : "-");
    g_resetReason = rich;
  } else {
    g_resetReason = pr.reason;
  }

  Serial.println("[boot] settings");
  settingsBegin();
  loadSettings(g_settings);

  Serial.println("[boot] display");
  gfxBegin(g_settings);
  gfxBoot(g_safeMode ? "Crashed" : "Clawd", FW_VERSION);

  Serial.println("[boot] net");
  netBegin(g_settings, bootProgress);
  // Arm SNTP now that WiFi (STA) is up, but only if night mode is enabled (the
  // SNTP client costs heap). clockReapply arms it iff needed. Skipped after a
  // crash so a fault in here can't boot-loop before the web server starts (the
  // device then comes up in safe mode, OTA-recoverable, instead of needing UART).
  if (!g_safeMode) clockReapply(g_settings);

  Serial.println("[boot] web");
  webPortalBegin(g_settings);

  Serial.println("[boot] modes");
  kMode->begin(g_settings);
  Serial.println("[boot] done");

  if (netMode() == NET_AP) {
    gfxApInfo(g_settings.apSsid.c_str(), g_settings.apPass.c_str(), netIP().c_str());
  } else if (g_safeMode) {
    // Last boot crashed: show the crash address (persistent) and keep the web
    // server up for OTA recovery — don't enter the render path that crashed.
    gfxCrash(g_epcStr, g_addrStr, netIP().c_str());
  } else {
    // Show which network we joined and how to reach the web UI, long enough to read.
    gfxStaInfo(netSSID().c_str(), netIP().c_str());
    delay(3500);
  }
}

// Longest loop() pass over the last 1-2 minutes (ms), for /api/heap: anything
// that blocks here freezes the animations and the web server alike.
static uint32_t g_loopMaxCur = 0, g_loopMaxPrev = 0, g_loopWinStart = 0, g_loopLast = 0;
uint32_t appLoopMaxMs() { return g_loopMaxCur > g_loopMaxPrev ? g_loopMaxCur : g_loopMaxPrev; }
// The same window per part of loop(), so a long pass can be pinned on the network,
// the web server or the screen: [0] net, [1] web, [2] screen.
static uint32_t g_partCur[3], g_partPrev[3];
uint32_t appLoopPartMs(int i) { return g_partCur[i] > g_partPrev[i] ? g_partCur[i] : g_partPrev[i]; }
static inline void partEnd(int i, uint32_t& t) {
  const uint32_t now = millis();
  if (now - t > g_partCur[i]) g_partCur[i] = now - t;
  t = now;
}

static void loopProfile() {
  const uint32_t now = millis();
  if (g_loopLast) {
    const uint32_t gap = now - g_loopLast;
    if (gap > g_loopMaxCur) g_loopMaxCur = gap;
  }
  g_loopLast = now;
  if (now - g_loopWinStart >= 60000UL) {
    g_loopWinStart = now;
    g_loopMaxPrev = g_loopMaxCur;
    g_loopMaxCur = 0;
    for (int i = 0; i < 3; i++) { g_partPrev[i] = g_partCur[i]; g_partCur[i] = 0; }
  }
}

void loop() {
  loopProfile();
#if defined(CLAWD_ESP8266)
  {   // debug trail: WiFi changes, and a heartbeat while it is down
    static int lastSt = -1;
    static uint32_t lastBeat = 0;
    const int st = (int)WiFi.status();
    if (st != lastSt) {
      styleLog("wifi %d -> %d heap=%u", lastSt, st, (unsigned)ESP.getFreeHeap());
      lastSt = st;
      lastBeat = millis();
    } else if (st != WL_CONNECTED && millis() - lastBeat > 30000) {
      lastBeat = millis();
      styleLog("alive wifi=%d heap=%u", st, (unsigned)ESP.getFreeHeap());
    }
  }
#endif
  uint32_t partT = millis();
  netLoop();
  partEnd(0, partT);
  webPortalLoop();
  partEnd(1, partT);

  if (webPortalRebootDue()) {
    delay(120);
    crashLogMarkRestart();
    ESP.restart();
  }

  if (g_safeMode) {
    delay(5);
    return;  // crashed last boot: web UI stays up for OTA recovery, no rendering
  }

  if (netMode() == NET_AP) {
    delay(5);
    return;  // setup mode: AP info stays on screen
  }

  // --- STA mode: the active feature fetches + renders itself ---

  // Night-mode state machine (NTP-trust gate), then apply the effective brightness
  // (night override / auto-brightness / manual level).
  clockService(g_settings);
  appApplyBrightness();

  partT = millis();
  kMode->service(g_settings);
  partEnd(2, partT);

  delay(5);
}
