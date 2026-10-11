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

// What the daemon wants on screen right now (POST /api/state): a full-screen
// desk animation for agent activity / pet moods, an optional text line with an
// optional countdown, and a brightness cap. Everything expires after ttl so a
// daemon that dies never leaves the clock stuck on "typing".
struct ClawdState {
  char    anim[16];          // desk.pak animation name, "" = no desk view
  char    text[96];          // ASCII line under the animation, "" = none
  int32_t countdownSec;      // > 0: append MM:SS counting down from this
  int16_t dim;               // 0..100 brightness cap, -1 = none
  uint16_t ttlSec;           // state lifetime (default 60)
};

enum : uint8_t { CLAWD_SCREEN_AUTO = 0, CLAWD_SCREEN_USAGE = 1, CLAWD_SCREEN_SPLASH = 2 };

#define CLAWD_DATA_FRESH_MS 90000UL   // usage counts as live within this window

void clawdUiBegin();
void clawdUiInvalidate();                    // another mode drew: repaint everything
void clawdUiPayload(const ClawdData& d);     // a payload landed (POST /api/usage)
void clawdUiState(const ClawdState& s);      // a state landed (POST /api/state)
int  clawdUiDim();                           // active brightness cap, or -1
bool clawdUiOnDesk();
// screen: CLAWD_SCREEN_*; fixedAnim: -1 = by usage rate, else an animation index.
void clawdUiService(uint8_t screen, int8_t fixedAnim);
bool        clawdUiOnSplash();
const char* clawdUiAnimName();               // what the splash is playing ("" if none)
