#include "ClawdRate.h"
#include "ClawdAnim.h"

// Thresholds in %/min. A 5-hour (300 min) session / 100 % = 0.33 %/min, the
// pace at which the window fills exactly as it resets: "heavy" starts there.
#define RATE_THRESH_NORMAL 0.10f
#define RATE_THRESH_ACTIVE 0.20f
#define RATE_THRESH_HEAVY  0.33f
// Require ~4 min of history so one noisy 1 % step doesn't read as heavy use.
#define MIN_WINDOW_MS 240000UL
#define RING_SIZE 6

struct Sample { uint32_t ms; float pct; };
static Sample  ring[RING_SIZE];
static uint8_t count = 0, head = 0;

bool clawdRateSample(float pct) {
  uint32_t now = clawdNow();
  bool reset = false;
  if (count > 0) {
    uint8_t latest = (head + RING_SIZE - 1) % RING_SIZE;
    if (pct + 5.0f < ring[latest].pct) { count = 0; head = 0; reset = true; }
  }
  ring[head] = {now, pct};
  head = (head + 1) % RING_SIZE;
  if (count < RING_SIZE) count++;
  return reset;
}

int clawdRateGroup() {
  if (count < 2) return 0;
  uint8_t o = (head + RING_SIZE - count) % RING_SIZE;
  uint8_t l = (head + RING_SIZE - 1) % RING_SIZE;
  uint32_t dt = ring[l].ms - ring[o].ms;
  if (dt < MIN_WINDOW_MS) return 0;
  float dp = ring[l].pct - ring[o].pct;
  if (dp < 0) dp = 0;
  float rate = dp * 60000.0f / (float)dt;
  if (rate < RATE_THRESH_NORMAL) return 0;
  if (rate < RATE_THRESH_ACTIVE) return 1;
  if (rate < RATE_THRESH_HEAVY)  return 2;
  return 3;
}
