// ClawdUi.cpp — the original firmware's compact (240 px) usage screen + the splash, drawn
// with ClawdDraw. Layout numbers are the original firmware's compute_layout() small
// breakpoint, resolved to absolute pixels (see ui.cpp in the original firmware repo).
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

#define DESK_TEXT_SHIFT  32        // a text line lifts the scene this far (feet kept, bubble tops lost)
#define DESK_TEXT_TOP    (CLAWD_H - DESK_TEXT_SHIFT)
#define DESK_SWITCH_WAIT_MS 2000UL // a new sequence waits at most this long for the pass end
#define DESK_STATS       128
#define STATE_TTL_MAX    3600

enum View : int8_t { VIEW_NONE = -1, VIEW_SPLASH, VIEW_IDLE, VIEW_USAGE, VIEW_DESK };

static ClawdSplash g_splash;
static ClawdMascot g_mascot;
static ClawdWalker g_walker;     // the pixel layout's strolling Clawd (the mascot is off there)
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
static char      g_deskText[96] = "";
static bool      g_cdOn = false;
static uint32_t  g_cdEndMs = 0;
static int16_t   g_dim = -1;
static uint32_t  g_dimUntil = 0;
static bool      g_deskDirty = true;      // the text line came or went: full desk repaint

struct DeskSeq {
  char    names[CLAWD_SEQ_MAX][16];
  uint8_t n = 0;
  bool    repeat = false;
  char    vars[CLAWD_VAR_MAX][16];    // variants of names[0] (deskHasVars)
  uint8_t nv = 0;
};
static DeskSeq   g_deskWant;              // what the daemon asked for
static bool      g_deskSwitch = false;    // g_deskWant waits for the current pass to end
static bool      g_deskSwitchNow = false; // ... unless urgent
static uint32_t  g_deskSwitchSince = 0;

// Plays g_desk.seq. The panel holds the previous frame, so each step only
// draws the delta to the next one (DeskPack.h).
struct DeskPlayer {
  DeskSeq  seq;
  uint8_t  item = 0;
  int16_t  index = -1;          // -1: nothing (pack missing), -2: restart on the next service
  DeskAnim a;
  uint16_t frame = 0;           // frame on screen
  uint32_t due = 0;             // when the next frame is due
  bool     held = false;        // a one-shot reached its end and holds it
  int      shift = 0, limit = CLAWD_H;   // scene lift and the first row it may not touch
  uint8_t  passes = 0;          // passes of names[0] since it (re)started
  int8_t   var = -1;            // the variant on screen, -1: names[0]
  int8_t   lastVar = -1;        // the variant played last (not picked twice in a row)
};
static DeskPlayer g_desk;

static uint16_t  g_drawSamples[DESK_STATS];   // 1/10 ms
static uint16_t  g_lateSamples[DESK_STATS];
static uint16_t  g_statN = 0, g_statPos = 0;
static uint32_t  g_statTotal = 0;

static View      g_view = VIEW_NONE;
static bool      g_dirtyAll = true;

// The clock's IP, captioned under the splash while it waits for data (auto
// screen only): the one way to reach a clock the desktop app cannot find.
static char      g_addr[24] = "";
static char      g_addrDrawn[24] = "";
static bool      g_dirtyPanels = true;
static bool      g_dirtyTitle = true;

// status line animation (the original firmware's whimsical spinner)
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

// ---- pace layout (usage.layout = CLAWD_LAYOUT_PACE) ---------------------------------
// Two taller cards that answer "at this rate, do I run out before the reset?":
// a pace pill, a white time marker on the bar (how much of the window has gone)
// and a forecast line. Same rules as plans/reports/261012-design-usage-screen-preview.
#define PACE_P1_Y   44
#define PACE_H      92
#define PACE_P2_Y   142
#define WIN_SESSION 300          // 5-hour window, minutes
#define WIN_WEEKLY  10080        // 7-day window, minutes

static uint8_t g_layout = CLAWD_LAYOUT_CLASSIC;   // requested layout (one this build draws)
static uint8_t g_effDrawn = 0xFF;                 // layout the chrome (title, mascot) was last set up for
static bool    g_floorWipe = false;               // an alert began: clear the pixel layout's floor strip once
static uint32_t g_sig = 0;                        // what the last panel paint showed
static bool    g_sigValid = false;

// Enterprise accounts (Spending / Period) only have the classic screen.
static uint8_t usageLayout() { return g_data.enterprise ? CLAWD_LAYOUT_CLASSIC : g_layout; }
// The title and mascot follow the layout only while usage is on screen; the idle
// screen keeps the classic chrome.
static uint8_t chromeLayout() { return g_view == VIEW_USAGE ? usageLayout() : (uint8_t)CLAWD_LAYOUT_CLASSIC; }
// Layouts whose panels reach the bottom: the spinner line only shows an alert there.
static bool fullBottom() {
  const uint8_t l = chromeLayout();
  return l == CLAWD_LAYOUT_PACE || l == CLAWD_LAYOUT_HERO || l == CLAWD_LAYOUT_RINGS || l == CLAWD_LAYOUT_PIXEL;
}

struct Pace { const char* label; uint16_t color; float t; };

// t = share of the window gone (0..1); the label is where the % lands at the
// reset if the current rate holds.
static Pace paceOf(float pct, int resetMin, int win) {
  if (resetMin < 0) return {"No data", CLAWD_DIM, 0.0f};
  float t = (float)(win - resetMin) / (float)win;
  if (t < 0) t = 0;
  if (t > 1) t = 1;
  if (t < 0.05f) return {"Just reset", CLAWD_GREEN, t};
  const float end = pct / t;
  if (end > 100.0f) return {"Over pace", CLAWD_RED, t};
  if (end >= 70.0f) return {"On pace", CLAWD_AMBER, t};
  return {"Under pace", CLAWD_GREEN, t};
}

static void formatSpan(int mins, char* buf, size_t n) {   // "35m", "2h 14m", "1d 4h"
  if (mins < 60)        snprintf(buf, n, "%dm", mins);
  else if (mins < 1440) snprintf(buf, n, "%dh %dm", mins / 60, mins % 60);
  else                  snprintf(buf, n, "%dd %dh", mins / 1440, (mins % 1440) / 60);
}

