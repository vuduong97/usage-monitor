// ClawdUi.cpp — Clawdmeter's compact (240 px) usage screen + the splash, drawn
// with ClawdDraw. Layout numbers are Clawdmeter's compute_layout() small
// breakpoint, resolved to absolute pixels (see ui.cpp in the Clawdmeter repo).
#include "ClawdUi.h"
#include "ClawdAnim.h"
#include "ClawdDraw.h"
#include "ClawdRate.h"
#include "ClawdPack.h"
#include "DeskPack.h"
#include "clawd_fonts.h"
#include <stdio.h>
#include <string.h>
#include <time.h>

// ---- layout (240x240) --------------------------------------------------------
#define MARGIN      8
#define CONTENT_W   (CLAWD_W - 2 * MARGIN)          // 224
#define PAD_X       10
#define PAD_Y       6
#define PANEL1_Y    44
#define PANEL_H     74
#define PANEL_GAP   6
#define PANEL2_Y    (PANEL1_Y + PANEL_H + PANEL_GAP) // 124
#define BAR_DY      30
#define BAR_H       12
#define RESET_DY    46
#define TITLE_Y     4
#define TITLE_NUDGE 8
#define TITLE_X0    64          // right of the mascot window
#define STATUS_Y    (CLAWD_H - 10 - 19)              // mono_18, bottom-aligned at -10
#define MASCOT_X    6
#define MASCOT_Y    2
#define IDLE_CY     (PANEL1_Y + (CLAWD_H - PANEL1_Y) / 2 - 20)
#define IDLE_PX     96
#define FRAME_W     2           // alert border

#define ALERT_TIMEOUT_MS 120000UL
#define ANIM_MSG_MS      4000UL

#define DESK_CELL        (CLAWD_W / DESK_GRID)          // 4 px
#define DESK_TEXT_ROWS   52                             // grid rows shown when a text line shows:
                                                        // the bottom 52 (feet kept, bubble tops lose 8)
#define STATE_TTL_MAX    3600

enum View : int8_t { VIEW_NONE = -1, VIEW_SPLASH, VIEW_IDLE, VIEW_USAGE, VIEW_DESK };

static ClawdSplash g_splash;
static ClawdMascot g_mascot;
static ClawdMini   g_mini;

static ClawdData g_data;
static bool      g_received = false;     // any ok payload since boot
static bool      g_lastOk = false;
static uint32_t  g_lastDataMs = 0;
static uint32_t  g_firstDataMs = 0;
static int       g_rateGroup = 0;

static long      g_clockEpoch = 0;
static uint32_t  g_clockBaseMs = 0;
static int       g_clockFmt = 24;
static int       g_clockMin = -1;

static bool      g_alert = false;
static char      g_alertMsg[128];
static uint32_t  g_alertSince = 0;
static bool      g_frameOn = false;

// desk view (POST /api/state)
static bool      g_deskOn = false;
static uint32_t  g_deskUntil = 0, g_deskSince = 0;
static char      g_deskWant[16] = "";
static char      g_deskText[96] = "";
static bool      g_cdOn = false;
static uint32_t  g_cdEndMs = 0;
static int16_t   g_dim = -1;
static uint32_t  g_dimUntil = 0;
static bool      g_deskDirty = true;      // anim/text changed: full desk repaint

struct DeskPlayer {
  int16_t  index = -1;
  DeskAnim a;
  uint16_t frame = 0, hold = 100;
  uint32_t frameStart = 0;
  bool     loaded = false;
};
static DeskPlayer g_desk;

static View      g_view = VIEW_NONE;
static bool      g_dirtyAll = true;
static bool      g_dirtyPanels = true;
static bool      g_dirtyTitle = true;

// status line animation (Clawdmeter's whimsical spinner)
static uint32_t g_spinLast = 0, g_msgStart = 0;
static uint8_t  g_spinIdx = 0, g_spinPhase = 0, g_msgIdx = 0;
static char     g_statusDrawn[96] = "";

static const char* const SPINNER[] = {
    "\xC2\xB7", "\xE2\x9C\xBB", "\xE2\x9C\xBD", "\xE2\x9C\xB6", "\xE2\x9C\xB3", "\xE2\x9C\xA2",
};
#define SPINNER_COUNT 6
#define SPINNER_PHASES (2 * (SPINNER_COUNT - 1))
static const uint16_t SPINNER_MS[SPINNER_COUNT] = {260, 130, 130, 130, 130, 260};

