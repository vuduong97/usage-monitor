---
title: clock unreachable after a replug at the office
date: 2026-10-12
summary: after the clock was unplugged and plugged back in, nothing could reach it (Mac with or without the app, phone). The user puts it down to the office WiFi blocking devices from each other. The clawd-desktop LAN scan now covers the whole subnet instead of only a /24 (commit 27e5b34)
---

# Clock unreachable after a replug

## Symptom
At about 10:20 the clock was unplugged and plugged back in. The Clawd app logged a push `ConnectTimeout`, then "not answering" after 3 failures. After that, neither mDNS nor the LAN scan found the clock.

## Evidence
- At boot the clock still showed WiFi `FRT ISC - ECOM` and IP 10.97.44.122, so DHCP worked.
- The clock then showed the "no data" splash (`magnifier`).
- From the Mac:
  - ping: no route to host; the ARP entry for .122 stayed `(incomplete)`;
  - HTTP: failed;
  - a fresh zeroconf resolve: timed out after 8 s. `dns-sd` answers came only from the mDNSResponder cache;
  - a /23 TCP sweep and an ARP sweep found no Espressif device.
- With the app quit, a 6-minute watch after another replug (every 2 s) never reached the clock.
- The user's phone on the same WiFi could not open `http://10.97.44.122` either.
- The router and other hosts answered normally.
- Conclusion: the clock was not reachable from anything on the LAN, so the app is not the cause. The user says the office WiFi has done this on and off before (lumibox app). Weak signal (−76 dBm) or ESP8266 modem sleep were not ruled out. No firmware change was made.

## Fix in clawd-desktop (27e5b34)
- Found while debugging: the office network is a /23, but the LAN scan only swept the /24.
- `scan_candidates` now takes the interface netmask, read via `ifaddr`, which zeroconf already depends on. The sweep is capped at a /22. The /23 sweep takes about 4 s.
- The tray "not found" text and a README troubleshooting row now mention that office and guest networks can block devices from reaching each other.
- 229 tests pass. The app was rebuilt and reinstalled in /Applications.

## Next
- Getting the clock back at the office means either waiting for the network to let it through, or putting the Mac and the clock on a phone hotspot or their own small router.
- Adding a second WiFi to the clock needs its web page, which is only reachable while the network lets it through, or from setup mode.
