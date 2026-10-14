"""Clock discovery: mDNS, the remembered clock, a typed address. Never a network scan."""
import asyncio
import inspect

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


def make(tmp_path, clocks=(), manual=(), saved=None):
    store = ClockStore(tmp_path / "clock.json")
    if saved is not None:
        store.save(saved)
    now = Now()
    f = ClockFinder(store, manual=lambda: list(manual), browser=FakeBrowser(clocks), clock=now)
    return f, store, now


def step(f):
    asyncio.run(f.step())


def drop(f, url):
    for _ in range(dsc.FAIL_LIMIT):
        f.report(url, False)


A = Clock(host="10.0.0.7", name="clawd", version="2.16.0")


def test_there_is_no_port_scan_anywhere():
    src = inspect.getsource(dsc)
    for gone in ("scan_lan", "open_connection", "httpx", "ip_network", '"subnets"'):
        assert gone not in src, gone


def test_first_mdns_clock_is_used_and_remembered(tmp_path):
    f, store, _now = make(tmp_path, clocks=[A])
    assert f.targets() == []
    step(f)
    assert f.targets() == ["http://10.0.0.7/api/usage"]
    assert store.load() == {"name": "clawd", "host": "10.0.0.7", "port": 80, "version": "2.16.0"}


def test_saved_clock_is_tried_at_once_then_follows_its_new_address(tmp_path):
    f, store, _now = make(tmp_path, saved={"name": "clawd", "host": "192.168.1.48", "port": 80})
    assert f.targets() == ["http://192.168.1.48/api/usage"]      # before mDNS answers
    f.browser.set(Clock(host="192.168.1.60", name="clawd"))      # the router gave it a new IP
    step(f)
    assert f.targets() == ["http://192.168.1.60/api/usage"]
    assert store.load()["host"] == "192.168.1.60"


def test_remembered_clock_wins_over_another_one(tmp_path):
    other = Clock(host="10.0.0.9", name="aaa-neighbour")
    f, _store, _now = make(tmp_path, clocks=[other, A], saved={"name": "clawd", "host": "1.2.3.4"})
    step(f)
    assert f.targets() == ["http://10.0.0.7/api/usage"]


def test_config_http_targets_bypass_discovery(tmp_path):
    f, store, _now = make(tmp_path, clocks=[A], manual=["http://192.168.0.5/api/usage"])
    step(f)
    assert f.targets() == ["http://192.168.0.5/api/usage"]
    assert store.load() == {}


def test_a_silent_clock_is_only_ever_retried_at_its_own_address(tmp_path):
    f, store, now = make(tmp_path, saved={"host": "10.97.40.207"})
    url = "http://10.97.40.207/api/usage"
    for _ in range(dsc.FAIL_LIMIT - 1):
        f.report(url, False)
    assert f.targets() == [url]
    f.report(url, False)
    assert f.targets() == []                      # stop pushing to a screen that is not there
    for _ in range(10):                           # a long outage: only that one address, now and then
        now.t += dsc.BAD_HOLD / 2
        step(f)
        assert f.targets() in ([], [url])
        if f.targets():
            drop(f, url)
    assert store.load()["host"] == "10.97.40.207"
    now.t += dsc.BAD_HOLD + 1
    step(f)
    assert f.targets() == [url]


def test_scanless_status_has_no_scan_fields(tmp_path):
    f, _store, _now = make(tmp_path, clocks=[A])
    d = f.describe()
    assert d["clock"] is None and d["failing"]
    assert not {"scanning", "lan_blocked", "needs_manual"} & set(d)
    step(f)
    f.report(A.usage_url, True)
    d = f.describe()
    assert d["clock"] == A and not d["failing"] and d["last_ok"]


def test_typed_address_wins_over_mdns_and_failures_and_restarts(tmp_path):
    f, store, _now = make(tmp_path, clocks=[A])
    assert f.set_manual("not a host:99999") is None
    c = f.set_manual(" 192.168.1.80 ")
    assert c is not None and c.source == "manual" and f.targets() == ["http://192.168.1.80/api/usage"]
    step(f)
    assert f.targets() == ["http://192.168.1.80/api/usage"]     # mDNS does not override it
    drop(f, "http://192.168.1.80/api/usage")
    drop(f, "http://192.168.1.80/api/usage")
    assert f.targets() == ["http://192.168.1.80/api/usage"]     # nor do failures
    f2 = ClockFinder(store, browser=FakeBrowser([A]))
    assert f2.targets() == ["http://192.168.1.80/api/usage"]    # a new app start keeps it


def test_retry_keeps_the_address_and_tries_it_at_once(tmp_path):
    f, store, now = make(tmp_path, saved={"name": "clawd", "host": "10.97.40.207", "port": 80})
    url = "http://10.97.40.207/api/usage"
    drop(f, url)
    step(f)
    assert f.targets() == []                      # on hold for BAD_HOLD
    f.retry()                                     # "Tìm lại"
    assert f.targets() == [url]                   # no wait, and nothing forgotten
    assert store.load()["host"] == "10.97.40.207" and store.load()["name"] == "clawd"
    now.t += 1
    step(f)
    assert f.targets() == [url]


def test_retry_unpins_a_typed_address_so_mdns_may_replace_it_later(tmp_path):
    f, store, _now = make(tmp_path, clocks=[A])
    f.set_manual("10.97.40.207")
    f.retry()
    assert store.load() == {"name": "", "host": "10.97.40.207", "port": 80, "version": ""}
    assert f.current is not None and f.current.source == "saved"
    step(f)
    assert f.targets() == ["http://10.97.40.207/api/usage"]     # still tried first
    drop(f, "http://10.97.40.207/api/usage")                     # but it is gone (another network)
    step(f)
    assert f.targets() == ["http://10.0.0.7/api/usage"]          # mDNS takes over
    assert store.load()["host"] == "10.0.0.7"


def test_retry_with_nothing_saved_only_listens_to_mdns(tmp_path):
    f, store, _now = make(tmp_path)
    f.retry()
    assert f.targets() == [] and store.load() == {}
    f.browser.set(A)
    step(f)
    assert f.targets() == ["http://10.0.0.7/api/usage"]


def test_old_scan_keys_are_dropped_from_clock_json(tmp_path):
    saved = {"name": "", "host": "10.97.40.207", "port": 80, "version": "2.16.0", "subnets": ["10.97.40.0/24"]}
    f, store, _now = make(tmp_path, saved=saved)
    f.report("http://10.97.40.207/api/usage", True)
    assert store.load() == {"name": "", "host": "10.97.40.207", "port": 80, "version": "2.16.0"}


def test_stored_clock_reads_clock_json():
    assert dsc.stored_clock({}) is None
    assert dsc.stored_clock({"manual_host": "10.0.0.5:8080", "host": "10.0.0.6"}).base == "http://10.0.0.5:8080"
    c = dsc.stored_clock({"host": "10.0.0.6", "port": "x", "name": "clawd"})
    assert (c.base, c.name, c.source) == ("http://10.0.0.6", "clawd", "saved")


def test_manual_clock_parsing():
    assert dsc.manual_clock("clawd.local:8080").base == "http://clawd.local:8080"
    assert dsc.manual_clock("http://10.0.0.5/whatever").usage_url == "http://10.0.0.5/api/usage"
    assert dsc.manual_clock("") is None and dsc.manual_clock(":80") is None
    assert dsc.manual_clock("h:abc") is None
