// Minimal Arduino core stand-in for host tests of Settings.cpp (String, Print,
// Stream, constrain). Only what the settings code and ArduinoJson touch.
#pragma once
#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <strings.h>

#define PROGMEM
#define HEX 16
#define DEC 10
#define F(x) (x)

template <class T, class L, class H>
static inline T constrain(T v, L lo, H hi) { return v < (T)lo ? (T)lo : (v > (T)hi ? (T)hi : v); }

class String {
 public:
  String() {}
  String(const char* c) : s_(c ? c : "") {}
  String(const std::string& v) : s_(v) {}
  String(char c) : s_(1, c) {}
  String(unsigned v, int base = 10) { s_ = num(v, base); }
  String(int v, int base = 10) { s_ = num((unsigned)v, base); }
  String(unsigned long v, int base = 10) { s_ = num(v, base); }
  size_t length() const { return s_.size(); }
  const char* c_str() const { return s_.c_str(); }
  char operator[](size_t i) const { return s_[i]; }
  void remove(size_t from) { if (from < s_.size()) s_.erase(from); }
  bool equalsIgnoreCase(const String& o) const { return strcasecmp(s_.c_str(), o.s_.c_str()) == 0; }
  bool concat(const char* c, unsigned n) { s_.append(c, n); return true; }
  bool concat(const char* c) { s_ += c; return true; }
  String& operator+=(const char* c) { s_ += c; return *this; }
  String& operator+=(const String& o) { s_ += o.s_; return *this; }
  String& operator+=(char c) { s_ += c; return *this; }
  bool operator==(const String& o) const { return s_ == o.s_; }
  bool operator!=(const String& o) const { return s_ != o.s_; }
  bool operator==(const char* c) const { return s_ == c; }
  bool operator!=(const char* c) const { return s_ != c; }
  friend String operator+(String a, const String& b) { a.s_ += b.s_; return a; }
  friend String operator+(String a, const char* b) { a.s_ += b; return a; }
  friend String operator+(const char* a, const String& b) { return String(a) + b; }
 private:
  static std::string num(unsigned long v, int base) {
    char b[40];
    snprintf(b, sizeof b, base == 16 ? "%lx" : "%lu", v);
    return b;
  }
  std::string s_;
};

class Print;
class Printable {
 public:
  virtual ~Printable() {}
  virtual size_t printTo(Print& p) const = 0;
};

class Print {
 public:
  virtual ~Print() {}
  virtual size_t write(uint8_t) = 0;
  virtual size_t write(const uint8_t* b, size_t n) { size_t k = 0; while (k < n && write(b[k])) k++; return k; }
  int getWriteError() { return 0; }
};

class Stream : public Print {
 public:
  virtual int read() = 0;
  virtual size_t readBytes(char* b, size_t n) { size_t k = 0; int c; while (k < n && (c = read()) >= 0) b[k++] = (char)c; return k; }
};