// One NUL-separated blob in flash: ~1.3 KB that would otherwise be DRAM
// (string literals + a pointer table) on the ESP8266.
static const char MESSAGES[] PROGMEM =
    "Accomplishing\0Elucidating\0Perusing\0Actioning\0Enchanting\0Philosophising\0"
    "Actualizing\0Envisioning\0Pondering\0Baking\0Finagling\0Pontificating\0Booping\0"
    "Flibbertigibbeting\0Processing\0Brewing\0Forging\0Puttering\0Calculating\0Forming\0"
    "Puzzling\0Cerebrating\0Frolicking\0Reticulating\0Channelling\0Generating\0Ruminating\0"
    "Churning\0Germinating\0Scheming\0Clauding\0Hatching\0Schlepping\0Coalescing\0Herding\0"
    "Shimmying\0Cogitating\0Honking\0Shucking\0Combobulating\0Hustling\0Simmering\0"
    "Computing\0Ideating\0Smooshing\0Concocting\0Imagining\0Spelunking\0Conjuring\0"
    "Incubating\0Spinning\0Considering\0Inferring\0Stewing\0Contemplating\0Jiving\0Sussing\0"
    "Cooking\0Manifesting\0Synthesizing\0Crafting\0Marinating\0Thinking\0Creating\0"
    "Meandering\0Tinkering\0Crunching\0Moseying\0Transmuting\0Deciphering\0Mulling\0"
    "Unfurling\0Deliberating\0Mustering\0Unravelling\0Determining\0Musing\0Vibing\0"
    "Discombobulating\0Noodling\0Wandering\0Divining\0Percolating\0Whirring\0Doing\0"
    "Wibbling\0Effecting\0Wizarding\0Working\0Wrangling\0";
#define MESSAGE_COUNT 90

static uint16_t pctColor(float pct) {
  if (pct >= 80.0f) return CLAWD_RED;
  if (pct >= 50.0f) return CLAWD_AMBER;
  return CLAWD_GREEN;
}

static void formatReset(int mins, char* buf, size_t n) {
  if (mins < 0)         snprintf(buf, n, "---");
  else if (mins < 60)   snprintf(buf, n, "Resets in %dm", mins);
  else if (mins < 1440) snprintf(buf, n, "Resets in %dh %dm", mins / 60, mins % 60);
  else                  snprintf(buf, n, "Resets in %dd %dh", mins / 1440, (mins % 1440) / 60);
}

static void fillRect(int x, int y, int w, int h, uint16_t c) {
  ClawdSurface* s = clawdSurface();
  if (s && w > 0 && h > 0) s->fill(x, y, w, h, s->tint(c));
}

static bool dataFresh(uint32_t now) {
  return g_received && g_lastOk && (now - g_lastDataMs) < CLAWD_DATA_FRESH_MS;
}

// ---- usage panels --------------------------------------------------------------
// One panel: card, "%" line, pill, bar, reset line — composed and pushed as one.
static void drawPanel(int y, const char* pill, float barPct, uint16_t barColor,
                      const ClawdText* extra, int extraCount) {
  const int cx = MARGIN + PAD_X, cy = y + PAD_Y, cw = CONTENT_W - 2 * PAD_X;
  const ClawdFont& pf = CF_STYRENE_14;
  const int pillW = clawdTextWidth(pf, pill) + 16;
  const int pillH = pf.lineHeight + 4;
  const int pillX = cx + cw - pillW;
  const int pillY = cy + 1;
  if (barPct < 0) barPct = 0;
  if (barPct > 100) barPct = 100;
  const int fillW = (int)(cw * barPct / 100.0f + 0.5f);

  ClawdShape shapes[4];
  int n = 0;
  shapes[n++] = {MARGIN, (int16_t)y, CONTENT_W, PANEL_H, 8, CLAWD_PANEL};
  shapes[n++] = {(int16_t)pillX, (int16_t)pillY, (int16_t)pillW, (int16_t)pillH, 99, CLAWD_BAR_BG};
  shapes[n++] = {cx, (int16_t)(cy + BAR_DY), (int16_t)cw, BAR_H, 6, CLAWD_BAR_BG};
  if (fillW > 0) shapes[n++] = {cx, (int16_t)(cy + BAR_DY), (int16_t)fillW, BAR_H, 6, barColor};

  ClawdText texts[6];
  int t = 0;
  texts[t++] = {&pf, pill, (int16_t)(pillX + 8), (int16_t)(pillY + 2), CLAWD_TEXT};
  for (int i = 0; i < extraCount && t < 6; i++) texts[t++] = extra[i];
  clawdRender(MARGIN, y, CONTENT_W, PANEL_H, CLAWD_BG, shapes, n, texts, t);
}

