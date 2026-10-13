"""Install the bundled firmware on the clock, then (optionally) the settings that
were waiting for it, e.g. a style pack the old image cannot draw.

Order matters: a pack is sent only after the new image is up and says it draws
that style API (an old image has no /api/style at all).
"""
from __future__ import annotations

import time
from typing import Callable

from clawd_desktop import firmware, pause
from clawd_desktop.clockapi import ClockApi, ClockError

SETTLE = 6.0         # let a push the engine already started finish before the upload
LEASE = 60.0         # push pause, renewed while we wait
UPLOAD_LEASE = 200.0 # > clockapi.UPLOAD_TIMEOUT


def install(api: ClockApi, bundle: firmware.Bundle,
            progress: Callable[[str], None] = lambda _s: None,
            hold: Callable[[float, str], None] | None = None,
            release: Callable[[], None] | None = None,
            sleep: Callable[[float], None] | None = None,
            clock: Callable[[], float] = time.monotonic,
            settle: float | None = None) -> dict:
    """Push the bundle; returns the clock's /api/heap after it came back."""
    hold = hold or pause.hold
    release = release or pause.release
    sleep = sleep or time.sleep
    settle = SETTLE if settle is None else settle
    image = bundle.read()
    progress("check")
    heap = api.heap()
    state = firmware.update_state(heap, bundle)
    if state != "available":
        raise ClockError(f"no update to install ({state})")
    hold(LEASE, "firmware update")
    try:
        progress("pause")
        sleep(settle)
        hold(UPLOAD_LEASE, "firmware update")       # the upload blocks: cover all of it
        progress("upload")
        started = clock()
        api.upload_firmware(image)
        progress("reboot")
        new = api.wait_back(started, clock=clock, tick=lambda: hold(LEASE, "firmware update"))
    finally:
        release()
    if firmware.clock_seq(new) < bundle.seq:
        raise ClockError("the clock came back on its old firmware")
    progress("done")
    return new


CRC_WAIT = 150.0     # a clock checksums packs it did not receive itself in the background
PACK_TRIES = 2


def _wait_crcs(api: ClockApi, heap: dict, rel, sleep, clock) -> dict:
    """A heap in which no pack of `rel` is still being checksummed (or the last one after
    CRC_WAIT: those packs are then sent anyway)."""
    from clawd_desktop import feed
    end = clock() + CRC_WAIT
    while any(feed.pack_state(heap, k, p[1]) == "wait"
              for k in feed.PACK_KINDS if (p := rel.pack(k)) is not None) and clock() < end:
        sleep(3.0)
        try:
            heap = api.heap()
        except ClockError:
            pass
    return heap


def install_packs(api: ClockApi, rel, heap: dict,
                  progress: Callable[[str], None] = lambda _s: None,
                  hold: Callable[[float, str], None] | None = None,
                  release: Callable[[], None] | None = None,
                  sleep: Callable[[float], None] | None = None,
                  clock: Callable[[], float] = time.monotonic) -> dict:
    """Send the release's clawd.pak, then desk.pak, where the clock's differs (CRC in
    /api/heap). One retry each. Returns the clock's heap afterwards."""
    from clawd_desktop import feed
    hold = hold or pause.hold
    release = release or pause.release
    sleep = sleep or time.sleep
    heap = _wait_crcs(api, heap, rel, sleep, clock)
    todo = [(k, p) for k in feed.PACK_KINDS if (p := rel.pack(k)) is not None
            and feed.pack_state(heap, k, p[1]) != "ok"]
    if not todo:
        return heap
    try:
        for kind, (data, crc) in todo:
            progress(kind)
            for attempt in range(PACK_TRIES):
                hold(UPLOAD_LEASE * 3, "pack update")
                try:
                    r = api.upload_pack(kind, data)
                except ClockError:
                    if attempt + 1 == PACK_TRIES:
                        raise
                    sleep(5.0)
                    continue
                got = r.get("crc")
                if isinstance(got, str) and got and got != crc:
                    if attempt + 1 == PACK_TRIES:
                        raise ClockError(f"{kind} arrived damaged (CRC {got}, want {crc})")
                    continue
                break
    finally:
        release()
    try:
        return api.heap()
    except ClockError:
        return heap


def install_release(api: ClockApi, rel, changes: dict, need_style_api: int = 0, **kw) -> dict:
    """Everything a feed release brings that this clock lacks: the firmware (when newer),
    then the packs that differ, then POST `changes`. Returns the clock's heap."""
    progress = kw.get("progress") or (lambda _s: None)
    progress("check")
    heap = api.heap()
    if firmware.update_state(heap, rel) == "available":
        heap = install(api, rel, **kw)
    pk = {k: kw[k] for k in ("progress", "hold", "release", "sleep", "clock") if k in kw}
    heap = install_packs(api, rel, heap, **pk)
    if need_style_api and firmware.clock_style_api(heap) != need_style_api:
        raise ClockError("the new firmware does not draw these style packs")
    if changes:
        api.save(changes)
    progress("done")
    return heap


def install_then_save(api: ClockApi, bundle: firmware.Bundle, changes: dict,
                      need_style_api: int = 0, **kw) -> dict:
    """`install`, then POST `changes`; ClockError when the new image does not draw style
    packs of API `need_style_api` (0: no need)."""
    heap = install(api, bundle, **kw)
    if need_style_api and firmware.clock_style_api(heap) != need_style_api:
        raise ClockError("the new firmware does not draw these style packs")
    if changes:
        api.save(changes)
    return heap
