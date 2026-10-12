#include "WebPortal.h"
#include "Platform.h"
#include <ArduinoJson.h>
#include <LittleFS.h>
#include "webui.h"
#include "Net.h"
#include "Gfx.h"
#include "UsageClient.h"
#include "ClawdAnim.h"
#include "ClawdPack.h"
#include "DeskPack.h"
#include "ClawdUi.h"
#include "StylePack.h"
#include "StyleStore.h"
#include "Clock.h"
#include "SettingsTransaction.h"
#include "CrashLog.h"

// Defined in main.cpp — re-init every mode + force a repaint after a config change.
extern void appInvalidate();
extern const char* appResetReason();   // last reset reason (diagnostics)
extern uint32_t appLoopMaxMs();        // longest loop() pass, last 1-2 min
extern void appApplyBrightness();   // main.cpp: re-resolve effective brightness now

static WebServerClass server(80);
static Settings*        S = nullptr;
static bool             g_reboot = false;
static uint32_t         g_rebootAt = 0;
// Heap low-water marks since boot, sampled every loop (webPortalLoop) and
// served by /api/heap for long-run stability checks.
static uint32_t         g_minHeap = 0xFFFFFFFF;
static uint32_t         g_minMaxBlk = 0xFFFFFFFF;

static void scheduleReboot(uint32_t inMs) {
  g_reboot = true;
  g_rebootAt = millis() + inMs;
}

// ---- optional web UI password ---------------------------------------------
// Off by default. When on, every handler below starts with this line and every
// page and endpoint needs the credentials. Digest, not basic, so the password
// is never sent over a plain-HTTP LAN. Two deliberate exceptions:
//   - the captive-portal probes, which a phone fires before anyone can type a
//     password and which only ever redirect,
//   - /api/usage, the daemon's push endpoint, which has no way to carry
//     credentials and only writes the numbers on the screen.
// Returns true when the request may proceed; on false it has already answered.
static bool requireAuth() {
  if (!S->auth.enabled || !S->auth.pass.length()) return true;
  if (server.authenticate(S->auth.user.c_str(), S->auth.pass.c_str())) return true;
  server.requestAuthentication(HTTPAuthMethod::DIGEST_AUTH, AUTH_REALM,
                               "Authentication required");
  return false;
}

// ---------------------------------------------------------------------------
static void sendJson(JsonDocument& doc, int code = 200) {
  String out;
  serializeJson(doc, out);
  server.send(code, "application/json", out);
}

static void handleRoot() {
  if (!requireAuth()) return;
  server.sendHeader("Cache-Control", "no-cache");
  server.sendHeader("Content-Encoding", "gzip");   // WEBUI_HTML_GZ is gzip'd (webui.h is generated)
  server.send_P(200, "text/html", (PGM_P)WEBUI_HTML_GZ, WEBUI_HTML_GZ_LEN);
}

static void handleGetConfig() {
  if (!requireAuth()) return;
  JsonDocument doc;
  JsonObject root = doc.to<JsonObject>();
  settingsToJson(*S, root, /*includeSecrets=*/false);
  // Clawd animation names, in index order, for the Usage tab's picker.
  JsonArray anims = root["clawdAnims"].to<JsonArray>();
  for (int i = 0; i < clawdAnimCount(); i++) {
    char name[16];
    clawdAnimName(i, name, sizeof(name));
    anims.add(String(name));
  }
  // Which chip this build runs on (the UI warns about per-chip limitations).
#if defined(CLAWD_ESP32C2)
  root["chip"] = "esp32c2";
#elif defined(CLAWD_ESP32)
  root["chip"] = "esp32";
#else
  root["chip"] = "esp8266";
#endif
  sendJson(doc);
}

static void handleStatus() {
  if (!requireAuth()) return;
  JsonDocument doc;
  JsonObject o = doc.to<JsonObject>();
  o["fw"] = FW_NAME;
  o["version"] = FW_VERSION;
  o["variant"] = FW_VARIANT;
  o["mode"] = (netMode() == NET_AP) ? "ap" : "sta";
  o["connected"] = netConnected();
  o["ssid"] = netSSID();
  o["ip"] = netIP();
  o["rssi"] = netRSSI();
  o["heap"] = ESP.getFreeHeap();
  o["maxblk"] = platformMaxFreeBlock();     // largest contiguous block (TLS handshake needs one)
  o["contstk"] = platformFreeContStack();   // primary stack headroom (ESP8266)
  o["uptime"] = millis() / 1000;
  o["reset"] = appResetReason();
  o["synced"] = clockSynced();
  { String ts = clockTimeStr(); if (ts.length()) o["time"] = ts; }
  o["tz"]        = S->clock.tz;
  o["night"]     = clockNightActive();   // dimming now
  o["nightHeld"] = clockNightHeld();      // in the window but waiting for a fresh NTP sync
  o["clockFresh"] = clockTrusted();       // last NTP sync within the trust window
  sendJson(doc);
}

