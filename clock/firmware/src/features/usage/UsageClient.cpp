#include "UsageClient.h"
#include <Arduino.h>
#include <ArduinoJson.h>
#include <math.h>
#include "ClawdUi.h"

static UsageData g_usage;

// ---------------------------------------------------------------------------
// Push only: Clawd.app POSTs /api/usage (every 60 s) and /api/state (every
// 20 s). The old pull mode (the clock GETting a Usage URL) is gone: it blocked
// loop() for the whole HTTP timeout when the URL was dead.
void usageInit(const Settings& s) {
  (void)s;
  g_usage.clear();
}

const UsageData& usageGet() { return g_usage; }

bool usageFresh(uint32_t withinMs) {
  return g_usage.valid && (millis() - g_usage.lastOkMs) <= withinMs;
}

// ---- parse: usage contract -------------------------------------------------
// { "s":29, "sr":142, "w":4, "wr":9876, "st":"allowed", "ok":true }
//   s  = 5h utilization %        sr = minutes until 5h reset
//   w  = 7d utilization %        wr = minutes until 7d reset
//   st = rate-limit status       ok = false => explicit "no data"
//
// Clawd desktop app extensions (its BLE payload, same keys over HTTP):
//   acct = "ent" on Enterprise: s = spending %, tp = % of the billing period
//          elapsed, rd = reset date ("Jul 1")
//   t/tf = local wall-clock epoch + 12/24 h format (drives the title clock)
//   m    = attention message (alert)    x = true: clear the alert
//   ok   = false is the daemon's "no data" beat: accepted, the screen goes idle
//          once the last good data ages out. The chime/beep keys (c, b) are
//   ignored — the clock has no speaker.
//   src  = whose usage: absent = Claude, "codex" = Codex CLI (labels the cards);
//          "off" with ok = false = the user shows no usage: time and Clawd only
//   why  = with ok = false: "login" = the Claude login expired (the screen says so)
static void usageFilter(JsonDocument& f) {
  f["s"] = true; f["sr"] = true; f["w"] = true;
  f["wr"] = true; f["st"] = true; f["ok"] = true;
  f["acct"] = true; f["tp"] = true; f["rd"] = true;
  f["t"] = true; f["tf"] = true; f["m"] = true; f["x"] = true;
  f["src"] = true; f["why"] = true;
}

// Forward a payload to the Clawd screens.
static void forwardToClawd(JsonDocument& doc, bool ok) {
  ClawdData c;
  memset(&c, 0, sizeof(c));
  c.ok = ok;
  c.sessionPct      = constrain(doc["s"] | 0.0f, 0.0f, 100.0f);
  c.sessionResetMin = doc["sr"] | -1;
  c.weeklyPct       = constrain(doc["w"] | 0.0f, 0.0f, 100.0f);
  c.weeklyResetMin  = doc["wr"] | -1;
  c.enterprise      = strcmp(doc["acct"] | "pro", "ent") == 0;
  c.timePct         = constrain(doc["tp"] | 0, 0, 100);
  strlcpy(c.resetDate, doc["rd"] | "", sizeof(c.resetDate));
  c.clockEpoch      = doc["t"] | 0L;
  c.clockFmt        = doc["tf"] | 24;
  strlcpy(c.alertMsg, doc["m"] | "", sizeof(c.alertMsg));
  c.alertClear      = doc["x"] | false;
  strlcpy(c.source, doc["src"] | "", sizeof(c.source));
  strlcpy(c.why, doc["why"] | "", sizeof(c.why));
  clawdUiPayload(c);
}

static bool applyUsageDoc(UsageData& d, JsonDocument& doc) {
  if (doc["ok"].is<bool>() && doc["ok"].as<bool>() == false) {
    forwardToClawd(doc, false);
    return true;                       // a valid beat, not a malformed payload
  }
  if (!doc["s"].is<float>() && !doc["s"].is<int>()) return false;   // require at least session %
  forwardToClawd(doc, true);

  d.sessionPct      = constrain(doc["s"].as<float>(), 0.0f, 100.0f);
  d.weeklyPct       = constrain(doc["w"] | 0.0f, 0.0f, 100.0f);
  d.sessionResetMin = doc["sr"] | 0;
  d.weeklyResetMin  = doc["wr"] | 0;
  strlcpy(d.status, doc["st"] | "", sizeof(d.status));

  d.valid = true;
  d.error = false;
  d.lastOkMs = millis();
  return true;
}

// Pushed payload (POST /api/usage): same contract, parsed from a String body.
bool usageApply(const String& body) {
  JsonDocument filter; usageFilter(filter);
  JsonDocument doc;
  if (deserializeJson(doc, body, DeserializationOption::Filter(filter))) return false;
  return applyUsageDoc(g_usage, doc);
}

// Pushed screen state (POST /api/state), clawd-home daemon:
//   { "a":"typing", "txt":"Allow Bash?", "cd":1500, "dim":30, "ttl":60 }
//   { "a":["yawn","doze","collapse","sleeping"] }   { "a":["idle","look"], "rep":1 }
//   a   = desk.pak animation or a sequence of up to 4 (each plays one pass, the
//         last loops; rep = cycle them all), "" / absent = back to the usage screens
//   v   = up to 3 same-meaning variants of a single looping a (no rep): a plays
//         3 passes, then one variant at random, then a again; older firmware
//         ignores it   { "a":"typing", "v":["building","carrying","typing boss"] }
//   u   = urgent: replace what plays now instead of at the end of its pass
//   txt = ASCII line under it      cd  = countdown seconds appended as MM:SS
//   dim = 0..100 brightness cap    ttl = seconds the state lives (resent ~20 s)
bool usageApplyState(const String& body) {
  JsonDocument doc;
  if (deserializeJson(doc, body) || !doc.is<JsonObject>()) return false;
  ClawdState s;
  memset(&s, 0, sizeof(s));
  JsonVariant a = doc["a"];
  if (a.is<JsonArray>()) {
    for (JsonVariant v : a.as<JsonArray>()) {
      if (s.animCount >= CLAWD_SEQ_MAX) break;
      const char* n = v | "";
      if (!n[0]) continue;
      strlcpy(s.anim[s.animCount++], n, sizeof(s.anim[0]));
    }
  } else if ((a | "")[0]) {
    strlcpy(s.anim[0], a | "", sizeof(s.anim[0]));
    s.animCount = 1;
  }
  s.repeat = doc["rep"] | false;
  JsonVariant v = doc["v"];
  if (v.is<JsonArray>()) {
    for (JsonVariant x : v.as<JsonArray>()) {
      if (s.varCount >= CLAWD_VAR_MAX) break;
      const char* n = x | "";
      if (!n[0]) continue;
      strlcpy(s.var[s.varCount++], n, sizeof(s.var[0]));
    }
  }
  s.urgent = doc["u"] | false;
  strlcpy(s.text, doc["txt"] | "", sizeof(s.text));
  s.countdownSec = doc["cd"] | 0L;
  s.dim = doc["dim"].is<int>() ? (int16_t)constrain(doc["dim"].as<int>(), -1, 100) : (int16_t)-1;
  s.ttlSec = (uint16_t)constrain(doc["ttl"] | 60, 1, 3600);
  clawdUiState(s);
  return true;
}