static void drawPanels() {
  const ClawdData& d = g_data;
  const int cx = MARGIN + PAD_X;
  char pct[16], reset[48], pct2[16], tail[24];
  const int s = (int)(d.sessionPct + 0.5f);

  if (d.enterprise) {
    // Spending: big Tiempos number + small "%", budget caption.
    snprintf(pct, sizeof(pct), "%d", s);
    const int numW = clawdTextWidth(CF_TIEMPOS_34, pct);
    ClawdText ex[3] = {
        {&CF_TIEMPOS_34, pct, (int16_t)cx, (int16_t)(PANEL1_Y + PAD_Y), CLAWD_TEXT},
        {&CF_STYRENE_14, "%", (int16_t)(cx + numW + 4), (int16_t)(PANEL1_Y + PAD_Y + 12), CLAWD_TEXT},
        {&CF_STYRENE_14, "of your monthly budget", (int16_t)cx, (int16_t)(PANEL1_Y + PAD_Y + RESET_DY), CLAWD_DIM},
    };
    drawPanel(PANEL1_Y, "Spending", d.sessionPct, pctColor(d.sessionPct), ex, 3);

    // Period: time elapsed, pace-coloured bar, "<pace> - Resets <date>".
    const char* pace = "Under pace";
    uint16_t paceColor = CLAWD_GREEN;
    if (d.sessionPct > d.timePct + 15.0f)      { pace = "Over pace"; paceColor = CLAWD_RED; }
    else if (d.sessionPct > d.timePct - 15.0f) { pace = "On pace";   paceColor = CLAWD_AMBER; }
    const uint16_t barPace = (d.sessionPct <= d.timePct) ? CLAWD_GREEN
                             : (d.sessionPct <= d.timePct + 15.0f) ? CLAWD_AMBER : CLAWD_RED;
    snprintf(pct2, sizeof(pct2), "%d%%", d.timePct);
    snprintf(tail, sizeof(tail), "Resets %.11s", d.resetDate);
    const int y = PANEL2_Y + PAD_Y + RESET_DY;
    const int w1 = clawdTextWidth(CF_STYRENE_14, pace);
    const int w2 = clawdTextWidth(CF_STYRENE_14, " - ");
    ClawdText ex2[4] = {
        {&CF_STYRENE_24, pct2, (int16_t)cx, (int16_t)(PANEL2_Y + PAD_Y), CLAWD_TEXT},
        {&CF_STYRENE_14, pace, (int16_t)cx, (int16_t)y, paceColor},
        {&CF_STYRENE_14, " - ", (int16_t)(cx + w1), (int16_t)y, CLAWD_DIM},
        {&CF_STYRENE_14, tail, (int16_t)(cx + w1 + w2), (int16_t)y, CLAWD_TEXT},
    };
    drawPanel(PANEL2_Y, "Period", (float)d.timePct, barPace, ex2, 4);
    return;
  }

  snprintf(pct, sizeof(pct), "%d%%", s);
  formatReset(d.sessionResetMin, reset, sizeof(reset));
  ClawdText ex[2] = {
      {&CF_STYRENE_24, pct, (int16_t)cx, (int16_t)(PANEL1_Y + PAD_Y), CLAWD_TEXT},
      {&CF_STYRENE_14, reset, (int16_t)cx, (int16_t)(PANEL1_Y + PAD_Y + RESET_DY), CLAWD_DIM},
  };
  drawPanel(PANEL1_Y, "Current", d.sessionPct, pctColor(d.sessionPct), ex, 2);

  const int w = (int)(d.weeklyPct + 0.5f);
  snprintf(pct2, sizeof(pct2), "%d%%", w);
  formatReset(d.weeklyResetMin, reset, sizeof(reset));
  ClawdText ex2[2] = {
      {&CF_STYRENE_24, pct2, (int16_t)cx, (int16_t)(PANEL2_Y + PAD_Y), CLAWD_TEXT},
      {&CF_STYRENE_14, reset, (int16_t)cx, (int16_t)(PANEL2_Y + PAD_Y + RESET_DY), CLAWD_DIM},
  };
  drawPanel(PANEL2_Y, "Weekly", d.weeklyPct, pctColor(d.weeklyPct), ex2, 2);
}

