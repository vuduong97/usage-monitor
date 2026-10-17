// Host stand-in for LittleFS: a File over std::fstream-less memory, enough for
// loadSettings/saveSettings (open r/w, rename, remove).
#pragma once
#include <Arduino.h>
#include <map>
#include <memory>
#include <memory>

class File : public Stream {
 public:
  File() {}
  File(std::shared_ptr<std::string> d, bool w) : d_(d), w_(w) { if (w) d_->clear(); }
  explicit operator bool() const { return (bool)d_; }
  int read() override { return (!w_ && d_ && pos_ < d_->size()) ? (unsigned char)(*d_)[pos_++] : -1; }
  size_t write(uint8_t c) override { if (!w_ || !d_) return 0; d_->push_back((char)c); return 1; }
  size_t write(const uint8_t* b, size_t n) override { if (!w_ || !d_) return 0; d_->append((const char*)b, n); return n; }
  void flush() {}
  size_t size() const { return d_ ? d_->size() : 0; }
  void close() {}
 private:
  std::shared_ptr<std::string> d_;
  bool w_ = false;
  size_t pos_ = 0;
};

struct FakeLittleFS {
  std::map<std::string, std::shared_ptr<std::string>> files;
  bool begin() { return true; }
  File open(const char* p, const char* mode) {
    if (mode[0] == 'w') { auto d = std::make_shared<std::string>(); files[p] = d; return File(d, true); }
    auto it = files.find(p);
    return it == files.end() ? File() : File(it->second, false);
  }
  bool rename(const char* a, const char* b) { auto it = files.find(a); if (it == files.end()) return false; files[b] = it->second; files.erase(a); return true; }
  bool remove(const char* p) { return files.erase(p) > 0; }
};
extern FakeLittleFS LittleFS;
