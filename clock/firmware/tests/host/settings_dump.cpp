// Host test driver: read a config JSON on stdin, apply it with settingsApplyJson
// onto defaults, and print settingsToJson (secrets included, as saved to flash).
// Built against any Settings.cpp, so the same input can be run through two
// firmware versions and the outputs compared.
#include <iostream>
#include <iterator>
#include <sstream>
#include <LittleFS.h>
#include "Settings.h"

FakeLittleFS LittleFS;

int main(int argc, char** argv) {
  std::string in((std::istreambuf_iterator<char>(std::cin)), std::istreambuf_iterator<char>());
  JsonDocument src;
  if (deserializeJson(src, in)) { std::cerr << "bad input json\n"; return 2; }
  Settings s;
  s.setDefaults();
  settingsApplyJson(s, src.as<JsonObjectConst>());
  // Optional 2nd patch (argv[1]): what the web UI POSTs on Save, applied on top
  // of the stored config. Keys it omits (secrets!) must survive.
  JsonDocument patch;
  if (argc > 1) {
    FILE* f = fopen(argv[1], "rb");
    if (!f) { std::cerr << "cannot open patch\n"; return 5; }
    std::string p2; char buf[4096]; size_t n;
    while ((n = fread(buf, 1, sizeof buf, f)) > 0) p2.append(buf, n);
    fclose(f);
    if (deserializeJson(patch, p2)) { std::cerr << "bad patch json\n"; return 6; }
    settingsApplyJson(s, patch.as<JsonObjectConst>());
  }
  // Through the real save/load path too: save to the fake flash, load it back.
  if (!saveSettings(s)) { std::cerr << "save failed\n"; return 3; }
  Settings t;
  if (!loadSettings(t)) { std::cerr << "load failed\n"; return 4; }
  JsonDocument out;
  JsonObject root = out.to<JsonObject>();
  settingsToJson(t, root, true);
  serializeJsonPretty(out, std::cout);
  std::cout << "\n";
  return 0;
}