// The forecast line and its colour. `nowMin` = local minute of day, -1 = unknown.
static uint16_t forecast(float pct, int resetMin, int win, int nowMin, char* buf, size_t n) {
  if (resetMin < 0) { snprintf(buf, n, "---"); return CLAWD_DIM; }
  if (pct >= 100.0f) { snprintf(buf, n, "Limit reached"); return CLAWD_RED; }
  const int el = win - resetMin;
  if (el < win / 20 || pct < 1.0f || el <= 0) { snprintf(buf, n, "Too early to tell"); return CLAWD_DIM; }
  const int left = (int)((100.0f - pct) * (float)el / pct + 0.5f);   // minutes to 100% at this rate
  if (left >= resetMin) { snprintf(buf, n, "Lasts until reset"); return CLAWD_GREEN; }
  if (win == WIN_SESSION && nowMin >= 0) {
    const int at = (nowMin + left) % 1440;
    snprintf(buf, n, "Out at ~%02d:%02d", at / 60, at % 60);
  } else {
    char span[16];
    formatSpan(left, span, sizeof(span));
    snprintf(buf, n, "Out in ~%s", span);
  }
  return CLAWD_RED;
}

static void drawPaceCard(int y, const char* label, float pct, int resetMin, int win, int nowMin) {
  const int cx = MARGIN + PAD_X, cw = CONTENT_W - 2 * PAD_X;
  const ClawdFont& sf = CF_STYRENE_14;
  if (pct < 0) pct = 0;
  const float bar = pct > 100 ? 100 : pct;
  const Pace p = paceOf(pct, resetMin, win);

  char num[12], reset[48], fc[32];
  snprintf(num, sizeof(num), "%d%%", (int)(pct + 0.5f));
  formatReset(resetMin, reset, sizeof(reset));
  const uint16_t fcColor = forecast(pct, resetMin, win, nowMin, fc, sizeof(fc));

  const int numW = clawdTextWidth(CF_STYRENE_24, num);
  const int pillW = clawdTextWidth(sf, p.label) + 12;
  const int pillX = cx + cw - pillW;
  const int fillW = (int)(cw * bar / 100.0f + 0.5f);
  const int markX = cx + (int)(cw * p.t + 0.5f) - 1;

  ClawdShape shapes[5];
  int n = 0;
  shapes[n++] = {MARGIN, (int16_t)y, CONTENT_W, PACE_H, 8, CLAWD_PANEL};
  if (resetMin >= 0) shapes[n++] = {(int16_t)pillX, (int16_t)(y + 9), (int16_t)pillW, 20, 99, p.color};
  shapes[n++] = {(int16_t)cx, (int16_t)(y + 38), (int16_t)cw, BAR_H, 6, CLAWD_BAR_BG};
  if (fillW > 0) shapes[n++] = {(int16_t)cx, (int16_t)(y + 38), (int16_t)(fillW < BAR_H ? BAR_H : fillW), BAR_H, 6, pctColor(pct)};
  if (resetMin >= 0) shapes[n++] = {(int16_t)markX, (int16_t)(y + 34), 2, 20, 1, CLAWD_TEXT};

  ClawdText texts[5];
  int t = 0;
  texts[t++] = {&CF_STYRENE_24, num, (int16_t)cx, (int16_t)(y + 6), CLAWD_TEXT};
  texts[t++] = {&sf, label, (int16_t)(cx + numW + 8), (int16_t)(y + 14), CLAWD_DIM};
  if (resetMin >= 0)
    texts[t++] = {&sf, p.label, (int16_t)(pillX + 6), (int16_t)(y + 11),
                  static_cast<uint16_t>(p.color == CLAWD_GREEN ? CLAWD_TEXT : CLAWD_BG)};
  texts[t++] = {&sf, reset, (int16_t)cx, (int16_t)(y + 56), CLAWD_DIM};
  texts[t++] = {&sf, fc, (int16_t)cx, (int16_t)(y + 72), fcColor};
  clawdRender(MARGIN, y, CONTENT_W, PACE_H, CLAWD_BG, shapes, n, texts, t);
}

static int localMinute(uint32_t now) {
  if (g_clockEpoch <= 0) return -1;
  const long cur = g_clockEpoch + (long)((now - g_clockBaseMs) / 1000);
  return (int)((cur % 86400L + 86400L) % 86400L / 60);
}

static void drawPacePanels() {
  const ClawdData& d = g_data;
  const int nowMin = localMinute(clawdNow());
  drawPaceCard(PACE_P1_Y, "5-hour", d.sessionPct, d.sessionResetMin, WIN_SESSION, nowMin);
  drawPaceCard(PACE_P2_Y, "weekly", d.weeklyPct, d.weeklyResetMin, WIN_WEEKLY, nowMin);
}

// ---- hero / gauges / rings / pixel layouts ---------------------------------------------
static void titleText(uint32_t now, char* out, size_t n, int* minute);

static void spanText(int mins, char* buf, size_t n) {   // "2h 14m", "---" when unknown
  if (mins < 0) snprintf(buf, n, "---");
  else formatSpan(mins, buf, n);
}

static float clampPct(float p) { return p < 0 ? 0 : p; }
static int   pctInt(float p) { return (int)(clampPct(p) + 0.5f); }
static int   fillPx(int w, float pct) {                  // bar fill width, 0..w
  const float p = pct > 100 ? 100 : clampPct(pct);
  return (int)(w * p / 100.0f + 0.5f);
}
static int rightX(const ClawdFont& f, const char* s, int right) { return right - clawdTextWidth(f, s); }
static int centerX(const ClawdFont& f, const char* s, int cx) { return cx - clawdTextWidth(f, s) / 2; }