// ---- title / clock -----------------------------------------------------------------
static void titleText(uint32_t now, char* out, size_t n, int* minute) {
  *minute = -1;
  if (g_clockEpoch <= 0) { snprintf(out, n, "Usage"); return; }
  time_t cur = (time_t)(g_clockEpoch + (long)((now - g_clockBaseMs) / 1000));
  struct tm tmv;
  gmtime_r(&cur, &tmv);       // the epoch is already local wall-clock time
  *minute = tmv.tm_hour * 60 + tmv.tm_min;
  if (g_clockFmt == 12) {
    int h = tmv.tm_hour % 12;
    if (h == 0) h = 12;
    snprintf(out, n, "%d:%02d %s", h, tmv.tm_min, tmv.tm_hour < 12 ? "AM" : "PM");
  } else {
    snprintf(out, n, "%02d:%02d", tmv.tm_hour, tmv.tm_min);
  }
}

static void drawTitle(uint32_t now) {
  char buf[16];
  titleText(now, buf, sizeof(buf), &g_clockMin);
  const ClawdFont& f = CF_TIEMPOS_34;
  int x = CLAWD_W / 2 + TITLE_NUDGE - clawdTextWidth(f, buf) / 2;
  if (x < TITLE_X0) x = TITLE_X0;
  ClawdText t = {&f, buf, (int16_t)x, TITLE_Y, CLAWD_TEXT};
  clawdRender(TITLE_X0, FRAME_W, CLAWD_W - TITLE_X0 - FRAME_W, PANEL1_Y - FRAME_W,
              CLAWD_BG, nullptr, 0, &t, 1);
}

// ---- status line -------------------------------------------------------------------
static void drawStatus(const char* text, bool marquee, uint32_t now, uint32_t since) {
  const ClawdFont& f = CF_MONO_18;
  const int tw = clawdTextWidth(f, text);
  ClawdText runs[2];
  int n = 0;
  if (!marquee || tw <= CONTENT_W) {
    runs[n++] = {&f, text, (int16_t)(CLAWD_W / 2 - tw / 2), STATUS_Y, CLAWD_ACCENT};
  } else {
    const int gap = 48, period = tw + gap;
    const int off = (int)((now - since) * 40 / 1000 % (uint32_t)period);   // 40 px/s
    runs[n++] = {&f, text, (int16_t)(MARGIN - off), STATUS_Y, CLAWD_ACCENT};
    runs[n++] = {&f, text, (int16_t)(MARGIN - off + period), STATUS_Y, CLAWD_ACCENT};
  }
  clawdRender(MARGIN, STATUS_Y, CONTENT_W, f.lineHeight, CLAWD_BG, nullptr, 0, runs, n);
}

static void tickStatus(uint32_t now, bool force) {
  if (g_alert) {
    static uint32_t last = 0;
    if (!force && now - last < 50) return;
    last = now;
    drawStatus(g_alertMsg, true, now, g_alertSince);
    g_statusDrawn[0] = 0;
    return;
  }
  if (now - g_msgStart >= ANIM_MSG_MS) {
    g_msgIdx = (g_msgIdx + 1) % MESSAGE_COUNT;
    g_msgStart = now;
  }
  if (!force && now - g_spinLast < SPINNER_MS[g_spinIdx]) return;
  if (!force) {
    g_spinLast = now;
    g_spinPhase = (g_spinPhase + 1) % SPINNER_PHASES;
    g_spinIdx = (g_spinPhase < SPINNER_COUNT) ? g_spinPhase : (SPINNER_PHASES - g_spinPhase);
  }
  const char* word;
  if (!g_received)                     word = "Waiting";
  else if (!dataFresh(now))            word = (g_msgIdx & 1) ? "No data" : "Listening";
  else if (now - g_firstDataMs < 5000) word = "Connected";
  else {
    const char* p = MESSAGES;
    for (int i = 0; i < g_msgIdx; i++) while (pgm_read_byte(p++)) {}
    static char msg[24];
    size_t n = 0;
    for (char c; n < sizeof(msg) - 1 && (c = (char)pgm_read_byte(p + n)); n++) msg[n] = c;
    msg[n] = 0;
    word = msg;
  }
  char buf[96];
  snprintf(buf, sizeof(buf), "%s %s\xE2\x80\xA6", SPINNER[g_spinIdx], word);
  if (!force && strcmp(buf, g_statusDrawn) == 0) return;
  strcpy(g_statusDrawn, buf);
  drawStatus(buf, false, now, now);
}