// Fingerprint of everything network-identity related: the WiFi list and the
// hostname. Changing any of it needs a reboot, because the connection and the
// mDNS registration are established once at boot.
static String netFingerprint(const Settings& s) {
  String f((int)s.wifiCount);
  for (uint8_t i = 0; i < s.wifiCount; i++) {
    f += '\n';
    f += s.wifi[i].ssid;
    f += '\x01';
    f += s.wifi[i].pass;
  }
  f += '\n';
  f += s.hostname;
  return f;
}

static void handlePostConfig() {
  if (!requireAuth()) return;
  if (!server.hasArg("plain")) { server.send(400, "text/plain", "no body"); return; }

  JsonDocument doc;
  if (deserializeJson(doc, server.arg("plain"))) {
    server.send(400, "text/plain", "bad json");
    return;
  }

  String oldNet = netFingerprint(*S);

  if (!applyAndSaveSettings(*S,
        [&](Settings& settings) { settingsApplyJson(settings, doc.as<JsonObjectConst>()); },
        [](const Settings& settings) { return saveSettings(settings); })) {
    server.send(500, "application/json", "{\"ok\":false,\"error\":\"could not save settings\"}");
    return;
  }

  // Live apply (no reboot needed for these)
  clockReapply(*S);         // re-arm SNTP iff the timezone changed
  appApplyBrightness();     // apply effective brightness (respects night/auto/manual)
  gfxApplyColors(*S);       // rotation, panel colour order/inversion, channel gain
  appInvalidate();          // re-init every mode + repaint (covers mode/URL/symbol changes)

  bool wifiChanged = netFingerprint(*S) != oldNet;

  JsonDocument res;
  res["ok"] = true;
  res["reboot"] = wifiChanged;
  sendJson(res);

  if (wifiChanged) scheduleReboot(800);
}

static void handleScan() {
  if (!requireAuth()) return;
  int n = WiFi.scanNetworks();
  JsonDocument doc;
  JsonArray arr = doc.to<JsonArray>();
  for (int i = 0; i < n && i < 25; i++) {
    JsonObject o = arr.add<JsonObject>();
    o["ssid"] = WiFi.SSID(i);
    o["rssi"] = WiFi.RSSI(i);
    o["enc"] = !platformScanIsOpen(i);
  }
  WiFi.scanDelete();
  sendJson(doc);
}

static void handleReboot() {
  if (!requireAuth()) return;
  server.send(200, "application/json", "{\"ok\":true}");
  scheduleReboot(400);
}

static void handleFactory() {
  if (!requireAuth()) return;
  factoryReset(*S);
  saveSettings(*S);
  server.send(200, "application/json", "{\"ok\":true}");
  scheduleReboot(400);
}

// Full settings backup: stream the persisted config.json verbatim. It includes
// the WiFi passwords — same trust domain as typing them into this page.
static void handleExport() {
  if (!requireAuth()) return;
  File f = LittleFS.open("/config.json", "r");
  if (!f) { server.send(404, "text/plain", "no config saved yet"); return; }
  server.sendHeader("Content-Disposition", "attachment; filename=clawd-config.json");
  server.streamFile(f, "application/json");
  f.close();
}

// Restore a backup: apply everything, persist, reboot (WiFi/hostname may change).
static void handleImport() {
  if (!requireAuth()) return;
  if (!server.hasArg("plain")) { server.send(400, "text/plain", "no body"); return; }
  JsonDocument doc;
  if (deserializeJson(doc, server.arg("plain"))) {
    server.send(400, "text/plain", "bad json");
    return;
  }
  if (!applyAndSaveSettings(*S,
        [&](Settings& settings) { settingsApplyJson(settings, doc.as<JsonObjectConst>()); },
        [](const Settings& settings) { return saveSettings(settings); })) {
    server.send(500, "application/json", "{\"ok\":false,\"error\":\"could not save settings\"}");
    return;
  }
  server.send(200, "application/json", "{\"ok\":true,\"reboot\":true}");
  scheduleReboot(800);
}

