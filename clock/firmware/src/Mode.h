// Mode.h — the display-mode interface.
//
// The feature (Usage + the Clawd animations) is a DisplayMode: it owns its own
// data fetch, render, dirty-tracking and settings slice. main.cpp holds no
// per-feature state of its own.
#pragma once
#include <Arduino.h>
#include "Settings.h"

class DisplayMode {
 public:
  virtual ~DisplayMode() {}

  virtual void begin(const Settings& s) {}          // one-time init at boot
  virtual void service(const Settings& s) {}        // every loop tick: fetch + render
  virtual void invalidate(const Settings& s) {}     // settings changed: re-init + repaint
};