// Hero: the 5-hour window as one big number, weekly as a strip underneath.
static void drawHeroPanels() {
  const ClawdData& d = g_data;
  const ClawdFont& sf = CF_STYRENE_14;
  const ClawdFont& nf = CF_STYRENE_48;
  const ClawdFont& pf = CF_STYRENE_24;
  const int nowMin = localMinute(clawdNow());
  const float s = clampPct(d.sessionPct);
  const Pace p = paceOf(s, d.sessionResetMin, WIN_SESSION);
  char num[8], span[16], rst[28], fc[32];
  snprintf(num, sizeof(num), "%d", pctInt(s));
  spanText(d.sessionResetMin, span, sizeof(span));
  snprintf(rst, sizeof(rst), "resets %s", span);
  const uint16_t fcColor = forecast(s, d.sessionResetMin, WIN_SESSION, nowMin, fc, sizeof(fc));

  const int numY = 62;
  const int base = numY + nf.lineHeight - nf.baseLine;           // shared baseline for the number and its %
  const int numW = clawdTextWidth(nf, num);
  const int pillW = clawdTextWidth(sf, p.label) + 26;
  const int pillX = 228 - pillW, pillY = base - 26;
  const int barX = 12, barW = 216, barY = 132;
  const int fillW = fillPx(barW, s);
  const int markX = barX + (int)(barW * p.t + 0.5f) - 1;

  ClawdShape sh[5];
  int n = 0;
  if (d.sessionResetMin >= 0) {
    sh[n++] = {(int16_t)pillX, (int16_t)pillY, (int16_t)pillW, 20, 99, CLAWD_BAR_BG};
    sh[n++] = {(int16_t)(pillX + 8), (int16_t)(pillY + 7), 6, 6, 3, p.color};
  }
  sh[n++] = {barX, barY, barW, 14, 7, CLAWD_BAR_BG};
  if (fillW > 0) sh[n++] = {barX, barY, (int16_t)(fillW < 14 ? 14 : fillW), 14, 7, pctColor(s)};
  ClawdShape mark = {(int16_t)markX, (int16_t)(barY - 5), 2, 24, 1, CLAWD_TEXT};
  ClawdShape all[6];
  int m = 0;
  for (int i = 0; i < n; i++) all[m++] = sh[i];
  if (d.sessionResetMin >= 0) all[m++] = mark;

  ClawdText tx[7];
  int t = 0;
  tx[t++] = {&sf, "5-HOUR", 12, 50, CLAWD_DIM};
  tx[t++] = {&sf, rst, (int16_t)rightX(sf, rst, 228), 50, CLAWD_DIM};
  tx[t++] = {&nf, num, 10, (int16_t)numY, CLAWD_TEXT};
  tx[t++] = {&pf, "%", (int16_t)(14 + numW), (int16_t)(base - (pf.lineHeight - pf.baseLine)), CLAWD_TEXT};
  if (d.sessionResetMin >= 0)
    tx[t++] = {&sf, p.label, (int16_t)(pillX + 18), (int16_t)(pillY + 3), CLAWD_TEXT};
  tx[t++] = {&sf, fc, 12, 156, fcColor};
  clawdRender(0, 44, CLAWD_W, 140, CLAWD_BG, all, m, tx, t);

  // weekly strip
  const float w = clampPct(d.weeklyPct);
  char wnum[8], wspan[16];
  snprintf(wnum, sizeof(wnum), "%d%%", pctInt(w));
  spanText(d.weeklyResetMin, wspan, sizeof(wspan));
  const int wf = fillPx(204, w);
  ClawdShape ws[3];
  int k = 0;
  ws[k++] = {8, 184, 224, 48, 8, CLAWD_PANEL};
  ws[k++] = {18, 216, 204, 6, 3, CLAWD_BAR_BG};
  if (wf > 0) ws[k++] = {18, 216, (int16_t)(wf < 6 ? 6 : wf), 6, 3, pctColor(w)};
  ClawdText wt[3] = {
      {&sf, "Weekly", 18, 192, CLAWD_DIM},
      {&sf, wspan, (int16_t)(18 + clawdTextWidth(sf, "Weekly") + 8), 192, CLAWD_DIM},
      {&pf, wnum, (int16_t)rightX(pf, wnum, 222), 187, CLAWD_TEXT},
  };
  clawdRender(8, 184, 224, 48, CLAWD_BG, ws, k, wt, 3);
}

// Gauges: two 270 degree gauges, each on its own card (the spinner line stays).
static void drawGaugeCard(int x, const char* label, float pct, int resetMin) {
  const int cx = x + 54, cy = 102;
  const float v = clampPct(pct);
  const int gr = 40, gw = 8;                  // wide enough inside for "100%"
  char num[8], span[16];
  snprintf(num, sizeof(num), "%d%%", pctInt(v));
  spanText(resetMin, span, sizeof(span));
  ClawdShape card = {(int16_t)x, 46, 108, 156, 10, CLAWD_PANEL};
  ClawdArc arcs[2];
  int n = 0;
  arcs[n++] = {(int16_t)cx, (int16_t)cy, (int16_t)gr, (uint8_t)gw, 225, 270, CLAWD_BAR_BG};
  const int sweep = (int)(270.0f * (v > 100 ? 100 : v) / 100.0f + 0.5f);
  if (sweep > 0) arcs[n++] = {(int16_t)cx, (int16_t)cy, (int16_t)gr, (uint8_t)gw, 225, (int16_t)sweep, pctColor(v)};
  ClawdText tx[3] = {
      {&CF_STYRENE_24, num, (int16_t)centerX(CF_STYRENE_24, num, cx), 91, CLAWD_TEXT},
      {&CF_STYRENE_14, label, (int16_t)centerX(CF_STYRENE_14, label, cx), 150, CLAWD_TEXT},
      {&CF_STYRENE_14, span, (int16_t)centerX(CF_STYRENE_14, span, cx), 172, CLAWD_DIM},
  };
  clawdRenderArcs(x, 46, 108, 156, CLAWD_BG, &card, 1, arcs, n, tx, 3);
}

static void drawGaugePanels() {
  drawGaugeCard(8, "Current", g_data.sessionPct, g_data.sessionResetMin);
  drawGaugeCard(124, "Weekly", g_data.weeklyPct, g_data.weeklyResetMin);
}

// Rings: 5-hour on the outside, weekly inside, a white dot on each ring where the
// window's time has got to; the clock and both numbers sit in the middle. The
// corner mascot window (92..148 x 36..78) is left alone, so the ring field is
// painted in four pieces around it.
#define RING_CX 120
#define RING_CY 120
#define RING_MASCOT_X 92
#define RING_MASCOT_Y 36

static int ringSweep(float pct) {
  const float v = pct > 100 ? 100 : clampPct(pct);
  int s = (int)(360.0f * v / 100.0f + 0.5f);
  return (v > 0 && s < 1) ? 1 : s;
}