// Push endpoint: the daemon POSTs the usage payload here when the device can't
// reach it (Wi-Fi client isolation). Body is the {s,sr,w,wr,st,ok} contract.
static void handleUsagePush() {
  if (!server.hasArg("plain")) { server.send(400, "text/plain", "no body"); return; }
  bool ok = usageApply(server.arg("plain"));
  // Each push is a full JSON parse + possibly a full repaint: worth tracking.
  uint32_t h = ESP.getFreeHeap();
  if (h < g_minHeap) g_minHeap = h;
  server.send(ok ? 200 : 400, "application/json",
              ok ? "{\"ok\":true}" : "{\"ok\":false}");
}

// Screen state push (clawd-home daemon): full-screen desk animation, text line,
// countdown, brightness cap. Open like /api/usage: the daemon on the LAN sends
// it every few seconds and it can only change what the clock shows, for ttl.
static void handleStatePush() {
  if (!server.hasArg("plain")) { server.send(400, "text/plain", "no body"); return; }
  bool ok = usageApplyState(server.arg("plain"));
  uint32_t h = ESP.getFreeHeap();
  if (h < g_minHeap) g_minHeap = h;
  server.send(ok ? 200 : 400, "application/json", ok ? "{\"ok\":true}" : "{\"ok\":false}");
}

// Heap watch for long-run stability checks: numbers only, nothing settable, so
// it is open like /api/usage and a logger can poll it without credentials.
//   while sleep 60; do curl -s http://<ip>/api/heap; echo; done >> heap.log
static void handleHeap() {
  {   // sample now too, so the first request after boot never reports UINT32_MAX
    uint32_t h = ESP.getFreeHeap(), b = platformMaxFreeBlock();
    if (h < g_minHeap) g_minHeap = h;
    if (b < g_minMaxBlk) g_minMaxBlk = b;
  }
  JsonDocument doc;
  JsonObject o = doc.to<JsonObject>();
  o["heap"] = ESP.getFreeHeap();
  o["maxblk"] = platformMaxFreeBlock();
  o["minHeap"] = g_minHeap;
  o["minMaxblk"] = g_minMaxBlk;
#if defined(CLAWD_ESP8266)
  o["frag"] = ESP.getHeapFragmentation();
  o["contFree"] = ESP.getFreeContStack();   // loop() stack never used yet (low-water mark)
#endif
  o["uptime"] = millis() / 1000;
  o["loopMaxMs"] = appLoopMaxMs();     // > ~100 ms: something blocked the animations
  o["reset"] = appResetReason();
  if (crashLogLast().length()) o["crash"] = crashLogLast();   // last crash since power on
  o["version"] = FW_VERSION;
  o["variant"] = FW_VARIANT;
  o["fwSeq"] = CLAWD_FW_SEQ;          // the desktop app never pushes an image with a lower one
  o["layouts"] = CLAWD_LAYOUTS;        // usage screen layouts this build draws (bitmask)
  o["styleApi"] = STYLE_API;           // style packs this build can draw (docs/STYLE_PACK.md)
  o["style"] = styleInfo() ? styleInfo()->id : "";   // installed pack, "" = classic screen
  o["styleFrameUs"] = styleLastFrameUs();            // last animation repaint (decode + SPI)
  o["styleFrameMaxUs"] = styleMaxFrameUs();
  o["pak"] = clawdPackCount();          // 0 = /clawd.pak missing or invalid
  o["splash"] = clawdUiOnSplash();
  o["anim"] = clawdUiAnimName();
  o["desk"] = deskPackCount();          // 0 = /desk.pak missing or invalid
  o["onDesk"] = clawdUiOnDesk();
  o["deskAnim"] = clawdUiDeskAnimName();   // what the desk player shows now (variants too)
  {   // desk player: decode + SPI time per frame and lateness, last 128 frames (us)
    ClawdDeskStats ds;
    clawdUiDeskStats(ds);
    JsonObject d = o["deskStats"].to<JsonObject>();
    d["n"] = ds.frames;
    d["total"] = ds.total;
    d["drawP50"] = ds.drawP50;
    d["drawP95"] = ds.drawP95;
    d["drawMax"] = ds.drawMax;
    d["lateP95"] = ds.lateP95;
  }
#if defined(CLAWD_ESP8266)
  {
    FSInfo fi;
    if (LittleFS.info(fi)) { o["fsTotal"] = fi.totalBytes; o["fsUsed"] = fi.usedBytes; }
  }
#endif
  sendJson(doc);
}

