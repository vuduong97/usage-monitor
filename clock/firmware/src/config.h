// config.h — compile-time constants for the Clawd desk clock firmware
//
// Hardware: the JUZIPi 1.54" desk clock, ESP-12F (ESP8266) with a 1.54" 240x240 ST7789
// IPS panel [board_esp8266.h]. The board headers for the ESP32 family
// (board_esp32*.h) are kept so a port stays possible, but nothing builds them.
// The board-specific pin map + panel quirks live in the board headers, selected
// below by the build-time target macro. Everything else here is shared.
#pragma once

// ---------------------------------------------------------------------------
// Firmware identity
// ---------------------------------------------------------------------------
#define FW_NAME     "clawd"
#define FW_VERSION  "2.16.0"

// /api/heap "variant": the desktop app matches on it to recognise this firmware.
#define FW_VARIANT  "esp8266-clawd"

// Bumped by hand whenever a build adds something the desktop app gates on
// (style packs, endpoints). The app only pushes its bundled image to a clock
// with a lower number, so it never downgrades one. 0 = before this field.
#define CLAWD_FW_SEQ 8               // 8: style pack API 2 (Clawd animations in the packs)

// ---------------------------------------------------------------------------
// Display wiring + panel quirks — board-specific, pulled from the right header.
// Provides TFT_SCLK/MOSI/DC/RST/CS/BL, TFT_BGR, TFT_BL_DEFAULT_INVERTED,
// HAS_LDR/LDR_PIN/ADC_MAX. Both panels are 1.54" 240x240 ST7789 IPS.
// ---------------------------------------------------------------------------
#if defined(CLAWD_ESP32C2)
  #include "board_esp32c2.h"
#elif defined(CLAWD_ESP32_PRO)
  #include "board_esp32_pro.h"
#elif defined(CLAWD_ESP32)
  #include "board_esp32.h"
#else
  #include "board_esp8266.h"
#endif

#define TFT_WIDTH  240
#define TFT_HEIGHT 240

// Panel RAM offsets per rotation pair (Arduino_GFX: offset1 -> rotation 0/1,
// offset2 -> rotation 2/3). The ST7789(V) is a 240x320 controller but the
// 1.54" glass only wires RAM rows 0-239, leaving an 80-row dead band. With
// the MADCTL MY bit set (rotation 2/3) the scan direction reverses into that
// dead band, so the row offset must jump to 80 or the image slides 80 px off
// the glass (content pushed to the top at 180°). #ifndef so a board header
// can override if a variant turns up with a 240x280 panel (would need 40).
#ifndef TFT_COL_OFFSET1
#define TFT_COL_OFFSET1 0
#endif
#ifndef TFT_ROW_OFFSET1
#define TFT_ROW_OFFSET1 0
#endif
#ifndef TFT_COL_OFFSET2
#define TFT_COL_OFFSET2 0
#endif
#ifndef TFT_ROW_OFFSET2
#define TFT_ROW_OFFSET2 80
#endif

// ---------------------------------------------------------------------------
// Limits (bound RAM usage on the ESP8266)
// ---------------------------------------------------------------------------
#define MAX_WIFI_NETS     4    // saved WiFi networks; strongest visible wins at boot

// ---------------------------------------------------------------------------
// Web UI password (off by default). Digest auth, so the password itself is
// never sent over the wire even though the UI is plain HTTP.
// ---------------------------------------------------------------------------
#define MAX_AUTH_USER_LEN 32
#define MAX_AUTH_PASS_LEN 64
#define DEFAULT_AUTH_USER "admin"
#define AUTH_REALM        "Clawd"

// ---------------------------------------------------------------------------
// Defaults (used on first boot / factory reset)
// ---------------------------------------------------------------------------
#define DEFAULT_AP_SSID      "Clawd-Setup"
#define DEFAULT_AP_PASS      ""              // empty => open AP
#define DEFAULT_HOSTNAME     "clawd"
#define DEFAULT_BRIGHTNESS    90             // 0..100 %

// --- Panel colour correction (device-wide) ---
// Panels differ between (and within) the clock variants: white balance drifts
// and some controllers have red and blue swapped. AUTO keeps the board header's
// TFT_BGR default; RGB/BGR force the MADCTL colour-order bit either way.
#define COLOR_ORDER_AUTO   0
#define COLOR_ORDER_RGB    1
#define COLOR_ORDER_BGR    2
#define DEFAULT_COLOR_ORDER  COLOR_ORDER_AUTO
#define DEFAULT_COLOR_INVERT false
#define DEFAULT_COLOR_GAIN   100     // percent per channel; 50..150 accepted
#define MIN_COLOR_GAIN        50
#define MAX_COLOR_GAIN       150

// --- Clock / night mode (device-wide) ---
#define NTP_SERVER1             "pool.ntp.org"
#define NTP_SERVER2             "time.nist.gov"
#define DEFAULT_TZ_NAME         ""        // IANA display name; empty = UTC
#define DEFAULT_TZ_POSIX        "UTC0"    // POSIX TZ rule the device feeds SNTP
#define DEFAULT_NIGHT_ENABLED   false
#define DEFAULT_NIGHT_START_MIN 1320      // 22:00
#define DEFAULT_NIGHT_END_MIN   420       // 07:00
#define DEFAULT_NIGHT_LEVEL     0         // 0..100, 0 = backlight fully off

// Night-mode NTP trust: only ENTER night mode when the clock was confirmed by a
// successful NTP sync within NIGHT_NTP_TRUST_MS (else we assume the clock may be
// wrong and keep the screen on). While inside the window but unconfirmed, re-arm
// SNTP every NIGHT_NTP_RESYNC_MS until a fresh sync lands or the window ends
// (morning). Once night mode has switched on, it stays on until the window ends.
#define NIGHT_NTP_TRUST_MS      300000UL  // 5 min: max age of the sync that unlocks night
#define NIGHT_NTP_RESYNC_MS      30000UL  // re-sync attempt cadence while held off
