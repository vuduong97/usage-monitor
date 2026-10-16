// Platform.h — the single place that knows which chip we build for.
//
// It pulls in the arch-specific SDK headers and exposes a small, uniform surface
// (class aliases + inline shims) so the rest of the firmware stays chip-agnostic.
// Target is chosen by a build-time macro: -D CLAWD_ESP32C2 (ESP8684 knockoff),
// -D CLAWD_ESP32 (classic ESP32, NM-TV-154), or -D CLAWD_ESP8266 (original
// clock, the default). Both ESP32 targets share the Arduino core 3.x branch.
#pragma once
#include <Arduino.h>
#include <time.h>

#if defined(CLAWD_ESP32C2) || defined(CLAWD_ESP32)
// ================== ESP32 family (C2/ESP8684 + classic ESP32) ==================
#include <WiFi.h>
#include <WebServer.h>
#include <ESPmDNS.h>
#include <Update.h>
#include <esp_system.h>

using WebServerClass = WebServer;

static inline void platformSetHostname(const char* h) { WiFi.setHostname(h); }
static inline void platformTimeBegin(const char* tz, const char* s1, const char* s2) {
  configTzTime(tz, s1, s2);   // ESP32 core: sets TZ + starts SNTP
}
static inline void platformMdnsUpdate() { /* ESP32 mDNS answers from a background task */ }
static inline void platformAnalogWriteInit(uint8_t pin) { (void)pin; /* analogWrite defaults to 8-bit (0..255) */ }
static inline bool platformScanIsOpen(int i) { return WiFi.encryptionType(i) == WIFI_AUTH_OPEN; }
static inline String platformUpdateError() { return String(Update.errorString()); }
static inline uint32_t platformChipId() { return (uint32_t)ESP.getEfuseMac(); }

// Reset / crash info. The ESP32 Arduino API exposes no exception PC (on the C2
// or the classic ESP32), so epc/addr stay empty; panics/WDTs still flag crashes.
struct PlatformReset { String reason; bool wasCrash; char epc[16]; char addr[16]; };
static inline PlatformReset platformResetInfo() {
  PlatformReset r; r.wasCrash = false; r.epc[0] = 0; r.addr[0] = 0;
  switch (esp_reset_reason()) {
    case ESP_RST_POWERON:   r.reason = "Power on"; break;
    case ESP_RST_EXT:       r.reason = "External reset"; break;
    case ESP_RST_SW:        r.reason = "Software restart"; break;
    case ESP_RST_PANIC:     r.reason = "Exception/panic"; r.wasCrash = true; break;
    case ESP_RST_INT_WDT:   r.reason = "Interrupt watchdog"; r.wasCrash = true; break;
    case ESP_RST_TASK_WDT:  r.reason = "Task watchdog"; r.wasCrash = true; break;
    case ESP_RST_WDT:       r.reason = "Watchdog"; r.wasCrash = true; break;
    case ESP_RST_BROWNOUT:  r.reason = "Brownout"; break;
    case ESP_RST_DEEPSLEEP: r.reason = "Deep-sleep wake"; break;
    default:                r.reason = "Unknown"; break;
  }
  return r;
}

static inline uint32_t platformMaxFreeBlock() { return ESP.getMaxAllocHeap(); }
static inline uint32_t platformFreeContStack() { return 0; }   // N/A on ESP32

#else
// ================================= ESP8266 =================================
#include <ESP8266WiFi.h>
#include <ESP8266WebServer.h>
#include <ESP8266mDNS.h>
#include <Updater.h>
#include <WiFiUdp.h>
extern "C" {
#include <user_interface.h>   // struct rst_info + REASON_* (reset cause / crash PC)
}

using WebServerClass = ESP8266WebServer;

static inline void platformSetHostname(const char* h) { WiFi.hostname(h); }
static inline void platformTimeBegin(const char* tz, const char* s1, const char* s2) {
  configTime(tz, s1, s2);     // ESP8266 core: TZ-string overload, sets TZ + starts SNTP
}
static inline void platformMdnsUpdate() { MDNS.update(); }
static inline void platformAnalogWriteInit(uint8_t pin) { (void)pin; analogWriteRange(255); }
static inline bool platformScanIsOpen(int i) { return WiFi.encryptionType(i) == ENC_TYPE_NONE; }
static inline String platformUpdateError() { return Update.getErrorString(); }
static inline uint32_t platformChipId() { return ESP.getChipId(); }

// Reset / crash info from the ESP8266 boot ROM (Xtensa exception PC + fault addr).
struct PlatformReset { String reason; bool wasCrash; char epc[16]; char addr[16]; };
static inline PlatformReset platformResetInfo() {
  PlatformReset r; r.wasCrash = false; r.epc[0] = 0; r.addr[0] = 0;
  r.reason = ESP.getResetReason();
  struct rst_info* ri = ESP.getResetInfoPtr();
  if (ri && ri->reason == REASON_EXCEPTION_RST) {
    r.wasCrash = true;
    snprintf(r.epc,  sizeof(r.epc),  "0x%08x", (unsigned)ri->epc1);
    snprintf(r.addr, sizeof(r.addr), "0x%08x", (unsigned)ri->excvaddr);
  }
  return r;
}

// Heap diagnostics (largest contiguous free block, and the separate primary
// "cont" stack headroom), reported by /api/heap.
static inline uint32_t platformMaxFreeBlock() { return ESP.getMaxFreeBlockSize(); }
static inline uint32_t platformFreeContStack() { return ESP.getFreeContStack(); }

#endif

// ---- common: wall-clock time (SNTP) ---------------------------------------
// True once SNTP has set the clock (epoch past 2021-01-01). Until then the
// caller must treat time as unknown (night mode stays off = fail-safe on).
static inline bool platformTimeValid() { return time(nullptr) > 1609459200; }

// Register a callback fired on every successful SNTP sync (the "NTP was just
// reachable and set the clock" signal used to trust the clock for night mode).
// Defined per core in Platform.cpp. Call once, after platformTimeBegin.
void platformOnTimeSync(void (*cb)());