// Replace the Clawd animation pack (/clawd.pak, tools/clawd_assets.js). The
// pack normally goes on through the stock firmware's /upload before this image
// is flashed; this is the in-place way to update it. Staged to a temp file so
// a failed upload never leaves a half pack behind.
//   curl -F "file=@data/clawd.pak" http://<ip>/api/clawdpak
static File g_pakUpload;
static bool g_pakFailed = false;
static const char* g_pakTmp = "/clawd.pak.tmp";
static void pakUpload();
static void handleClawdPakUpload() { g_pakTmp = "/clawd.pak.tmp"; pakUpload(); }
// The desk pack takes most of the free space (the stock firmware's assets stay
// on the filesystem), so the old and new packs never fit side by side: the old
// one goes before the new one streams in, and the desk view shows
// "desk.pak missing" until the upload completes.
static void handleDeskPakUpload() {
  g_pakTmp = "/desk.pak.tmp";
  if (server.upload().status == UPLOAD_FILE_START &&
      (!S->auth.enabled || !S->auth.pass.length() ||
       server.authenticate(S->auth.user.c_str(), S->auth.pass.c_str()))) {
    clawdUiDeskReleased();
    LittleFS.remove(DESK_PACK_PATH);
    LittleFS.remove(g_pakTmp);
  }
  pakUpload();
}
static void pakUpload() {
  HTTPUpload& up = server.upload();
  // Checked while the body streams in too (like /update), or an unauthenticated
  // POST could still write the file and only lose the final answer.
  if (S->auth.enabled && S->auth.pass.length() &&
      !server.authenticate(S->auth.user.c_str(), S->auth.pass.c_str())) {
    g_pakFailed = true;
    if (g_pakUpload) g_pakUpload.close();
    return;
  }
  if (up.status == UPLOAD_FILE_START) {
    g_pakFailed = false;
    g_pakUpload = LittleFS.open(g_pakTmp, "w");
    if (!g_pakUpload) g_pakFailed = true;
  } else if (up.status == UPLOAD_FILE_WRITE) {
    if (g_pakUpload && g_pakUpload.write(up.buf, up.currentSize) != up.currentSize) g_pakFailed = true;
  } else if (up.status == UPLOAD_FILE_END || up.status == UPLOAD_FILE_ABORTED) {
    if (g_pakUpload) g_pakUpload.close();
    if (up.status == UPLOAD_FILE_ABORTED) g_pakFailed = true;
  }
  yield();
}
static void handleClawdPakDone() {
  if (!requireAuth()) return;
  bool ok = !g_pakFailed;
  if (ok) {
    LittleFS.remove(CLAWD_PACK_PATH);
    ok = LittleFS.rename("/clawd.pak.tmp", CLAWD_PACK_PATH) && clawdPackOpen();
    clawdUiBegin();     // re-read descriptors, restart the screens
  } else {
    LittleFS.remove("/clawd.pak.tmp");
  }
  JsonDocument doc;
  doc["ok"] = ok;
  doc["anims"] = clawdPackCount();
  sendJson(doc, ok ? 200 : 500);
}
// Same for the desk animations (/desk.pak, clawd-home tools/desk):
//   curl -F "file=@build/desk/desk.pak" http://<ip>/api/deskpak
static void handleDeskPakDone() {
  if (!requireAuth()) return;
  bool ok = !g_pakFailed;
  if (ok) {
    LittleFS.remove(DESK_PACK_PATH);
    ok = LittleFS.rename("/desk.pak.tmp", DESK_PACK_PATH) && deskPackOpen();
    clawdUiBegin();
  } else {
    LittleFS.remove("/desk.pak.tmp");
  }
  JsonDocument doc;
  doc["ok"] = ok;
  doc["anims"] = deskPackCount();
  sendJson(doc, ok ? 200 : 500);
}

