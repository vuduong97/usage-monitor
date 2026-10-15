/// `events.read_hid_idle()`: seconds since the last keyboard/mouse input, `None` if unknown.
/// A locked screen gets no input either, so this also covers "locked". Windows:
/// `GetLastInputInfo` (+ `GetTickCount`); macOS: `ioreg -c IOHIDSystem` `HIDIdleTime`;
/// elsewhere `None`.
pub fn read_hid_idle() -> Option<f64> {
    todo!("phase 04")
}