static void drawRingPanels() {
  const ClawdData& d = g_data;
  const float s = clampPct(d.sessionPct), w = clampPct(d.weeklyPct);
  const Pace ps = paceOf(s, d.sessionResetMin, WIN_SESSION);
  const Pace pw = paceOf(w, d.weeklyResetMin, WIN_WEEKLY);

  ClawdArc arcs[6];
  int n = 0;
  arcs[n++] = {RING_CX, RING_CY, 110, 12, 0, 360, CLAWD_BAR_BG};
  if (s > 0) arcs[n++] = {RING_CX, RING_CY, 110, 12, 0, (int16_t)ringSweep(s), pctColor(s)};
  arcs[n++] = {RING_CX, RING_CY, 94, 8, 0, 360, CLAWD_BAR_BG};
  if (w > 0) arcs[n++] = {RING_CX, RING_CY, 94, 8, 0, (int16_t)ringSweep(w), pctColor(w)};
  if (d.sessionResetMin >= 0) {
    const int a = (int)(360.0f * ps.t + 0.5f);
    arcs[n++] = {(int16_t)(RING_CX + (int)(110 * clawdSin(a) + (clawdSin(a) < 0 ? -0.5f : 0.5f))),
                 (int16_t)(RING_CY - (int)(110 * clawdCos(a) + (clawdCos(a) < 0 ? -0.5f : 0.5f))),
                 0, 5, 0, 360, CLAWD_TEXT};
  }
  if (d.weeklyResetMin >= 0) {
    const int a = (int)(360.0f * pw.t + 0.5f);
    arcs[n++] = {(int16_t)(RING_CX + (int)(94 * clawdSin(a) + (clawdSin(a) < 0 ? -0.5f : 0.5f))),
                 (int16_t)(RING_CY - (int)(94 * clawdCos(a) + (clawdCos(a) < 0 ? -0.5f : 0.5f))),
                 0, 5, 0, 360, CLAWD_TEXT};
  }

  char clock[16], a[8], b[8], span[16], rst[28];
  int minute;
  titleText(clawdNow(), clock, sizeof(clock), &minute);
  g_clockMin = minute;
  snprintf(a, sizeof(a), "%d%%", pctInt(s));
  snprintf(b, sizeof(b), "%d%%", pctInt(w));
  spanText(d.sessionResetMin, span, sizeof(span));
  snprintf(rst, sizeof(rst), "resets %s", span);
  const ClawdFont& sf = CF_STYRENE_14;
  const ClawdFont& pf = CF_STYRENE_24;
  ClawdShape div = {RING_CX - 1, 130, 2, 16, 1, CLAWD_BAR_BG};
  ClawdText tx[6] = {
      {&CF_TIEMPOS_34, clock, (int16_t)centerX(CF_TIEMPOS_34, clock, RING_CX), 80, CLAWD_TEXT},
      {&pf, a, (int16_t)rightX(pf, a, RING_CX - 8), 126, pctColor(s)},
      {&pf, b, (int16_t)(RING_CX + 8), 126, pctColor(w)},
      {&sf, "5-hour", (int16_t)rightX(sf, "5-hour", RING_CX - 8), 152, CLAWD_DIM},
      {&sf, "weekly", (int16_t)(RING_CX + 8), 152, CLAWD_DIM},
      {&sf, rst, (int16_t)centerX(sf, rst, RING_CX), 176, CLAWD_DIM},
  };
  clawdRenderArcs(0, 0, CLAWD_W, RING_MASCOT_Y, CLAWD_BG, &div, 1, arcs, n, tx, 6);
  clawdRenderArcs(0, RING_MASCOT_Y, RING_MASCOT_X, 42, CLAWD_BG, &div, 1, arcs, n, tx, 6);
  clawdRenderArcs(RING_MASCOT_X + 56, RING_MASCOT_Y, CLAWD_W - RING_MASCOT_X - 56, 42, CLAWD_BG, &div, 1, arcs, n, tx, 6);
  clawdRenderArcs(0, RING_MASCOT_Y + 42, CLAWD_W, CLAWD_H - RING_MASCOT_Y - 42, CLAWD_BG, &div, 1, arcs, n, tx, 6);
}

// Pixel: 20 blocks a row (5% each), compact reset text, a dotted floor for Clawd.
#define PIX_ROW1_Y 52
#define PIX_ROW2_Y 124
#define PIX_FLOOR_Y 234

static void drawPixelRow(int y, const char* label, float pct, int resetMin) {
  const ClawdFont& f = CF_MONO_18;
  const float v = clampPct(pct);
  char num[8], span[16], rst[24];
  snprintf(num, sizeof(num), "%d%%", pctInt(v));
  spanText(resetMin, span, sizeof(span));
  char* o = rst + snprintf(rst, sizeof(rst), "rst ");
  for (const char* p = span; *p && o < rst + sizeof(rst) - 1; p++)
    if (*p != ' ') *o++ = *p;
  *o = 0;
  ClawdText top[2] = {{&f, label, 11, (int16_t)y, CLAWD_DIM},
                      {&f, num, (int16_t)rightX(f, num, 229), (int16_t)y, CLAWD_TEXT}};
  clawdRender(0, y, CLAWD_W, f.lineHeight, CLAWD_BG, nullptr, 0, top, 2);
  fillRect(0, y + 24, CLAWD_W, 18, CLAWD_BG);
  const int lit = (int)((v > 100 ? 100 : v) / 5.0f + 0.5f);
  const uint16_t col = pctColor(v);
  for (int i = 0; i < 20; i++) fillRect(11 + i * 11, y + 24, 9, 18, i < lit ? col : CLAWD_BAR_BG);
  ClawdText bottom = {&f, rst, 11, (int16_t)(y + 48), CLAWD_DIM};
  clawdRender(0, y + 48, CLAWD_W, f.lineHeight, CLAWD_BG, nullptr, 0, &bottom, 1);
}

static void drawPixelPanels() {
  drawPixelRow(PIX_ROW1_Y, "5H", g_data.sessionPct, g_data.sessionResetMin);
  drawPixelRow(PIX_ROW2_Y, "7D", g_data.weeklyPct, g_data.weeklyResetMin);
  fillRect(0, PIX_FLOOR_Y - 38, CLAWD_W, 38, CLAWD_BG);   // wipes an alert line's leftovers; Clawd repaints itself after
  for (int x = 4; x < CLAWD_W - 4; x += 4) fillRect(x, PIX_FLOOR_Y, 2, 2, CLAWD_BAR_BG);
}

// What a paint of the new layouts shows. A payload that changes none of it repaints nothing.
static uint32_t panelSig(uint32_t now) {
  const ClawdData& d = g_data;
  uint32_t h = 2166136261u;
  const int32_t v[] = {usageLayout(), pctInt(d.sessionPct), d.sessionResetMin, pctInt(d.weeklyPct),
                       d.weeklyResetMin, localMinute(now)};
  for (size_t i = 0; i < sizeof(v) / sizeof(v[0]); i++) h = (h ^ (uint32_t)v[i]) * 16777619u;
  return h;
}

