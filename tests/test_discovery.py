"""Clock discovery: mDNS first, the remembered clock, LAN scan, manual address."""
import asyncio

import httpx

from clawd_desktop import discovery as dsc
from clawd_desktop.discovery import Clock, ClockFinder, ClockStore


class FakeBrowser:
    def __init__(self, clocks=()):
        self.clocks = {f"{c.name}._clawd._tcp.local.": c for c in clocks}
        self.changed = 0

    def set(self, *clocks):
        self.clocks = {f"{c.name}._clawd._tcp.local.": c for c in clocks}
        self.changed += 1

    def start(self):
        return True

    def stop(self):
        pass

    def names(self):
        return sorted(self.clocks)

    def resolve(self, name):
        return self.clocks.get(name)


class Now:
    def __init__(self):
        self.t = 1000.0

    def __call__(self):
        return self.t


def make(tmp_path, clocks=(), scan=(), manual=(), saved=None):
    store = ClockStore(tmp_path / "clock.json")
    if saved is not None:
        store.save(saved)
    calls = []

    async def scanner():
        calls.append(1)
        return list(scan)
    now = Now()
    f = ClockFinder(store, manual=lambda: list(manual), browser=FakeBrowser(clocks),
                    scanner=scanner, clock=now)
    return f, store, now, calls


def step(f):
    asyncio.run(f.step())


A = Clock(host="10.0.0.7", name="clawd", version="2.16.0")


def test_first_mdns_clock_is_used_and_remembered(tmp_path):
    f, store, _now, calls = make(tmp_path, clocks=[A])
    assert f.targets() == []
    step(f)
    assert f.targets() == ["http://10.0.0.7/api/usage"]
    assert store.load() == {"name": "clawd", "host": "10.0.0.7", "port": 80, "version": "2.16.0"}
    assert calls == []


def test_saved_clock_is_tried_at_once_then_follows_its_new_address(tmp_path):
    f, store, _now, _ = make(tmp_path, saved={"name": "clawd", "host": "192.168.1.48", "port": 80})
    assert f.targets() == ["http://192.168.1.48/api/usage"]      # before mDNS answers
    f.browser.set(Clock(host="192.168.1.48", name="clawd"))
    step(f)
    assert f.targets() == ["http://192.168.1.48/api/usage"]
    assert store.load()["host"] == "192.168.1.48"


def test_remembered_clock_wins_over_another_one(tmp_path):
    other = Clock(host="10.0.0.9", name="aaa-neighbour")
    f, _store, _now, _ = make(tmp_path, clocks=[other, A], saved={"name": "clawd", "host": "1.2.3.4"})
    step(f)
    assert f.targets() == ["http://10.0.0.7/api/usage"]


def test_config_http_targets_bypass_discovery(tmp_path):
    f, store, _now, calls = make(tmp_path, clocks=[A], manual=["http://192.168.0.5/api/usage"])
    step(f)
    assert f.targets() == ["http://192.168.0.5/api/usage"]
    assert store.load() == {} and calls == []


def test_failing_clock_is_dropped_and_found_again_by_scan(tmp_path):
    f, store, now, calls = make(tmp_path, saved={"host": "10.0.0.7"},
                                scan=[Clock(host="10.0.0.50", version="2.16.0", source="scan")])
    url = "http://10.0.0.7/api/usage"
    for _ in range(dsc.FAIL_LIMIT - 1):
        f.report(url, False)
    assert f.targets() == [url]
    f.report(url, False)
    assert f.targets() == []                      # stop polling for a screen that is not there
    step(f)                                       # too soon after start: no scan yet
    assert calls == [] and f.targets() == []
    now.t += dsc.SCAN_AFTER
    step(f)
    assert calls == [1]
    assert f.targets() == ["http://10.0.0.50/api/usage"]
    f.report("http://10.0.0.50/api/usage", True)
    assert store.load()["host"] == "10.0.0.50"


def test_dropped_clock_is_retried_after_the_hold(tmp_path):
    f, _store, now, _calls = make(tmp_path, saved={"host": "10.0.0.7"})
    for _ in range(dsc.FAIL_LIMIT):
        f.report("http://10.0.0.7/api/usage", False)
    step(f)
    assert f.targets() == []
    now.t += dsc.BAD_HOLD + 1
    step(f)
    assert f.targets() == ["http://10.0.0.7/api/usage"]


def test_scanned_clock_takes_its_mdns_name_later(tmp_path):
    f, store, now, _ = make(tmp_path, scan=[Clock(host="10.0.0.7", source="scan")])
    now.t += dsc.SCAN_AFTER
    step(f)
    assert f.current.source == "scan"
    f.browser.set(A)
    step(f)
    assert f.current == A and store.load()["name"] == "clawd"


