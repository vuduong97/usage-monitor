#include "ClawdAnim.h"
#include "ClawdDraw.h"
#include "ClawdRate.h"
#include "ClawdPack.h"

// ---- catalogue ----------------------------------------------------------------
// Frame buffers: the splash and the idle mini never show together, so they
// share one; the corner mascot has its own. Both halves of one pool.
static uint8_t  g_framePool[2 * CLAWD_MAX_FRAME];
static uint8_t* const g_bigBuf = g_framePool;
static uint8_t* const g_masBuf = g_framePool + CLAWD_MAX_FRAME;

int clawdAnimCount() { return clawdPackCount(); }

void clawdAnimName(int i, char* out, int n) {
  if (n <= 0) return;
  out[0] = 0;
  ClawdAnim a;
  if (!clawdPackAnim(i, a)) return;
  strncpy(out, a.name, n - 1);
  out[n - 1] = 0;
}

int clawdAnimFind(const char* name) {
  char buf[16];
  for (int i = 0; i < clawdPackCount(); i++) {
    clawdAnimName(i, buf, sizeof(buf));
    if (strcmp(buf, name) == 0) return i;
  }
  return -1;
}

static void loadPalette(const ClawdAnim& a, uint16_t* out) {
  for (int k = 0; k < 16; k++) out[k] = a.palette[k];
}

// ---- player -------------------------------------------------------------------
void ClawdPlayer::start(int i, uint32_t now) {
  index = clawdPackAnim(i, a) ? (int16_t)i : (int16_t)-1;
  frame = 0;
  frameStart = now;
  loopEntered = now;
  inLoop = false;
  loopRelease = false;
  holding = false;
  loaded = 0xFFFF;
  sync();
}

void ClawdPlayer::sync() {
  if (index < 0 || loaded == frame) return;
  if (!clawdPackFrame(a, frame, buf)) { loaded = 0xFFFF; return; }
  loaded = frame;
  hold = clawdPackHold(a, frame);
}

bool ClawdPlayer::tick(uint32_t now, bool& completed) {
  completed = false;
  if (index < 0 || holding || a.frameCount == 0) return false;
  sync();
  if (now - frameStart < hold) return false;
  const bool fromLoop = inLoop;
  uint16_t next = frame + 1;
  if (frame == a.loopEnd && !loopRelease) next = a.loopStart;
  if (next >= a.frameCount) {
    completed = true;
    next = 0;
    loopRelease = false;
  }
  frame = next;
  frameStart = now;
  sync();
  const bool nowIn = frame >= a.loopStart && frame <= a.loopEnd;
  if (nowIn && !fromLoop) loopEntered = now;
  inLoop = nowIn;
  return true;
}

// =============================================================================
// Full-screen splash: 60x60 stage, 4 px cells (240 px)
// =============================================================================
#define GRID 60
#define SPLASH_CELL (CLAWD_W / GRID)
// The official 55x37 art stage sits at a fixed anchor on the 60x60 grid and
// every animation is placed at its authored offset — never centred per crop —
// so all of them share one idle-Clawd position and transitions are seamless.
#define STAGE_ANCHOR_X ((GRID - 55) / 2)
#define STAGE_ANCHOR_Y ((GRID - 37) / 2)
#define SCENE_LOOP_MS 6000
#define ROTATE_MS 20000

// Usage-rate groups (same as Clawdmeter): idle, normal, active, heavy.
static const char* const GROUPS[4][4] = {
    {"magnifier", "walking", "pointing", "lurking"},
    {"crab walking", "waving", "trumpet", "basketball"},
    {"laptop", "dancing", "skateboard", "soccer"},
    {"racing car", "cloud", "sailing scene", "jumping happy"},
};

static uint8_t g_stagePrev[GRID * GRID / 2];   // last painted cells, 4 bpp

void ClawdSplash::begin() {
  player_.index = -1;
  player_.buf = g_bigBuf;
  active_ = false;
}

