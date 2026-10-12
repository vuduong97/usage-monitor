// ClawdUi.h — the Clawd screens on the clock (240x240, no touch, no
// buttons): the usage screen of the original firmware's small layout and the full-screen
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
//
// The animation is a sequence of up to CLAWD_SEQ_MAX desk.pak names: each one
// plays one full pass (intro + one loop, or to its end) and hands over to the
// next, so chains like yawn -> doze -> collapse -> sleeping join seamlessly.
// The last one then loops, or with `repeat` the whole list cycles (idle
// rotation). A different sequence takes over at the end of the current pass
// (at most DESK_SWITCH_WAIT_MS later), or at once when `urgent`.
//
// A single looping animation can carry up to CLAWD_VAR_MAX same-meaning
// variants: it plays CLAWD_VAR_EVERY passes, then one variant picked at
// random (never the previous one when there are two or more), then itself again.
#define CLAWD_SEQ_MAX 4
#define CLAWD_VAR_MAX 3
#define CLAWD_VAR_EVERY 3
struct ClawdState {
  char    anim[CLAWD_SEQ_MAX][16]; // desk.pak animation names, animCount = 0: no desk view
  uint8_t animCount;
  char    var[CLAWD_VAR_MAX][16];  // variants of anim[0] (one animation, no repeat only)
  uint8_t varCount;
  bool    repeat;            // cycle the whole sequence instead of looping the last
  bool    urgent;            // replace what plays now instead of at its pass end
  char    text[96];          // ASCII line under the animation, "" = none
  int32_t countdownSec;      // > 0: append MM:SS counting down from this
  int16_t dim;               // 0..100 brightness cap, -1 = none
  uint16_t ttlSec;           // state lifetime (default 60)
};

enum : uint8_t { CLAWD_SCREEN_AUTO = 0, CLAWD_SCREEN_USAGE = 1, CLAWD_SCREEN_SPLASH = 2 };

// Usage screen layouts (Settings usage.layout). A value this build lacks draws
// the classic one. CLAWD_LAYOUTS is the bitmask /api/heap reports, so the
// desktop app only offers what the clock can draw.
enum : uint8_t {
  CLAWD_LAYOUT_CLASSIC = 0,   // two cards + spinner line
  CLAWD_LAYOUT_PACE = 1,      // cards with pace pill, time marker and forecast
  CLAWD_LAYOUT_HERO = 2,      // one big 5-hour number, weekly as a strip
  CLAWD_LAYOUT_RINGS = 3,     // two concentric rings around the clock
  CLAWD_LAYOUT_GAUGES = 4,    // two 270 degree gauges side by side
  CLAWD_LAYOUT_PIXEL = 5      // 20-block bars, Clawd strolling along the floor
};
// One bit per layout, spelled out: the desktop app's tools/bundle_firmware.py reads this line.
#define CLAWD_LAYOUTS ((1u << CLAWD_LAYOUT_CLASSIC) | (1u << CLAWD_LAYOUT_PACE) | \
                       (1u << CLAWD_LAYOUT_HERO) | (1u << CLAWD_LAYOUT_RINGS) | \
                       (1u << CLAWD_LAYOUT_GAUGES) | (1u << CLAWD_LAYOUT_PIXEL))

#define CLAWD_DATA_FRESH_MS 90000UL   // usage counts as live within this window

void clawdUiBegin();
void clawdUiInvalidate();                    // another mode drew: repaint everything
// The clock's IP ("" = none), captioned under the splash while it waits for data.
void clawdUiSetAddress(const char* ip);
void clawdUiPayload(const ClawdData& d);     // a payload landed (POST /api/usage)
void clawdUiState(const ClawdState& s);      // a state landed (POST /api/state)
int  clawdUiDim();                           // active brightness cap, or -1
bool clawdUiOnDesk();
// screen: CLAWD_SCREEN_*; fixedAnim: -1 = by usage rate, else an animation index;
// layout: CLAWD_LAYOUT_* for the usage screen.
void clawdUiService(uint8_t screen, int8_t fixedAnim, uint8_t layout = CLAWD_LAYOUT_CLASSIC);
bool        clawdUiOnSplash();
const char* clawdUiAnimName();               // what the splash is playing ("" if none)

// The desk pack is about to be replaced: close it and stop drawing from it.
void clawdUiDeskReleased();

// Desk player timing over the last DESK_STATS frames (for /api/heap): draw
// time per frame (decode + SPI) and how late frames started against their due
// time, in microseconds.
struct ClawdDeskStats {
  uint16_t frames;           // samples in the window
  uint32_t drawP50, drawP95, drawMax;
  uint32_t lateP95;
  uint32_t total;            // frames drawn since boot
};
void clawdUiDeskStats(ClawdDeskStats& out);
const char* clawdUiDeskAnimName();          // the desk animation on screen ("" if none)