// Style packs (docs/STYLE_PACK.md). The desktop app keeps every pack and sends
// only the one in use; the clock switches to it at once, no reboot.
//   curl -F "file=@styles/synth.pak" http://<ip>/api/style    install and show it
//   curl -X DELETE http://<ip>/api/style                       back to the classic screen
//   curl http://<ip>/api/style                                 what is installed (open)
static void styleJson(JsonObject o) {
  const StyleInfo* i = styleInfo();
  o["api"] = STYLE_API;
  o["active"] = i != nullptr;
  o["slot"] = styleStoreSlotBytes();
  if (styleStoreError()[0]) o["error"] = styleStoreError();
  if (!i) return;
  char crc[9];
  snprintf(crc, sizeof(crc), "%08x", (unsigned)i->crc);
  o["id"] = i->id;
  o["name"] = i->name;
  o["size"] = i->size;
  o["crc"] = crc;
}
static void handleStyleGet() {
  JsonDocument doc;
  styleJson(doc.to<JsonObject>());
  sendJson(doc);
}
static void handleStyleUpload() {
  g_pakTmp = STYLE_TMP_PATH;
  HTTPUpload& up = server.upload();
  if (up.status == UPLOAD_FILE_START &&
      (!S->auth.enabled || !S->auth.pass.length() ||
       server.authenticate(S->auth.user.c_str(), S->auth.pass.c_str()))) {
    LittleFS.remove(STYLE_TMP_PATH);
#if defined(CLAWD_ESP8266)
    // Short of room: drop the stored copy first. The screen keeps drawing the
    // old pack from its flash mirror until the new one is in.
    FSInfo fi;
    if (LittleFS.info(fi) && fi.totalBytes - fi.usedBytes < 2u * STYLE_MAX_BYTES) LittleFS.remove(STYLE_PACK_PATH);
#endif
  }
  if (up.status == UPLOAD_FILE_WRITE && up.totalSize + up.currentSize > STYLE_MAX_BYTES) {
    g_pakFailed = true;                   // too big for a pack: stop writing it
    if (g_pakUpload) g_pakUpload.close();
    return;
  }
  pakUpload();
}
static void handleStyleDone() {
  if (!requireAuth()) return;
  const char* why = g_pakFailed ? "upload failed: too big, or no room on the clock" : styleStoreInstall();
  if (g_pakFailed) LittleFS.remove(STYLE_TMP_PATH);
  JsonDocument doc;
  doc["ok"] = why == nullptr;
  if (why) doc["error"] = why;
  styleJson(doc["style"].to<JsonObject>());
  sendJson(doc, why ? 400 : 200);
}
static void handleStyleClear() {
  if (!requireAuth()) return;
  styleStoreClear();
  JsonDocument doc;
  doc["ok"] = true;
  styleJson(doc["style"].to<JsonObject>());
  sendJson(doc);
}