// ---- desk view (full-screen clawd-on-desk animation) --------------------------------
static bool deskLive(uint32_t now) { return g_deskOn && (int32_t)(g_deskUntil - now) > 0; }

static void deskLoad(uint32_t now) {
  g_desk.loaded = deskPackFrame(g_desk.a, g_desk.frame, clawdFramePool(), CLAWD_FRAME_POOL);
  g_desk.hold = deskPackHold(g_desk.a, g_desk.frame);
  g_desk.frameStart = now;
}

static void deskStart(uint32_t now) {
  int idx = deskPackFind(g_deskWant);
  if (idx < 0 && deskPackCount() > 0) idx = 0;        // unknown name: still show Clawd
  g_desk.index = (idx >= 0 && deskPackAnim(idx, g_desk.a)) ? (int16_t)idx : (int16_t)-1;
  g_desk.frame = 0;
  g_desk.loaded = false;
  if (g_desk.index >= 0) deskLoad(now);
}

static uint8_t deskCell(void*, int gx, int gy) {
  const DeskAnim& a = g_desk.a;
  if (g_desk.index < 0 || !g_desk.loaded) return 0;
  const int x = gx - a.ox, y = gy - a.oy;
  if (x < 0 || y < 0 || x >= a.w || y >= a.h) return 0;
  const uint32_t i = (uint32_t)y * a.w + x;
  const uint8_t b = clawdFramePool()[i >> 1];
  return (i & 1) ? (b & 0x0F) : (b >> 4);
}

static void deskLine(uint32_t now, char* out, size_t n) {
  out[0] = 0;
  if (g_cdOn) {
    int32_t left = (int32_t)(g_cdEndMs - now);
    if (left < 0) left = 0;
    const int s = (int)((left + 999) / 1000);
    snprintf(out, n, "%s%s%02d:%02d", g_deskText, g_deskText[0] ? " " : "", s / 60, s % 60);
  } else {
    snprintf(out, n, "%s", g_deskText);
  }
}

static void deskTick(uint32_t now, bool force) {
  char line[112];
  deskLine(now, line, sizeof(line));
  const int rows = line[0] ? DESK_TEXT_ROWS : DESK_GRID;
  bool changed = force;
  if (g_desk.index >= 0 && now - g_desk.frameStart >= g_desk.hold) {
    g_desk.frame = (uint16_t)((g_desk.frame + 1) % g_desk.a.frameCount);
    deskLoad(now);
    changed = true;
  }
  if (changed) {
    if (g_desk.index >= 0)
      clawdRenderCells(deskCell, nullptr, 0, DESK_GRID - rows, DESK_GRID, rows, DESK_CELL, 0, 0,
                       g_desk.a.palette, clawdStagePrev(), force);
    else if (force) {
      static const char* const msg = "desk.pak missing";
      ClawdText t = {&CF_STYRENE_14, msg,
                     (int16_t)(CLAWD_W / 2 - clawdTextWidth(CF_STYRENE_14, msg) / 2),
                     (int16_t)(CLAWD_H / 2 - 8), CLAWD_DIM};
      clawdRender(0, CLAWD_H / 2 - 8, CLAWD_W, 20, CLAWD_BG, nullptr, 0, &t, 1);
    }
  }
  if (!line[0]) return;
  const bool marquee = clawdTextWidth(CF_MONO_18, line) > CONTENT_W;
  static uint32_t last = 0;
  if (marquee) {
    if (!force && now - last < 50) return;
    last = now;
  } else if (!force && strcmp(line, g_statusDrawn) == 0) {
    return;
  }
  strncpy(g_statusDrawn, line, sizeof(g_statusDrawn) - 1);
  g_statusDrawn[sizeof(g_statusDrawn) - 1] = 0;
  drawStatus(line, marquee, now, g_deskSince);
}

