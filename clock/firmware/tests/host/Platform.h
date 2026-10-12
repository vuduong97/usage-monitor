// Host stand-in for Platform.h: the one call Settings.cpp makes.
#pragma once
#include <Arduino.h>
static inline uint32_t platformChipId() { return 0xBEEF; }