int ClawdSplash::gaitCells(uint16_t frame, bool fromLoop) const {
  // Cells the body moves when the gait advances INTO `frame`: the measured
  // backward drift of the planted feet, so feet stay put on screen.
  if (walkKind_ == 1) {                     // crab walking: 8-frame scuttle [1..8]
    if (frame == 1) return fromLoop ? 1 : 0;
    return (frame == 4 || frame == 5 || frame == 8) ? 1 : 0;
  }
  if (walkKind_ == 2) {                     // walking: 5-frame waddle [2..6]
    if (frame < 2 || frame > 6) return 0;
    if (frame == 2 && !fromLoop) return 0;
    return (frame == 6) ? 2 : 1;
  }
  return 0;
}

void ClawdSplash::startAnim(int index) {
  uint32_t now = clawdNow();
  player_.start(index, now);
  loadPalette(player_.a, pal_);
  pendingPick_ = false;
  lastPick_ = now;
  walkKind_ = 0;
  if (strcmp(player_.a.name, "crab walking") == 0) walkKind_ = 1;
  else if (strcmp(player_.a.name, "walking") == 0) walkKind_ = 2;
  if (walkKind_) {
    walkHome_ = STAGE_ANCHOR_X + player_.a.ox;
    walkX_ = walkHome_;
    walkDir_ = 0;
    walkFace_ = 1;
    walkPhase_ = 0;
    walkPhaseStart_ = now;
    player_.holding = true;                 // walkers start standing
  }
  render(false);
}

void ClawdSplash::pick() {
  const int count = clawdPackCount();
  if (count == 0) return;
  if (fixed_ >= 0) { startAnim(fixed_ < count ? fixed_ : 0); return; }
  int g = clawdRateGroup();
  if (g < 0 || g > 3) g = 0;
  for (int tries = 0; tries < 4; tries++) {
    int idx = clawdAnimFind(GROUPS[g][groupRotation_[g]++ % 4]);
    if (idx >= 0) { startAnim(idx); return; }
  }
  startAnim(0);
}

void ClawdSplash::show(int8_t fixedAnim) {
  fixed_ = fixedAnim;
  active_ = true;
  forceFull_ = true;
  pick();
}

void ClawdSplash::repickForRate() {
  if (active_ && fixed_ < 0) pick();
}

void ClawdSplash::walkBegin(int target) {
  if (target == walkX_) return;
  walkTarget_ = target;
  walkDir_ = (target > walkX_) ? 1 : -1;
  walkFace_ = walkDir_;
  player_.frame = 0;
  player_.frameStart = clawdNow();
  player_.sync();
  player_.holding = false;
  player_.loopRelease = false;
  player_.inLoop = false;
}

// Stand -> walk to the right edge -> off-screen left -> back home (Clawdmeter's
// demo choreography).
void ClawdSplash::walkChoreo(uint32_t now) {
  if (!player_.holding) return;
  const int w = player_.a.w;
  switch (walkPhase_) {
    case 0: if (now - walkPhaseStart_ > 1200) { walkPhase_ = 1; walkBegin(GRID - w); } break;
    case 1: walkPhase_ = 2; walkPhaseStart_ = now; break;
    case 2: if (now - walkPhaseStart_ > 1200) { walkPhase_ = 3; walkBegin(-w); } break;
    case 3: walkPhase_ = 4; walkPhaseStart_ = now; break;
    case 4: if (now - walkPhaseStart_ > 800) { walkPhase_ = 5; walkBegin(walkHome_); } break;
    case 5: walkPhase_ = 0; walkPhaseStart_ = now; break;
  }
}

uint8_t ClawdSplash::cellAt(void* self, int gx, int gy) {
  ClawdSplash& s = *(ClawdSplash*)self;
  const ClawdAnim& a = s.player_.a;
  // Art touching its crop's left/right edge was drawn to hang off that edge
  // (lurking peeks in from the left), so it snaps to the true screen edge.
  int ax = STAGE_ANCHOR_X + a.ox;
  if (a.ox == 0) ax = 0;
  if (a.ox + a.w == 55) ax = GRID - a.w;
  if (s.walkKind_) ax = s.walkX_;
  const int ay = STAGE_ANCHOR_Y + a.oy;
  int sx = gx - ax;
  if (s.walkKind_ && s.walkFace_ < 0) sx = a.w - 1 - sx;   // walking left: mirror
  return clawdFrameCell(a, s.player_.cells(), sx, gy - ay);
}

