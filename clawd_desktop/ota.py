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
