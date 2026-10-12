#include "WgClient.h"
#include "Net.h"
#include "Platform.h"

// Not compiled in (ESP8266, or any build without SMALLTV_WIREGUARD). The stubs
// keep every call site free of #ifdefs; the status says so explicitly so the
// web UI can explain the section is missing rather than hiding it.

void wgBegin(const Settings& s, bool bootWasCrash) { (void)s; (void)bootWasCrash; }
void wgReapply(const Settings& s) { (void)s; }
void wgService(const Settings& s) { (void)s; }

void wgStatusJson(JsonObject o) {
  o["compiledIn"] = false;
  o["enabled"] = false;
  o["up"] = false;
  o["held"] = false;
  o["badConfig"] = false;
}

bool wgHeld() { return false; }

bool wgNeedsClock(const Settings& s) { (void)s; return false; }