def test_manual_address_and_forget(tmp_path):
    f, store, _now, _ = make(tmp_path, clocks=[A])
    assert f.set_manual("not a host:99999") is None
    c = f.set_manual(" 192.168.1.80 ")
    assert c.source == "manual" and f.targets() == ["http://192.168.1.80/api/usage"]
    step(f)
    assert f.targets() == ["http://192.168.1.80/api/usage"]     # mDNS does not override it
    for _ in range(dsc.FAIL_LIMIT + 2):
        f.report("http://192.168.1.80/api/usage", False)
    assert f.targets() == ["http://192.168.1.80/api/usage"]     # nor do failures
    # a new app start keeps the typed address
    f2 = ClockFinder(store, browser=FakeBrowser([A]), scanner=None)
    assert f2.targets() == ["http://192.168.1.80/api/usage"]
    f.forget()
    assert f.targets() == [] and store.load() == {}
    step(f)
    assert f.targets() == ["http://10.0.0.7/api/usage"]


def test_manual_clock_parsing():
    assert dsc.manual_clock("clawd.local:8080").base == "http://clawd.local:8080"
    assert dsc.manual_clock("http://10.0.0.5/whatever").usage_url == "http://10.0.0.5/api/usage"
    assert dsc.manual_clock("") is None and dsc.manual_clock(":80") is None
    assert dsc.manual_clock("h:abc") is None


def test_scan_candidates_follow_the_netmask():
    wide = dsc.scan_candidates("10.97.44.245", 23)          # office /23: both halves
    assert len(wide) == 509 and "10.97.45.12" in wide and "10.97.44.245" not in wide
    assert len(dsc.scan_candidates("10.1.2.3", 16)) == 1021   # capped at a /22
    assert len(dsc.scan_candidates("192.168.1.20", 28)) == 13
    assert dsc.local_prefix(None) == 24


def test_scan_candidates_cover_the_slash_24():
    hosts = dsc.scan_candidates("192.168.1.20")
    assert len(hosts) == 253 and "192.168.1.48" in hosts and "192.168.1.20" not in hosts
    assert dsc.scan_candidates(None) == []


def test_scan_lan_keeps_only_clawd_firmware():
    def handler(req: httpx.Request) -> httpx.Response:
        host = req.url.host
        if host == "10.0.0.2":
            return httpx.Response(200, json={"heap": 1, "pak": 17, "desk": 30, "version": "2.16.0"})
        if host == "10.0.0.3":
            return httpx.Response(200, json={"heap": 1})          # some other device
        if host == "10.0.0.4":
            return httpx.Response(401)
        raise httpx.ConnectError("refused", request=req)
    found = asyncio.run(dsc.scan_lan(["10.0.0.2", "10.0.0.3", "10.0.0.4", "10.0.0.5"],
                                     transport=httpx.MockTransport(handler)))
    assert found == [Clock(host="10.0.0.2", version="2.16.0", source="scan")]


def test_describe_reports_state(tmp_path):
    f, _store, _now, _ = make(tmp_path, clocks=[A])
    d = f.describe()
    assert d["clock"] is None and d["failing"]
    step(f)
    f.report(A.usage_url, True)
    d = f.describe()
    assert d["clock"] == A and not d["failing"] and d["last_ok"]


def test_lan_blocked_heuristic():
    import errno
    from clawd_desktop.discovery import lan_blocked
    assert lan_blocked([errno.EHOSTUNREACH] * 253)
    assert not lan_blocked([errno.EHOSTUNREACH] * 250 + [errno.ECONNREFUSED])   # router answered
    assert not lan_blocked([errno.EHOSTUNREACH] * 250 + [0])                    # something is open
    assert not lan_blocked([None] * 200 + [errno.EHOSTUNREACH] * 53)            # timeouts: normal LAN
    assert not lan_blocked([])


def test_blocked_scan_is_reported_and_retried_sooner(tmp_path):
    from clawd_desktop import discovery
    blocked = discovery.ScanResult()
    blocked.blocked = True
    f, _store, now, calls = make(tmp_path)
    f.scanner = lambda: asyncio.sleep(0, result=blocked)
    now.t += discovery.SCAN_AFTER + 1
    step(f)
    assert f.describe()["lan_blocked"] is True
    now.t += discovery.RESCAN_BLOCKED + 1
    f.scanner = lambda: asyncio.sleep(0, result=discovery.ScanResult([A]))
    step(f)
    assert f.current is not None and f.current.host == A.host
    assert f.describe()["lan_blocked"] is False
