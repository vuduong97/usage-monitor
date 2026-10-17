#pragma once
// Crash record that survives a soft reset (ESP8266 RTC user memory).
//
// On this core a panic, abort, assert or cont-stack overflow restarts through
// the SDK's system_restart_local, so the next boot reports "Software/System
// restart" exactly like ESP.restart(). custom_crash_callback() runs on every
// crash path and leaves a record here; crashLogMarkRestart() marks a planned
// restart. crashLogBegin() at boot tells the two apart.
#include <Arduino.h>

void crashLogBegin();          // once, early in setup()
void crashLogMarkRestart();    // right before a planned ESP.restart()
bool crashLogBootWasCrash();   // this boot followed a crash
// "n=2 reason=254 cause=0 epc=0x... vaddr=0x... sp: 0x402... 0x402..." or "".
const String& crashLogLast();