static void drawPanels() {
  switch (usageLayout()) {
    case CLAWD_LAYOUT_PACE:   drawPacePanels();   return;
    case CLAWD_LAYOUT_HERO:   drawHeroPanels();   return;
    case CLAWD_LAYOUT_RINGS:  drawRingPanels();   return;
    case CLAWD_LAYOUT_GAUGES: drawGaugePanels();  return;
    case CLAWD_LAYOUT_PIXEL:  drawPixelPanels();  return;
    default: break;
  }
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
  const uint8_t layout = chromeLayout();
  if (layout == CLAWD_LAYOUT_PIXEL) {          // mono clock across the top, no mascot there
    if (g_clockMin < 0) snprintf(buf, sizeof(buf), "--:--");
    const ClawdFont& pf = CF_MONO_32;
    ClawdText t = {&pf, buf, (int16_t)(CLAWD_W / 2 - clawdTextWidth(pf, buf) / 2), 4, CLAWD_TEXT};
    clawdRender(FRAME_W, FRAME_W, CLAWD_W - 2 * FRAME_W, PIX_ROW1_Y - FRAME_W - 2, CLAWD_BG, nullptr, 0, &t, 1);
    return;
  }
  if (layout == CLAWD_LAYOUT_RINGS) {          // in the middle of the rings
    const ClawdFont& rf = CF_TIEMPOS_34;
    ClawdText t = {&rf, buf, (int16_t)(RING_CX - clawdTextWidth(rf, buf) / 2), 80, CLAWD_TEXT};
    clawdRender(52, 80, 136, 36, CLAWD_BG, nullptr, 0, &t, 1);
    return;
  }
  const ClawdFont& f = CF_TIEMPOS_34;
  int x = CLAWD_W / 2 + TITLE_NUDGE - clawdTextWidth(f, buf) / 2;
  if (layout == CLAWD_LAYOUT_HERO) x = 232 - clawdTextWidth(f, buf);   // hero: clock on the right
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

static void deskSpan(void*, int x, int y, int len, const uint8_t* idx) {
  const DeskAnim& a = g_desk.a;
  const int sy = a.y + y - g_desk.shift;
  if (sy < 0 || sy >= g_desk.limit) return;
  uint16_t* line = clawdScratch();
  for (int i = 0; i < len; i++) line[i] = a.palette[idx[i] & 0x0F];
  clawdSurface()->span(a.x + x, sy, len, line);
}

static void deskStat(uint32_t drawUs, uint32_t lateUs) {
  g_drawSamples[g_statPos] = (uint16_t)(drawUs / 100 > 65535 ? 65535 : drawUs / 100);
  g_lateSamples[g_statPos] = (uint16_t)(lateUs / 100 > 65535 ? 65535 : lateUs / 100);
  g_statPos = (uint16_t)((g_statPos + 1) % DESK_STATS);
  if (g_statN < DESK_STATS) g_statN++;
  g_statTotal++;
}

// Apply delta `d` of the current animation to the panel.
static bool deskDraw(uint16_t d, bool full = false) {
  if (g_desk.index < 0) return false;
  if (!deskPackDraw(g_desk.a, d, deskSpan, nullptr, full)) {
    g_desk.index = -1;               // pack gone or corrupt: stop drawing from it
    return false;
  }
  return true;
}

// A screen rectangle [x0,x1) x [y0,y1); empty when x1 <= x0 or y1 <= y0.
struct DeskRect { int x0, y0, x1, y1; };
static const DeskRect DESK_NONE = {0, 0, 0, 0};

// The current animation's crop on screen (the part it may touch), or empty.
static DeskRect deskCrop() {
  if (g_desk.index < 0) return DESK_NONE;
  const DeskAnim& a = g_desk.a;
  DeskRect r = {a.x, a.y - g_desk.shift, a.x + a.w, a.y - g_desk.shift + a.h};
  if (r.y0 < 0) r.y0 = 0;
  if (r.y1 > g_desk.limit) r.y1 = g_desk.limit;
  return r;
}

static void fillDeskRect(int x0, int y0, int x1, int y1) {
  if (x1 > x0 && y1 > y0) fillRect(x0, y0, x1 - x0, y1 - y0, CLAWD_BG);
}

// Black out `r` except where it overlaps `keep` (up to four bands).
static void deskClearExcept(const DeskRect& r, const DeskRect& keep) {
  const int ix0 = r.x0 > keep.x0 ? r.x0 : keep.x0, ix1 = r.x1 < keep.x1 ? r.x1 : keep.x1;
  const int iy0 = r.y0 > keep.y0 ? r.y0 : keep.y0, iy1 = r.y1 < keep.y1 ? r.y1 : keep.y1;
  if (ix1 <= ix0 || iy1 <= iy0) { fillDeskRect(r.x0, r.y0, r.x1, r.y1); return; }
  fillDeskRect(r.x0, r.y0, r.x1, iy0);       // above
  fillDeskRect(r.x0, iy1, r.x1, r.y1);       // below
  fillDeskRect(r.x0, iy0, ix0, iy1);         // left
  fillDeskRect(ix1, iy0, r.x1, iy1);         // right
}

// Start item g_desk.item of g_desk.seq from its first frame. `old` is the
// screen area that may still show something (the previous crop, or the whole
// scene); empty when it is already black. Frame 0 is painted in full over the
// old picture and only the part of `old` outside the new crop is blacked out,
// so a switch never flashes black (the panel has no framebuffer to hide it).
static void deskStartItem(uint32_t now, const DeskRect& old) {
  int idx = deskPackFind(g_desk.var >= 0 ? g_desk.seq.vars[g_desk.var] : g_desk.seq.names[g_desk.item]);
  if (idx < 0 && deskPackCount() > 0) idx = 0;        // unknown name: still show Clawd
  g_desk.index = (idx >= 0 && deskPackAnim(idx, g_desk.a)) ? (int16_t)idx : (int16_t)-1;
  g_desk.frame = 0;
  g_desk.held = false;
  const bool dirty = old.x1 > old.x0 && old.y1 > old.y0;
  if (dirty) deskClearExcept(old, deskCrop());
  const uint32_t t0 = clawdMicros();
  if (!deskDraw(0, dirty)) return;
  deskStat(clawdMicros() - t0, 0);
  g_desk.due = now + deskPackHold(g_desk.a, 0);
}

// Take over g_deskWant from its first item.
static void deskAdopt(uint32_t now, const DeskRect& old) {
  g_desk.seq = g_deskWant;
  g_desk.item = 0;
  g_desk.passes = 0;
  g_desk.var = -1;
  g_deskSwitch = g_deskSwitchNow = false;
  deskStartItem(now, old);
}

// Variants only mix into a single looping animation.
static bool deskHasVars(const DeskSeq& q) { return q.nv && q.n == 1 && !q.repeat; }

// A variant index at random, never the last one when there is a choice.
static int8_t deskPickVar(uint8_t nv, int8_t last) {
  if (nv < 2 || last < 0 || last >= nv) return (int8_t)clawdRandom(nv);
  const int8_t r = (int8_t)clawdRandom(nv - 1u);
  return r < last ? r : (int8_t)(r + 1);
}

// One step past the last frame: next item, a variant and back, loop back, or hold.
static void deskPassEnd(uint32_t now, uint32_t late) {
  if (g_deskSwitch) { deskAdopt(now, deskCrop()); return; }
  const DeskSeq& q = g_desk.seq;
  if (deskHasVars(q)) {
    if (g_desk.var >= 0) {                     // a variant's one pass: back to the animation
      g_desk.var = -1;
      g_desk.passes = 0;
      deskStartItem(now, deskCrop());
      return;
    }
    if (++g_desk.passes >= CLAWD_VAR_EVERY) {
      g_desk.var = g_desk.lastVar = deskPickVar(q.nv, g_desk.lastVar);
      deskStartItem(now, deskCrop());
      return;
    }
  }
  if (g_desk.item + 1 < q.n || (q.repeat && q.n > 1)) {
    const DeskRect old = deskCrop();
    g_desk.item = (uint8_t)((g_desk.item + 1) % q.n);
    deskStartItem(now, old);
    return;
  }
  if (g_desk.a.flags & DESK_FLAG_ONCE) { g_desk.held = true; return; }
  const uint32_t t0 = clawdMicros();
  if (!deskDraw(g_desk.a.frameCount)) return;          // the wrap delta
  deskStat(clawdMicros() - t0, late);
  g_desk.frame = g_desk.a.loopStart;
  g_desk.due += deskPackHold(g_desk.a, g_desk.frame);
}

static void deskStep(uint32_t now) {
  if (g_desk.index < 0) return;
  if (g_deskSwitch && (g_deskSwitchNow || g_desk.held || now - g_deskSwitchSince >= DESK_SWITCH_WAIT_MS)) {
    deskAdopt(now, deskCrop());
    return;
  }
  if (g_desk.held || (int32_t)(now - g_desk.due) < 0) return;
  const uint32_t late = (now - g_desk.due) * 1000UL;
  if (g_desk.frame + 1u >= g_desk.a.frameCount) {
    deskPassEnd(now, late);
  } else {
    const uint32_t t0 = clawdMicros();
    if (!deskDraw(g_desk.frame + 1)) return;
    deskStat(clawdMicros() - t0, late);
    g_desk.frame++;
    g_desk.due += deskPackHold(g_desk.a, g_desk.frame);
  }
  // Far behind (a long web request, a repaint): drop the debt instead of racing.
  if ((int32_t)(now - g_desk.due) > 250) g_desk.due = now;
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

static bool deskHasLine() { return g_deskText[0] || g_cdOn; }

enum DeskRestart : uint8_t { DESK_STEP, DESK_ON_BLACK, DESK_IN_PLACE };

// DESK_ON_BLACK: the screen was just cleared; DESK_IN_PLACE: the desk is still
// on screen (new sequence, or the text line came or went). Both restart the
// sequence from its first frame; in place it is painted over the old scene.
static void deskTick(uint32_t now, DeskRestart restart) {
  const bool force = restart != DESK_STEP;
  if (force) {
    g_desk.shift = deskHasLine() ? DESK_TEXT_SHIFT : 0;
    g_desk.limit = deskHasLine() ? DESK_TEXT_TOP : CLAWD_H;
    if (restart == DESK_IN_PLACE) {
      // the line band is redrawn by drawStatus below; the scene above it in place
      if (deskHasLine()) fillDeskRect(0, g_desk.limit, CLAWD_W, CLAWD_H);
      deskAdopt(now, DeskRect{0, 0, CLAWD_W, g_desk.limit});
      if (g_desk.index < 0) fillDeskRect(0, 0, CLAWD_W, CLAWD_H);
    } else {
      deskAdopt(now, DESK_NONE);
    }
    if (g_desk.index < 0) {
      static const char* const msg = "desk.pak missing";
      ClawdText t = {&CF_STYRENE_14, msg,
                     (int16_t)(CLAWD_W / 2 - clawdTextWidth(CF_STYRENE_14, msg) / 2),
                     (int16_t)(CLAWD_H / 2 - 8), CLAWD_DIM};
      clawdRender(0, CLAWD_H / 2 - 8, CLAWD_W, 20, CLAWD_BG, nullptr, 0, &t, 1);
    }
  } else {
    deskStep(now);
  }
  char line[112];
  deskLine(now, line, sizeof(line));
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
    if (fullBottom()) { g_dirtyPanels = true; g_sigValid = false; }   // the alert line covered the bottom cards
  }
}

// ---- public ----------------------------------------------------------------------
void clawdUiBegin() {
  memset(&g_data, 0, sizeof(g_data));
  g_data.sessionResetMin = g_data.weeklyResetMin = -1;
  clawdPackOpen();
  deskPackOpen();
  g_desk.index = -1;
  g_deskSwitch = false;
  g_deskDirty = true;
  g_splash.begin();
  g_mascot.begin(MASCOT_X, MASCOT_Y);
  g_walker.begin(4, CLAWD_W - 4, PIX_FLOOR_Y);
  g_mini.begin("cloud", CLAWD_W / 2, IDLE_CY, IDLE_PX);
  g_view = VIEW_NONE;
  g_dirtyAll = true;
}

void clawdUiInvalidate() {
  g_view = VIEW_NONE;
  g_dirtyAll = true;
}

void clawdUiSetAddress(const char* ip) {
  if (!ip) ip = "";
  strncpy(g_addr, ip, sizeof(g_addr) - 1);
  g_addr[sizeof(g_addr) - 1] = 0;
}

// The splash's bottom band (it never paints there while the band is on).
static void drawAddrBand() {
  const ClawdFont& f = CF_STYRENE_14;
  const int y = CLAWD_H - CLAWD_SPLASH_BAND_PX;
  ClawdText t = {&f, g_addr, (int16_t)(CLAWD_W / 2 - clawdTextWidth(f, g_addr) / 2),
                 (int16_t)(y + (CLAWD_SPLASH_BAND_PX - f.lineHeight) / 2), CLAWD_DIM};
  clawdRender(0, y, CLAWD_W, CLAWD_SPLASH_BAND_PX, CLAWD_BG, nullptr, 0, &t, g_addr[0] ? 1 : 0);
  strcpy(g_addrDrawn, g_addr);
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
    if (!wasOn) { g_statusDrawn[0] = 0; g_floorWipe = true; }
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

static bool deskSeqEqual(const DeskSeq& a, const DeskSeq& b) {
  if (a.n != b.n || a.repeat != b.repeat || a.nv != b.nv) return false;
  for (uint8_t i = 0; i < a.n; i++)
    if (strcmp(a.names[i], b.names[i]) != 0) return false;
  for (uint8_t i = 0; i < a.nv; i++)
    if (strcmp(a.vars[i], b.vars[i]) != 0) return false;
  return true;
}

void clawdUiState(const ClawdState& s) {
  const uint32_t now = clawdNow();
  const uint32_t ttl = (uint32_t)(s.ttlSec == 0 ? 60 : (s.ttlSec > STATE_TTL_MAX ? STATE_TTL_MAX : s.ttlSec)) * 1000UL;
  g_dim = (s.dim >= 0 && s.dim <= 100) ? s.dim : -1;
  g_dimUntil = now + ttl;
  if (!s.animCount || !s.anim[0][0]) { g_deskOn = false; return; }
  DeskSeq want;
  for (uint8_t i = 0; i < s.animCount && i < CLAWD_SEQ_MAX; i++) {
    strncpy(want.names[i], s.anim[i], sizeof(want.names[i]) - 1);
    want.names[i][sizeof(want.names[i]) - 1] = 0;
    want.n = (uint8_t)(i + 1);
  }
  want.repeat = s.repeat;
  for (uint8_t i = 0; i < s.varCount && i < CLAWD_VAR_MAX; i++) {
    strncpy(want.vars[i], s.var[i], sizeof(want.vars[i]) - 1);
    want.vars[i][sizeof(want.vars[i]) - 1] = 0;
    want.nv = (uint8_t)(i + 1);
  }
  const bool wasLive = deskLive(now);
  if (!wasLive || g_desk.index < 0) {
    g_deskWant = want;
    g_desk.index = -2;                           // (re)start on the next service
  } else if (!deskSeqEqual(want, g_deskWant)) {
    g_deskWant = want;
    g_deskSwitch = true;                         // at the end of the current pass
    g_deskSwitchNow = s.urgent;
    g_deskSwitchSince = now;
  } else if (g_deskSwitch && s.urgent) {
    g_deskSwitchNow = true;
  }
  const bool cd = s.countdownSec > 0;
  const bool hadLine = deskHasLine();
  if (strcmp(s.text, g_deskText) != 0) {
    strncpy(g_deskText, s.text, sizeof(g_deskText) - 1);
    g_deskText[sizeof(g_deskText) - 1] = 0;
    g_deskSince = now;                           // restart the marquee
  }
  g_cdOn = cd;
  // Only a line that appears or goes moves the scene: that needs a repaint.
  // A changed line is just redrawn by deskTick.
  if (deskHasLine() != hadLine) g_deskDirty = true;
  if (cd) g_cdEndMs = now + (uint32_t)s.countdownSec * 1000UL;
  g_deskOn = true;
  g_deskUntil = now + ttl;
}

int clawdUiDim() {
  return (g_dim >= 0 && (int32_t)(g_dimUntil - clawdNow()) > 0) ? g_dim : -1;
}

bool clawdUiOnSplash() { return g_view == VIEW_SPLASH; }

bool clawdUiSpriteBox(int& x, int& y, int& w, int& h) {
  if (g_view != VIEW_USAGE && g_view != VIEW_IDLE) return false;
  if (chromeLayout() == CLAWD_LAYOUT_PIXEL) {       // the walker's floor band
    x = 0; y = PIX_FLOOR_Y - 38; w = CLAWD_W; h = 38;
    return true;
  }
  const bool ring = chromeLayout() == CLAWD_LAYOUT_RINGS;
  x = ring ? RING_MASCOT_X : MASCOT_X;
  y = ring ? RING_MASCOT_Y : MASCOT_Y;
  w = 56;                                           // the mascot window: 28 x 21 cells, 2 px each
  h = 42;
  return true;
}
bool clawdUiOnDesk() { return g_view == VIEW_DESK; }

const char* clawdUiAnimName() {
  static char name[16];
  name[0] = 0;
  if (g_view == VIEW_SPLASH) clawdAnimName(g_splash.current(), name, sizeof(name));
  else if (g_view == VIEW_DESK && g_desk.index >= 0) strncpy(name, g_desk.a.name, sizeof(name) - 1);
  return name;
}

void clawdUiDeskReleased() {
  deskPackClose();
  g_desk.index = -1;
  if (g_view == VIEW_DESK) g_dirtyAll = true;    // shows "desk.pak missing"
}

static uint32_t percentile(const uint16_t* v, uint16_t n, int pct) {
  if (!n) return 0;
  uint16_t s[DESK_STATS];
  memcpy(s, v, n * sizeof(uint16_t));
  for (uint16_t i = 1; i < n; i++)              // insertion sort: 128 values, on request only
    for (uint16_t j = i; j > 0 && s[j - 1] > s[j]; j--) { uint16_t t = s[j]; s[j] = s[j - 1]; s[j - 1] = t; }
  return (uint32_t)s[(uint32_t)(n - 1) * pct / 100] * 100UL;
}

void clawdUiDeskStats(ClawdDeskStats& o) {
  o.frames = g_statN;
  o.drawP50 = percentile(g_drawSamples, g_statN, 50);
  o.drawP95 = percentile(g_drawSamples, g_statN, 95);
  o.drawMax = percentile(g_drawSamples, g_statN, 100);
  o.lateP95 = percentile(g_lateSamples, g_statN, 95);
  o.total = g_statTotal;
}

const char* clawdUiDeskAnimName() {
  if (g_view != VIEW_DESK || g_desk.index < 0) return "";
  return g_desk.var >= 0 ? g_desk.seq.vars[g_desk.var] : g_desk.seq.names[g_desk.item];
}

void clawdUiService(uint8_t screen, int8_t fixedAnim, uint8_t layout) {
  const uint32_t now = clawdNow();
  if (layout > 7 || !((CLAWD_LAYOUTS >> layout) & 1u)) layout = CLAWD_LAYOUT_CLASSIC;
  if (layout != g_layout) {
    g_layout = layout;
    g_dirtyAll = true;                 // different card geometry: repaint everything
  }
  if (g_alert && now - g_alertSince >= ALERT_TIMEOUT_MS) endAlert();

  const bool fresh = dataFresh(now);
  View want;
  if (deskLive(now))                       want = VIEW_DESK;
  else if (g_alert)                        want = fresh ? VIEW_USAGE : VIEW_IDLE;
  else if (screen == CLAWD_SCREEN_SPLASH)  want = VIEW_SPLASH;
  else if (screen == CLAWD_SCREEN_USAGE)   want = fresh ? VIEW_USAGE : VIEW_IDLE;
  else                                     want = fresh ? VIEW_USAGE : VIEW_SPLASH;
  // The title and mascot follow the layout while usage is up (never on the idle
  // screen), and an Enterprise payload falls back to the classic one.
  const uint8_t eff = (want == VIEW_USAGE) ? usageLayout() : (uint8_t)CLAWD_LAYOUT_CLASSIC;
  if (eff != g_effDrawn) {
    g_effDrawn = eff;
    g_dirtyAll = true;
    g_mascot.place(eff == CLAWD_LAYOUT_RINGS ? RING_MASCOT_X : MASCOT_X,
                   eff == CLAWD_LAYOUT_RINGS ? RING_MASCOT_Y : MASCOT_Y);
  }
  // Auto screen + splash means no fresh data: caption the IP under it. A splash
  // picked on purpose (screen = splash) stays full height.
  const bool band = want == VIEW_SPLASH && screen == CLAWD_SCREEN_AUTO;
  if (want == VIEW_SPLASH && g_view == VIEW_SPLASH && band != g_splash.bottomBand())
    g_dirtyAll = true;               // band came or went: repaint the splash

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
    if (want == VIEW_DESK && was == VIEW_DESK) {   // repaint the desk without a black flash
      g_deskDirty = false;
      g_statusDrawn[0] = 0;
      deskTick(now, DESK_IN_PLACE);
      return;
    }
    fillRect(0, 0, CLAWD_W, CLAWD_H, CLAWD_BG);
    g_frameOn = false;
    if (want == VIEW_DESK) {
      g_splash.hide();
      g_deskDirty = false;
      g_statusDrawn[0] = 0;
      deskTick(now, DESK_ON_BLACK);
      return;
    }
    if (want == VIEW_SPLASH) {
      shownAnim = fixedAnim;
      g_splash.setBottomBand(band);
      g_splash.show(fixedAnim);
      if (band) drawAddrBand();
      if (!clawdPackOk()) {          // animations not uploaded: say so, don't sit black
        static const char* const msg = "clawd.pak missing";
        ClawdText t = {&CF_STYRENE_14, msg,
                       (int16_t)(CLAWD_W / 2 - clawdTextWidth(CF_STYRENE_14, msg) / 2),
                       (int16_t)(CLAWD_H / 2 - 8), CLAWD_DIM};
        clawdRender(0, CLAWD_H / 2 - 8, CLAWD_W, 20, CLAWD_BG, nullptr, 0, &t, 1);
      }
    } else {
      g_splash.hide();
      g_sigValid = false;
      // the splash borrowed the mascot's frame buffer, and the walker shares it: reload the one in use
      if (chromeLayout() == CLAWD_LAYOUT_PIXEL) g_walker.reset();
      else if (was == VIEW_SPLASH || was == VIEW_NONE || was == VIEW_USAGE || was == VIEW_IDLE) g_mascot.reset();
      g_mascot.invalidate();
      g_mini.reset();
      g_dirtyTitle = true;
      g_dirtyPanels = true;
      g_statusDrawn[0] = 0;
      // Layouts that reach the bottom (pace, hero, rings, pixel) only show an alert there;
      // pixel's floor is not repainted by the panels, so a spinner would stay on it.
      if (!(fullBottom() && g_view == VIEW_USAGE) || g_alert) tickStatus(now, true);
    }
  }

  if (g_view == VIEW_DESK) {
    deskTick(now, DESK_STEP);
    return;
  }
  if (g_view == VIEW_SPLASH) {
    if (g_splash.bottomBand() && strcmp(g_addr, g_addrDrawn) != 0) drawAddrBand();
    g_splash.tick();
    return;
  }

  // usage / idle
  if (g_alert) {
    const bool on = ((now - g_alertSince) / 600) % 2 == 0;
    if (on != g_frameOn) drawFrame(on);
  }
  if (g_floorWipe) {                  // the alert line replaces Clawd's stroll: no sprite left under it
    g_floorWipe = false;
    if (g_view == VIEW_USAGE && chromeLayout() == CLAWD_LAYOUT_PIXEL)
      fillRect(0, PIX_FLOOR_Y - 38, CLAWD_W, 38, CLAWD_BG);
  }
  char probe[16];
  int minute;
  titleText(now, probe, sizeof(probe), &minute);
  if (g_dirtyTitle || minute != g_clockMin) {
    g_dirtyTitle = false;
    drawTitle(now);
  }
  if (g_view == VIEW_USAGE) {
    if (g_dirtyPanels) {
      g_dirtyPanels = false;
      const uint8_t l = usageLayout();
      const bool sigLayout = l >= CLAWD_LAYOUT_HERO;     // the new layouts skip a paint that would show the same
      const uint32_t sig = sigLayout ? panelSig(now) : 0;
      if (!sigLayout || !g_sigValid || sig != g_sig) {
        g_sig = sig;
        g_sigValid = sigLayout;
        drawPanels();
        if (l == CLAWD_LAYOUT_PIXEL) g_walker.invalidate();
      }
    }
  } else {
    g_dirtyPanels = true;          // repaint the panels when data comes back
    g_mini.tick();
  }
  if (chromeLayout() == CLAWD_LAYOUT_PIXEL) {
    if (!g_alert) g_walker.tick(g_rateGroup);          // the alert line sits over its floor
  } else {
    g_mascot.tick(g_rateGroup);
  }
  // Those layouts reach the bottom: the status line only shows an alert there.
  if (!(fullBottom() && g_view == VIEW_USAGE) || g_alert) tickStatus(now, false);
}