void ClawdSplash::render(bool force) {
  if (!active_ || player_.index < 0) return;
  if (forceFull_ && !force) return;         // a full repaint is due on the next tick
  clawdRenderCells(cellAt, this, 0, 0, GRID, GRID, SPLASH_CELL, 0, 0, pal_, g_stagePrev, force);
}

void ClawdSplash::tick() {
  if (!active_ || player_.index < 0) return;
  const uint32_t now = clawdNow();
  if (forceFull_) { forceFull_ = false; render(true); }
  if (walkKind_) walkChoreo(now);

  // Scenes hold their loop for SCENE_LOOP_MS, then the outro plays.
  if (!walkKind_ && player_.inLoop && !player_.loopRelease &&
      now - player_.loopEntered >= SCENE_LOOP_MS)
    player_.loopRelease = true;

  // Auto-rotate, never a hard cut: walkers switch while standing at home,
  // everything else releases its loop and switches after the outro.
  if (fixed_ < 0 && now - lastPick_ >= ROTATE_MS) {
    if (walkKind_) {
      if (walkPhase_ == 0 && player_.holding) { pick(); return; }
    } else {
      player_.loopRelease = true;
      pendingPick_ = true;
      lastPick_ = now;
    }
  }

  const bool fromLoop = player_.inLoop;
  bool completed;
  if (!player_.tick(now, completed)) return;
  if (completed) {
    if (pendingPick_) { pendingPick_ = false; pick(); return; }
    if (walkKind_) {                        // walk finished: stand
      player_.holding = true;
      render(false);
      return;
    }
  }
  // Walk translation locked to the gait frames; clamp onto the target, then
  // release the loop so the gait exits and Clawd stands.
  if (walkKind_ && walkDir_ != 0 && player_.inLoop) {
    walkX_ += walkDir_ * gaitCells(player_.frame, fromLoop);
    if ((walkDir_ > 0 && walkX_ >= walkTarget_) || (walkDir_ < 0 && walkX_ <= walkTarget_)) {
      walkX_ = walkTarget_;
      walkDir_ = 0;
      player_.loopRelease = true;
    }
  }
  render(false);
}

// =============================================================================
// Corner mascot: a 28x21-cell window of the art stage (x 15..42, y 16..36) —
// the union of the idle pose and the acts — at 2 px per cell.
// =============================================================================
#define MAS_GX0 15
#define MAS_GY0 16
#define MAS_GW 28
#define MAS_GH 21
#define MAS_CELL 2
#define MAS_ACT_LOOP_MS 2500

static const char* const MAS_ACTS[4][3] = {      // Clawdmeter's per-rate acts, minus
    {"pointing", "waving", nullptr},             // the lurking walk-off trips
    {"waving", "pointing", nullptr},
    {"waving", "dancing", nullptr},
    {"dancing", "waving", "dancing"},
};
static const uint16_t MAS_STILL_MS[4] = {10000, 7000, 5000, 3500};
static uint8_t g_mascotPrev[MAS_GW * MAS_GH / 2 + 1];

void ClawdMascot::begin(int x, int y) {
  player_.buf = g_masBuf;
  x_ = (int16_t)x;
  y_ = (int16_t)y;
  reset();
}

void ClawdMascot::reset() {
  int idx = clawdAnimFind("walking");          // frame 0 == the official still pose
  player_.start(idx < 0 ? 0 : idx, clawdNow());
  player_.holding = true;
  loadPalette(player_.a, pal_);
  acting_ = false;
  stillSince_ = clawdNow();
  force_ = true;
}

