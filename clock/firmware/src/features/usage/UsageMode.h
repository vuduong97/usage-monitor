// UsageMode.h — Claude usage, Clawd edition.
//
// The screens are a port of the original firmware's compact (240 px) UI: the usage card
// pair (or Enterprise spending/period), the daemon-fed clock, the whimsical
// status line, attention alerts and the animated corner Clawd, plus the
// full-screen official Clawd animations. All of it lives in features/clawd;
// this mode just feeds it data and ticks it (see ClawdUi.h).
#pragma once
#include "Mode.h"
#include "config.h"
#include "UsageData.h"

class UsageMode : public DisplayMode {
 public:
  void begin(const Settings& s) override;
  void service(const Settings& s) override;
  void invalidate(const Settings& s) override;
};

extern UsageMode g_usageMode;