#if defined(CLAWD_ESP8266)
// Filesystem maintenance for the shared LittleFS (the stock firmware's assets +
// ours), driven by clawd-home tools/stock_fs.sh: back the stock files up to the
// Mac, free their space, and put them back before a return to stock. All of it
// needs the web password when one is set (config.json holds WiFi credentials).
//   GET  /api/fs/list            "<size>\t<path>\n" for every file
//   GET  /api/fs/get?path=/x     one file
//   GET  /api/fs/raw             the whole LittleFS partition, byte for byte
//   POST /api/fs/put?path=/x     multipart upload to that path (dirs created)
//   POST /api/fs/rm?path=/x      delete one file (empty dirs go with it)
#include <flash_hal.h>
static void fsListDir(const String& dir, uint8_t depth) {
  Dir d = LittleFS.openDir(dir);
  while (d.next()) {
    const String p = (dir == "/" ? String("/") : dir + "/") + d.fileName();
    if (d.isDirectory()) {
      if (depth < 8) fsListDir(p, depth + 1);
    } else {
      server.sendContent(String(d.fileSize()) + "\t" + p + "\n");
    }
    yield();
  }
}
static void handleFsList() {
  if (!requireAuth()) return;
  server.setContentLength(CONTENT_LENGTH_UNKNOWN);
  server.send(200, "text/plain", "");
  fsListDir("/", 0);
  server.sendContent("");
}
static bool fsPathArg(String& p) {
  p = server.arg("path");
  if (!p.startsWith("/") || p.indexOf("..") >= 0 || p.length() > 64) {
    server.send(400, "text/plain", "bad path");
    return false;
  }
  return true;
}
static void handleFsGet() {
  if (!requireAuth()) return;
  String p;
  if (!fsPathArg(p)) return;
  File f = LittleFS.open(p, "r");
  if (!f || f.isDirectory()) { server.send(404, "text/plain", "no such file"); return; }
  server.streamFile(f, "application/octet-stream");
  f.close();
}
static void handleFsRaw() {
  if (!requireAuth()) return;
  const uint32_t start = FS_PHYS_ADDR, size = FS_PHYS_SIZE;
  server.setContentLength(size);
  server.send(200, "application/octet-stream", "");
  uint32_t buf[128];
  for (uint32_t off = 0; off < size; off += sizeof(buf)) {
    const uint32_t n = size - off < sizeof(buf) ? size - off : sizeof(buf);
    if (!ESP.flashRead(start + off, buf, n)) break;
    if (server.client().write((const uint8_t*)buf, n) != n) break;
    yield();
  }
}
static File g_fsPut;
static bool g_fsPutFailed = false;
static void handleFsPutUpload() {
  HTTPUpload& up = server.upload();
  if (S->auth.enabled && S->auth.pass.length() &&
      !server.authenticate(S->auth.user.c_str(), S->auth.pass.c_str())) {
    g_fsPutFailed = true;
    if (g_fsPut) g_fsPut.close();
    return;
  }
  if (up.status == UPLOAD_FILE_START) {
    String p = server.arg("path");
    g_fsPutFailed = !p.startsWith("/") || p.indexOf("..") >= 0 || p.length() > 64;
    if (!g_fsPutFailed) {
      g_fsPut = LittleFS.open(p, "w");
      if (!g_fsPut) g_fsPutFailed = true;
    }
  } else if (up.status == UPLOAD_FILE_WRITE) {
    if (g_fsPut && g_fsPut.write(up.buf, up.currentSize) != up.currentSize) g_fsPutFailed = true;
  } else if (up.status == UPLOAD_FILE_END || up.status == UPLOAD_FILE_ABORTED) {
    if (g_fsPut) g_fsPut.close();
    if (up.status == UPLOAD_FILE_ABORTED) g_fsPutFailed = true;
  }
  yield();
}
static void handleFsPutDone() {
  if (!requireAuth()) return;
  String p;
  if (!fsPathArg(p)) return;
  if (g_fsPutFailed) LittleFS.remove(p);
  File f = LittleFS.open(p, "r");
  JsonDocument doc;
  doc["ok"] = !g_fsPutFailed && f;
  doc["size"] = f ? (uint32_t)f.size() : 0;
  if (f) f.close();
  sendJson(doc, g_fsPutFailed ? 500 : 200);
}
static void handleFsRm() {
  if (!requireAuth()) return;
  String p;
  if (!fsPathArg(p)) return;
  if (p == DESK_PACK_PATH) clawdUiDeskReleased();
  const bool ok = LittleFS.remove(p);
  JsonDocument doc;
  doc["ok"] = ok;
  sendJson(doc, ok ? 200 : 404);
}
#endif

// Attention overlay. Every field is optional and falls back to the preset named
// by "type" (itself defaulting to "info"), so the smallest useful request is
// {"label":"..."} and the largest overrides the lot:
//   {"type":"alert","title":"NAS","label":"<text>","anim":"bang",
//    "color":"#3399ff","ttl":<seconds>,"priority":0..3}
// Never persisted. Behind the password like the rest of the API: unlike the
// daemon's usage push, whatever fires these is a script of your own and can
// send credentials, and taking over the whole screen is not something to leave
// open on a device you deliberately locked.

// ---- OTA ------------------------------------------------------------------
static void handleUpdateDone() {
  if (!requireAuth()) return;
  bool ok = !Update.hasError();
  server.sendHeader("Connection", "close");
  server.send(ok ? 200 : 500, "text/plain", ok ? "OK" : platformUpdateError().c_str());
  if (ok) scheduleReboot(1200);
  else styleStoreResume();            // the image did not take: the style slot is ours again
}