// ---- alert frame -------------------------------------------------------------------
static void drawFrame(bool on) {
  const uint16_t c = on ? CLAWD_ACCENT : CLAWD_BG;
  fillRect(0, 0, CLAWD_W, FRAME_W, c);
  fillRect(0, CLAWD_H - FRAME_W, CLAWD_W, FRAME_W, c);
  fillRect(0, FRAME_W, FRAME_W, CLAWD_H - 2 * FRAME_W, c);
  fillRect(CLAWD_W - FRAME_W, FRAME_W, FRAME_W, CLAWD_H - 2 * FRAME_W, c);
  g_frameOn = on;
}

static void endAlert() {
  if (!g_alert) return;
  g_alert = false;
  g_mascot.setAttention(false);
  if (g_view != VIEW_SPLASH && g_view != VIEW_DESK) {
    drawFrame(false);
    g_statusDrawn[0] = 0;
  }
}

// ---- public ----------------------------------------------------------------------
void clawdUiBegin() {
  memset(&g_data, 0, sizeof(g_data));
  g_data.sessionResetMin = g_data.weeklyResetMin = -1;
  clawdPackOpen();
  deskPackOpen();
  g_desk.index = -1;
  g_deskDirty = true;
  g_splash.begin();
  g_mascot.begin(MASCOT_X, MASCOT_Y);
  g_mini.begin("cloud", CLAWD_W / 2, IDLE_CY, IDLE_PX);
  g_view = VIEW_NONE;
  g_dirtyAll = true;
}

void clawdUiInvalidate() {
  g_view = VIEW_NONE;
  g_dirtyAll = true;
}

void clawdUiPayload(const ClawdData& d) {
  const uint32_t now = clawdNow();
  g_lastOk = d.ok;
  if (d.alertClear) endAlert();
  if (d.alertMsg[0]) {
    const bool wasOn = g_alert;
    strncpy(g_alertMsg, d.alertMsg, sizeof(g_alertMsg) - 1);
    g_alertMsg[sizeof(g_alertMsg) - 1] = 0;
    g_alert = true;
    g_alertSince = now;
    g_mascot.setAttention(true);
    if (!wasOn) g_statusDrawn[0] = 0;
  }
  if (!d.ok) return;     // "no data" beat: keep the last numbers, go idle when stale

  g_data = d;
  if (!g_received) g_firstDataMs = now;
  g_received = true;
  g_lastDataMs = now;
  if (d.clockEpoch > 0) {
    g_clockEpoch = d.clockEpoch;
    g_clockBaseMs = now;
    g_clockFmt = d.clockFmt == 12 ? 12 : 24;
    g_dirtyTitle = true;
  } else if (g_clockEpoch) {
    g_clockEpoch = 0;
    g_dirtyTitle = true;
  }
  if (!d.enterprise) clawdRateSample(d.sessionPct);
  const int g = clawdRateGroup();
  if (g != g_rateGroup) {
    g_rateGroup = g;
    g_splash.repickForRate();
  }
  g_dirtyPanels = true;
}

void clawdUiState(const ClawdState& s) {
  const uint32_t now = clawdNow();
  const uint32_t ttl = (uint32_t)(s.ttlSec == 0 ? 60 : (s.ttlSec > STATE_TTL_MAX ? STATE_TTL_MAX : s.ttlSec)) * 1000UL;
  g_dim = (s.dim >= 0 && s.dim <= 100) ? s.dim : -1;
  g_dimUntil = now + ttl;
  if (!s.anim[0]) { g_deskOn = false; return; }
  const bool wasLive = deskLive(now);
  if (!wasLive || strcmp(s.anim, g_deskWant) != 0) {
    strncpy(g_deskWant, s.anim, sizeof(g_deskWant) - 1);
    g_deskWant[sizeof(g_deskWant) - 1] = 0;
    g_desk.index = -2;                           // (re)start on the next service
  }
  const bool cd = s.countdownSec > 0;
  if (strcmp(s.text, g_deskText) != 0 || cd != g_cdOn) {
    strncpy(g_deskText, s.text, sizeof(g_deskText) - 1);
    g_deskText[sizeof(g_deskText) - 1] = 0;
    g_deskSince = now;
    g_deskDirty = true;                          // the line appeared/went: rows change
  }
  g_cdOn = cd;
  if (cd) g_cdEndMs = now + (uint32_t)s.countdownSec * 1000UL;
  g_deskOn = true;
  g_deskUntil = now + ttl;
}

