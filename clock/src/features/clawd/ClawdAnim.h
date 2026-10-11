// ClawdAnim.h — playback of the official Clawd animations on the SmallTV.
//
// A port of Clawdmeter's splash engine (splash.cpp): every animation plays
// intro -> loop -> outro, so switches always pass through the shared idle
// pose; the two walk gaits translate across the stage locked to their feet;
// usage-rate groups pick what plays. Rendering goes straight to the panel in
// changed grid rows (no framebuffer).
//
//   ClawdSplash  full-screen 60x60 stage, 4 px cells on the 240x240 panel
//   ClawdMascot  the small corner Clawd on the usage screen (2 px cells):
//                idles, does rate-scaled acts, waves/points on attention
//   ClawdMini    one animation crop, looping (the idle screen's cloud)
#pragma once
#include <stdint.h>
#include "ClawdAssets.h"

int  clawdAnimFind(const char* name);           // index in CLAWD_ANIMS, or -1
int  clawdAnimCount();
void clawdAnimName(int index, char* out, int n);

// intro -> loop -> outro sequencing for one animation.
struct ClawdPlayer {
  int16_t   index = -1;
  ClawdAnim a;                 // descriptor copied out of flash
  uint16_t  frame = 0;
  uint32_t  frameStart = 0;
  uint32_t  loopEntered = 0;
  bool      inLoop = false;
  bool      loopRelease = false;   // let the loop run out into the outro
  bool      holding = false;       // completed: resting on frame 0

  void start(int index, uint32_t now);
  // Advance by elapsed time. Returns true when the frame changed. `completed`
  // is set when the file ran past its last frame (the caller decides whether
  // to replay, switch or hold); the frame is then back at 0.
  bool tick(uint32_t now, bool& completed);
};

class ClawdSplash {
 public:
  void begin();
  // fixedAnim: -1 = pick from the usage-rate group and rotate every 20 s,
  // otherwise that animation index on repeat.
  void show(int8_t fixedAnim);
  void hide() { active_ = false; }
  bool active() const { return active_; }
  void tick();
  void repickForRate();        // usage-rate group changed while showing
  int  current() const { return player_.index; }

 private:
  void pick();
  void startAnim(int index);
  void render(bool force);
  static uint8_t cellAt(void* self, int gx, int gy);

  ClawdPlayer player_;
  uint16_t pal_[16];
  bool     active_ = false;
  bool     forceFull_ = false;
  int8_t   fixed_ = -1;
  uint32_t lastPick_ = 0;
  bool     pendingPick_ = false;
  uint8_t  groupRotation_[4] = {0, 0, 0, 0};
  // walkers
  uint8_t  walkKind_ = 0;      // 0 none, 1 crab, 2 front
  int16_t  walkX_ = 0, walkTarget_ = 0, walkHome_ = 0;
  int8_t   walkDir_ = 0, walkFace_ = 1;
  uint8_t  walkPhase_ = 0;
  uint32_t walkPhaseStart_ = 0;
  void walkBegin(int target);
  void walkChoreo(uint32_t now);
  int  gaitCells(uint16_t frame, bool fromLoop) const;
};

class ClawdMascot {
 public:
  void begin(int x, int y);    // top-left of the 28x21-cell window, 2 px cells
  void reset();                // back to the still pose, repaint on next tick
  void tick(int rateGroup);
  void setAttention(bool on);
  void invalidate() { force_ = true; }

 private:
  void startAct(int index);
  void render(bool force);
  static uint8_t cellAt(void* self, int gx, int gy);

  ClawdPlayer player_;
  uint16_t pal_[16];
  int16_t  x_ = 0, y_ = 0;
  bool     acting_ = false, attention_ = false, force_ = true;
  uint8_t  actIdx_ = 0, attnAct_ = 0;
  uint32_t stillSince_ = 0;
};

class ClawdMini {
 public:
  void begin(const char* anim, int cx, int cy, int px);   // centred at (cx, cy), ~px wide
  void reset() { force_ = true; frame_ = 0; }
  void tick();

 private:
  static uint8_t cellAt(void* self, int gx, int gy);
  ClawdAnim a_;
  uint16_t  pal_[16];
  bool      ok_ = false, force_ = true;
  uint16_t  frame_ = 0;
  uint32_t  started_ = 0;
  int16_t   x_ = 0, y_ = 0;
  uint8_t   cell_ = 2;
};

uint32_t clawdNow();   // millis(); supplied by the platform (device or preview)