// The upload callback runs while the body streams in, ahead of
// handleUpdateDone, so the password has to be checked here too: guarding only
// the final handler would let an unauthenticated POST write a whole image to
// flash and merely lose the 200 at the end.
static void handleUpdateUpload() {
  HTTPUpload& up = server.upload();
  if (S->auth.enabled && S->auth.pass.length() &&
      !server.authenticate(S->auth.user.c_str(), S->auth.pass.c_str())) {
    if (up.status != UPLOAD_FILE_START) Update.end();
    return;
  }
  if (up.status == UPLOAD_FILE_START) {
#if defined(CLAWD_ESP8266)
    WiFiUDP::stopAll();   // free UDP sockets so the OTA has max contiguous flash/heap
#endif
    styleStoreSuspend();  // the new image is staged over the style pack's flash mirror
    // The whole free slot: Updater::begin already refuses an image that would
    // overlap the running one. The usual extra 4 KB slack made a ~520 KB image
    // unable to replace itself in the 4m3m 1 MB sketch area.
    uint32_t maxSpace = ESP.getFreeSketchSpace() & 0xFFFFF000;
    if (!Update.begin(maxSpace)) Update.printError(Serial);
  } else if (up.status == UPLOAD_FILE_WRITE) {
    if (Update.write(up.buf, up.currentSize) != up.currentSize) Update.printError(Serial);
  } else if (up.status == UPLOAD_FILE_END) {
    if (!Update.end(true)) Update.printError(Serial);
  } else if (up.status == UPLOAD_FILE_ABORTED) {
    Update.end();
    styleStoreResume();   // no final handler runs after an abort
  }
  yield();
}

// ---- captive portal -------------------------------------------------------
static void handleNotFound() {
  if (netMode() == NET_AP) {
    // Redirect everything to the config page so the captive portal pops.
    server.sendHeader("Location", String("http://") + WiFi.softAPIP().toString(), true);
    server.send(302, "text/plain", "");
  } else {
    server.send(404, "text/plain", "Not found");
  }
}

// ---------------------------------------------------------------------------
void webPortalBegin(Settings& settings) {
  S = &settings;

  server.on("/", HTTP_GET, handleRoot);
  server.on("/api/config", HTTP_GET, handleGetConfig);
  server.on("/api/config", HTTP_POST, handlePostConfig);
  server.on("/api/status", HTTP_GET, handleStatus);
  server.on("/api/scan", HTTP_GET, handleScan);
  server.on("/api/reboot", HTTP_POST, handleReboot);
  server.on("/api/factory", HTTP_POST, handleFactory);
  server.on("/api/export", HTTP_GET, handleExport);
  server.on("/api/import", HTTP_POST, handleImport);
  server.on("/api/usage", HTTP_POST, handleUsagePush);   // daemon pushes usage here
  server.on("/api/heap", HTTP_GET, handleHeap);          // unauthenticated heap watch
  server.on("/api/clawdpak", HTTP_POST, handleClawdPakDone, handleClawdPakUpload);
  server.on("/api/deskpak", HTTP_POST, handleDeskPakDone, handleDeskPakUpload);
  server.on("/api/style", HTTP_GET, handleStyleGet);
  server.on("/api/style", HTTP_POST, handleStyleDone, handleStyleUpload);
  server.on("/api/style", HTTP_DELETE, handleStyleClear);
#if defined(CLAWD_ESP8266)
  server.on("/api/fs/list", HTTP_GET, handleFsList);
  server.on("/api/fs/get", HTTP_GET, handleFsGet);
  server.on("/api/fs/raw", HTTP_GET, handleFsRaw);
  server.on("/api/fs/put", HTTP_POST, handleFsPutDone, handleFsPutUpload);
  server.on("/api/fs/rm", HTTP_POST, handleFsRm);
#endif
  server.on("/api/state", HTTP_POST, handleStatePush);    // daemon pushes screen state here
  server.on("/update", HTTP_POST, handleUpdateDone, handleUpdateUpload);

  // Common captive-portal probe endpoints
  server.on("/generate_204", handleNotFound);
  server.on("/gen_204", handleNotFound);
  server.on("/hotspot-detect.html", handleNotFound);
  server.on("/connecttest.txt", handleNotFound);
  server.onNotFound(handleNotFound);

  server.begin();
}

void webPortalLoop() {
  server.handleClient();
  {
    uint32_t h = ESP.getFreeHeap(), b = platformMaxFreeBlock();
    if (h < g_minHeap) g_minHeap = h;
    if (b < g_minMaxBlk) g_minMaxBlk = b;
  }
}

bool webPortalRebootDue() {
  return g_reboot && (int32_t)(millis() - g_rebootAt) >= 0;
}
