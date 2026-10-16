// ClawdRate.h — short-term usage rate (%/min of the 5-hour session), ported
// from the original firmware's usage_rate.cpp. Picks which group of animations plays.
#pragma once
#include <stdint.h>

// Feed the session % of every fresh payload. Returns true on a session reset
// (a >5-point drop), never on the first sample.
bool clawdRateSample(float sessionPct);
// 0 = idle, 1 = normal, 2 = active, 3 = heavy (0 until ~4 min of history).
int  clawdRateGroup();