void ClawdMascot::startAct(int index) {
  if (index < 0) return;
  player_.start(index, clawdNow());
  loadPalette(player_.a, pal_);
  acting_ = true;
}

void ClawdMascot::setAttention(bool on) {
  if (attention_ == on) return;
  attention_ = on;
  if (on) {
    attnAct_ = 0;
    startAct(clawdAnimFind("waving"));
  } else if (acting_) {
    player_.loopRelease = true;             // finish the current act, then rest
  }
}

uint8_t ClawdMascot::cellAt(void* self, int gx, int gy) {
  ClawdMascot& m = *(ClawdMascot*)self;
  const ClawdAnim& a = m.player_.a;
  return clawdFrameCell(a, m.player_.cells(), gx - a.ox, gy - a.oy);
}

void ClawdMascot::render(bool force) {
  clawdRenderCells(cellAt, this, MAS_GX0, MAS_GY0, MAS_GW, MAS_GH, MAS_CELL, x_, y_,
                   pal_, g_mascotPrev, force);
}

void ClawdMascot::tick(int rate) {
  const uint32_t now = clawdNow();
  if (rate < 0 || rate > 3) rate = 0;
  if (force_) { force_ = false; render(true); }

  if (!acting_) {
    if (attention_ || now - stillSince_ < MAS_STILL_MS[rate]) return;
    const char* name = MAS_ACTS[rate][actIdx_ % 3];
    if (!name) { actIdx_ = 0; name = MAS_ACTS[rate][0]; }
    actIdx_++;
    startAct(clawdAnimFind(name));
    render(false);
    return;
  }
  if (!attention_ && player_.inLoop && !player_.loopRelease &&
      now - player_.loopEntered >= MAS_ACT_LOOP_MS)
    player_.loopRelease = true;
  bool completed;
  if (!player_.tick(now, completed)) return;
  if (completed) {
    if (attention_) {                       // alternate waving / pointing until cleared
      attnAct_ ^= 1;
      startAct(clawdAnimFind(attnAct_ ? "pointing" : "waving"));
    } else {
      acting_ = false;
      stillSince_ = now;
      int idx = clawdAnimFind("walking");
      player_.start(idx < 0 ? 0 : idx, now);
      player_.holding = true;
      loadPalette(player_.a, pal_);
    }
  }
  render(false);
}

// =============================================================================
// Mini creature: one crop, looping, centred.
// =============================================================================
void ClawdMini::begin(const char* name, int cx, int cy, int px) {
  int idx = clawdAnimFind(name);
  ok_ = idx >= 0 && clawdPackAnim(idx, a_);
  if (!ok_) return;
  loadPalette(a_, pal_);
  int amax = a_.w > a_.h ? a_.w : a_.h;
  cell_ = (uint8_t)(px / amax < 1 ? 1 : px / amax);
  x_ = (int16_t)(cx - a_.w * cell_ / 2);
  y_ = (int16_t)(cy - a_.h * cell_ / 2);
  frame_ = 0;
  hold_ = clawdPackHold(a_, 0);
  started_ = clawdNow();
  force_ = true;
}

uint8_t ClawdMini::cellAt(void* self, int gx, int gy) {
  ClawdMini& m = *(ClawdMini*)self;
  return clawdFrameCell(m.a_, g_bigBuf, gx, gy);
}

void ClawdMini::tick() {
  if (!ok_) return;
  const uint32_t now = clawdNow();
  bool changed = force_;
  if (now - started_ >= hold_) {
    started_ = now;
    frame_ = (frame_ + 1) % a_.frameCount;
    hold_ = clawdPackHold(a_, frame_);
    changed = true;
  }
  if (!changed) return;
  // The buffer is shared with the splash, so reload on every paint.
  if (!clawdPackFrame(a_, frame_, g_bigBuf)) return;
  // Shares the splash's diff buffer too: the two never show at the same time.
  clawdRenderCells(cellAt, this, 0, 0, a_.w, a_.h, cell_, x_, y_, pal_, g_stagePrev, force_);
  force_ = false;
}
