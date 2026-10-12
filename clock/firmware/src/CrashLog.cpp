#include "CrashLog.h"

#if defined(SMALLTV_ESP8266)
#include <user_interface.h>

namespace {

// RTC user memory is 128 words; eboot's OTA command sits at the start, so the
// record lives near the end (offset in 4-byte blocks).
constexpr uint32_t kOffset = 96;
constexpr uint32_t kMagic = 0xC1A3D0C5;
constexpr uint32_t kAddrs = 12;

enum : uint32_t { kNone = 0, kCrashed = 1, kRestart = 2 };

struct Record {
  uint32_t magic;
  uint32_t pending;            // kCrashed / kRestart: what ended the last boot
  uint32_t count;              // crashes since power on
  uint32_t reason, exccause, epc1, excvaddr;
  uint32_t nAddr;
  uint32_t addr[kAddrs];       // code addresses found on the stack, for addr2line
  uint32_t sum;
};
static_assert(sizeof(Record) / 4 + kOffset <= 128, "RTC user memory is 512 B");

uint32_t checksum(const Record& r) {
  const uint32_t* w = reinterpret_cast<const uint32_t*>(&r);
  uint32_t s = 0x5A5A5A5A;
  for (size_t i = 0; i < offsetof(Record, sum) / 4; i++) s = (s << 5 | s >> 27) ^ w[i];
  return s;
}

bool load(Record& r) {
  if (!ESP.rtcUserMemoryRead(kOffset, reinterpret_cast<uint32_t*>(&r), sizeof(r))) return false;
  return r.magic == kMagic && r.sum == checksum(r) && r.nAddr <= kAddrs;
}

void store(Record& r) {
  r.magic = kMagic;
  r.sum = checksum(r);
  ESP.rtcUserMemoryWrite(kOffset, reinterpret_cast<uint32_t*>(&r), sizeof(r));
}

bool   g_wasCrash = false;
String g_last;

}  // namespace

// Called by the core's postmortem on exceptions, watchdogs, panic/abort/assert
// and stack overflow, before the restart.
extern "C" void custom_crash_callback(struct rst_info* ri, uint32_t stack, uint32_t stackEnd) {
  Record r;
  if (!load(r)) memset(&r, 0, sizeof(r));
  r.pending = kCrashed;
  r.count++;
  r.reason = ri ? ri->reason : 0;
  r.exccause = ri ? ri->exccause : 0;
  r.epc1 = ri ? ri->epc1 : 0;
  r.excvaddr = ri ? ri->excvaddr : 0;
  r.nAddr = 0;
  for (uint32_t p = stack; p + 4 <= stackEnd && r.nAddr < kAddrs; p += 4) {
    const uint32_t v = *reinterpret_cast<const uint32_t*>(p);
    if ((v >= 0x40200000 && v < 0x40300000) || (v >= 0x40100000 && v < 0x40108000))
      r.addr[r.nAddr++] = v;   // flash or IRAM code: a return address, most likely
  }
  store(r);
}

void crashLogBegin() {
  Record r;
  if (!load(r)) {
    memset(&r, 0, sizeof(r));
    store(r);
    return;
  }
  g_wasCrash = r.pending == kCrashed;
  if (r.count) {
    char head[96];
    snprintf(head, sizeof(head), "n=%u reason=%u cause=%u epc=0x%08x vaddr=0x%08x sp:",
             (unsigned)r.count, (unsigned)r.reason, (unsigned)r.exccause,
             (unsigned)r.epc1, (unsigned)r.excvaddr);
    g_last = head;
    char a[12];
    for (uint32_t i = 0; i < r.nAddr; i++) {
      snprintf(a, sizeof(a), " 0x%08x", (unsigned)r.addr[i]);
      g_last += a;
    }
  }
  r.pending = kNone;
  store(r);
}

void crashLogMarkRestart() {
  Record r;
  if (!load(r)) memset(&r, 0, sizeof(r));
  r.pending = kRestart;
  store(r);
}

bool crashLogBootWasCrash() { return g_wasCrash; }
const String& crashLogLast() { return g_last; }

#else   // ESP32: esp_reset_reason() already tells panics from restarts

static String g_none;
void crashLogBegin() {}
void crashLogMarkRestart() {}
bool crashLogBootWasCrash() { return false; }
const String& crashLogLast() { return g_none; }

#endif
