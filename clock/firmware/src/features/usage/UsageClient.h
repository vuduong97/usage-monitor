// UsageClient.h — Claude usage and the screen state, pushed by Clawd.app.
//
// The desktop app reads the Claude API rate-limit headers and POSTs the latest
// snapshot to /api/usage, and the agent state to /api/state (PROTOCOL.md).
#pragma once
#include "Settings.h"
#include "UsageData.h"

void usageInit(const Settings& s);       // forget the last payload
const UsageData& usageGet();
bool usageFresh(uint32_t withinMs);       // true if the last good update is recent enough

// Apply a usage payload pushed to the device (POST /api/usage).
bool usageApply(const String& body);      // parse {s,sr,w,wr,st,ok}; true on success
bool usageApplyState(const String& body); // parse {a,txt,cd,dim,ttl} (POST /api/state)
