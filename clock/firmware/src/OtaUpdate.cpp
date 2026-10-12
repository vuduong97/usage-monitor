#include "OtaUpdate.h"
#include "config.h"

// Clawdmeter build: no GitHub self-update and no TLS at all. The image has to
// stay small enough (< ~550 KB) for the clock's stock 4m3m flash layout, where
// OTA staging space is what is left of the 1 MB sketch area. Manual /update
// (and flashing the stock firmware back through it) is unaffected.
OtaLatest otaCheckLatest(const Settings&) {
  OtaLatest r;
  r.error = F("disabled in the Clawdmeter build: upload firmware.bin via /update");
  return r;
}
String otaUpdateFromGitHub(const Settings&) { return F("disabled in the Clawdmeter build"); }
bool   otaBootRequested() { return false; }
bool   otaRequestBootUpdate(const char*) { return false; }
void   otaBootUpdate(const Settings&) {}
String otaTakeBootResult() { return String(); }
