#include "UsageClient.h"
#include "Platform.h"
#include <ArduinoJson.h>
#include <math.h>
#include "ClawdUi.h"

static UsageData g_usage;
static uint32_t  g_nextPollMs = 0;
static bool      g_inited = false;

// ---------------------------------------------------------------------------
void usageInit(const Settings& s) {
  (void)s;
  g_usage.clear();
  g_nextPollMs = millis();
  g_inited = true;
}

void usageForceRefresh() { g_nextPollMs = millis(); }

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
// Clawdmeter daemon extensions (its BLE payload, same keys over HTTP):
//   acct = "ent" on Enterprise: s = spending %, tp = % of the billing period
//          elapsed, rd = reset date ("Jul 1")
//   t/tf = local wall-clock epoch + 12/24 h format (drives the title clock)
//   m    = attention message (alert)    x = true: clear the alert
//   ok   = false is the daemon's "no data" beat: accepted, the screen goes idle
//          once the last good data ages out. The chime/beep keys (c, b) are
//   ignored — the SmallTV has no speaker.
static void usageFilter(JsonDocument& f) {
  f["s"] = true; f["sr"] = true; f["w"] = true;
  f["wr"] = true; f["st"] = true; f["ok"] = true;
  f["acct"] = true; f["tp"] = true; f["rd"] = true;
  f["t"] = true; f["tf"] = true; f["m"] = true; f["x"] = true;
}

// Forward a payload to the Clawdmeter screens.
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

static bool parseUsage(UsageData& d, Stream& stream) {
  JsonDocument filter; usageFilter(filter);
  JsonDocument doc;
  if (deserializeJson(doc, stream, DeserializationOption::Filter(filter))) return false;
  return applyUsageDoc(d, doc);
}

// Pushed payload (POST /api/usage): same contract, parsed from a String body.
bool usageApply(const String& body) {
  JsonDocument filter; usageFilter(filter);
  JsonDocument doc;
  if (deserializeJson(doc, body, DeserializationOption::Filter(filter))) return false;
  return applyUsageDoc(g_usage, doc);
}

// ---- one HTTP(S) GET + parse (mirrors StockClient::fetchUrl) ----------------
static bool fetchUsage(const Settings& s) {
  const String& url = s.usage.usageUrl;
  if (url.length() < 8) return false;
  bool https = url.startsWith("https://");

  std::unique_ptr<NetClient> client;
  if (https) {
#if defined(SMALLTV_CLAWD)
    return false;   // no TLS in the Clawdmeter build: use http:// or push mode
#endif
    if (ESP.getFreeHeap() < 20000) return false;   // too little heap for TLS (incl. the 9 KB thunk); skip, don't crash
    client.reset(platformMakeSecureClient(2048));   // LAN / self-hosted endpoint
  } else {
    client.reset(new WiFiClient());
  }

  HTTPClient http;
  http.setTimeout(s.httpTimeout);
  http.setReuse(false);
  if (!http.begin(*client, url)) return false;
  http.addHeader("Accept", "application/json");

  int code = http.GET();
  if (code != HTTP_CODE_OK) { http.end(); return false; }

  bool ok = parseUsage(g_usage, http.getStream());
  http.end();
  return ok;
}

// ---------------------------------------------------------------------------
void usageService(const Settings& s) {
  if (!g_inited) usageInit(s);
  if ((int32_t)(millis() - g_nextPollMs) < 0) return;

  if (!fetchUsage(s)) g_usage.error = true;   // keep stale data, flag the error

  g_nextPollMs = millis() + (uint32_t)s.usage.pollSec * 1000UL;
}