int clawdUiDim() {
  return (g_dim >= 0 && (int32_t)(g_dimUntil - clawdNow()) > 0) ? g_dim : -1;
}

bool clawdUiOnSplash() { return g_view == VIEW_SPLASH; }
bool clawdUiOnDesk() { return g_view == VIEW_DESK; }

const char* clawdUiAnimName() {
  static char name[16];
  name[0] = 0;
  if (g_view == VIEW_SPLASH) clawdAnimName(g_splash.current(), name, sizeof(name));
  else if (g_view == VIEW_DESK && g_desk.index >= 0) strncpy(name, g_desk.a.name, sizeof(name) - 1);
  return name;
}

void clawdUiService(uint8_t screen, int8_t fixedAnim) {
  const uint32_t now = clawdNow();
  if (g_alert && now - g_alertSince >= ALERT_TIMEOUT_MS) endAlert();

  const bool fresh = dataFresh(now);
  View want;
  if (deskLive(now))                       want = VIEW_DESK;
  else if (g_alert)                        want = fresh ? VIEW_USAGE : VIEW_IDLE;
  else if (screen == CLAWD_SCREEN_SPLASH)  want = VIEW_SPLASH;
  else if (screen == CLAWD_SCREEN_USAGE)   want = fresh ? VIEW_USAGE : VIEW_IDLE;
  else                                     want = fresh ? VIEW_USAGE : VIEW_SPLASH;

  static int8_t shownAnim = -2;
  if (want == VIEW_SPLASH && g_view == VIEW_SPLASH && fixedAnim != shownAnim) {
    shownAnim = fixedAnim;
    g_splash.show(fixedAnim);
  }

  if (want == VIEW_DESK && g_view == VIEW_DESK && (g_deskDirty || g_desk.index == -2))
    g_dirtyAll = true;               // new animation or text line: repaint the desk

  if (want != g_view || g_dirtyAll) {
    const View was = g_view;
    g_view = want;
    g_dirtyAll = false;
    fillRect(0, 0, CLAWD_W, CLAWD_H, CLAWD_BG);
    g_frameOn = false;
    if (want == VIEW_DESK) {
      g_splash.hide();
      g_deskDirty = false;
      g_statusDrawn[0] = 0;
      if (g_desk.index < 0 || was != VIEW_DESK) deskStart(now);
      deskTick(now, true);
      return;
    }
    if (want == VIEW_SPLASH) {
      shownAnim = fixedAnim;
      g_splash.show(fixedAnim);
      if (!clawdPackOk()) {          // animations not uploaded: say so, don't sit black
        static const char* const msg = "clawd.pak missing";
        ClawdText t = {&CF_STYRENE_14, msg,
                       (int16_t)(CLAWD_W / 2 - clawdTextWidth(CF_STYRENE_14, msg) / 2),
                       (int16_t)(CLAWD_H / 2 - 8), CLAWD_DIM};
        clawdRender(0, CLAWD_H / 2 - 8, CLAWD_W, 20, CLAWD_BG, nullptr, 0, &t, 1);
      }
    } else {
      g_splash.hide();
      // the desk view borrowed the mascot's frame buffer: reload it
      if (was == VIEW_SPLASH || was == VIEW_NONE || was == VIEW_DESK) g_mascot.reset();
      g_mascot.invalidate();
      g_mini.reset();
      g_dirtyTitle = true;
      g_dirtyPanels = true;
      g_statusDrawn[0] = 0;
      tickStatus(now, true);
    }
  }

  if (g_view == VIEW_DESK) {
    deskTick(now, false);
    return;
  }
  if (g_view == VIEW_SPLASH) {
    g_splash.tick();
    return;
  }

  // usage / idle
  if (g_alert) {
    const bool on = ((now - g_alertSince) / 600) % 2 == 0;
    if (on != g_frameOn) drawFrame(on);
  }
  char probe[16];
  int minute;
  titleText(now, probe, sizeof(probe), &minute);
  if (g_dirtyTitle || minute != g_clockMin) {
    g_dirtyTitle = false;
    drawTitle(now);
  }
  if (g_view == VIEW_USAGE) {
    if (g_dirtyPanels) { g_dirtyPanels = false; drawPanels(); }
  } else {
    g_dirtyPanels = true;          // repaint the panels when data comes back
    g_mini.tick();
  }
  g_mascot.tick(g_rateGroup);
  tickStatus(now, false);
}
