// ClawdUi.h — the Clawdmeter screens on the SmallTV (240x240, no touch, no
// buttons): the usage screen of Clawdmeter's small layout and the full-screen
// Clawd splash. Which one shows is chosen in the web UI (auto / usage /
// splash); an attention alert always brings the usage screen forward.
//
// Plain C types only, so the desktop preview (tools/clawd_preview) can drive
// exactly the code that runs on the device.
#pragma once
#include <stdint.h>

struct ClawdData {
  bool  ok;                  // false = the daemon's "no data" beat (no live token)
  float sessionPct;          // 5-hour window %, or spending % on Enterprise
  int   sessionResetMin;     // -1 = unknown
  float weeklyPct;
  int   weeklyResetMin;
  bool  enterprise;
  int   timePct;             // Enterprise: % of the billing period elapsed
  char  resetDate[12];       // Enterprise: e.g. "Jul 1"
  long  clockEpoch;          // local wall-clock seconds from the daemon, 0 = none
  int   clockFmt;            // 12 or 24
  char  alertMsg[128];       // attention message, "" = none
  bool  alertClear;          // the session moved on: end the alert
};

enum : uint8_t { CLAWD_SCREEN_AUTO = 0, CLAWD_SCREEN_USAGE = 1, CLAWD_SCREEN_SPLASH = 2 };

#define CLAWD_DATA_FRESH_MS 90000UL   // usage counts as live within this window

void clawdUiBegin();
void clawdUiInvalidate();                    // another mode drew: repaint everything
void clawdUiPayload(const ClawdData& d);     // a payload landed (POST /api/usage)
// screen: CLAWD_SCREEN_*; fixedAnim: -1 = by usage rate, else an animation index.
void clawdUiService(uint8_t screen, int8_t fixedAnim);
bool        clawdUiOnSplash();
const char* clawdUiAnimName();               // what the splash is playing ("" if none)
