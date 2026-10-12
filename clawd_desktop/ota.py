"""Install the bundled firmware on the clock, then (optionally) a setting that
needs it, e.g. a usage layout the old image cannot draw.

Order matters: the layout is saved only after the new image is up and says it
draws that layout (an old image would store an unknown value and draw classic).
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
            hold: Callable[[float, str], None] = pause.hold,
            release: Callable[[], None] = pause.release,
            sleep: Callable[[float], None] = time.sleep,
            clock: Callable[[], float] = time.monotonic,
            settle: float = SETTLE) -> dict:
    """Push the bundle; returns the clock's /api/heap after it came back."""
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
                      need_layout: int | None = None, **kw) -> dict:
    """`install`, then POST `changes` once the new image draws `need_layout`."""
    heap = install(api, bundle, **kw)
    if need_layout is not None and not firmware.clock_layouts(heap) >> need_layout & 1:
        raise ClockError("the new firmware does not draw that layout")
    if changes:
        api.save(changes)
    return heap
