Import("env")

# clawd only: the ESP8266 Arduino core force-links newlib's float
# printf/scanf (`-u _printf_float -u _scanf_float`, ~8 KB). The clawd build
# never formats or parses a float through printf/scanf (ticker, the only %f
# user, is compiled out; dtostrf/String(float) do not use printf), and the
# 4m3m OTA slot has no room to spare. A %f would print nothing, not crash.
flags = list(env.get("LINKFLAGS", []))
out, i = [], 0
while i < len(flags):
    if flags[i] == "-u" and i + 1 < len(flags) and flags[i + 1] in ("_printf_float", "_scanf_float"):
        i += 2
        continue
    out.append(flags[i])
    i += 1
env.Replace(LINKFLAGS=out)
