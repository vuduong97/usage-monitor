//! The integration files the app copies into an agent, embedded in the binary so this crate does
//! not depend on any directory tree outside itself (`paths.integration(rel)` in Python, which
//! read `_MEIPASS/clawd_desktop/integrations` when frozen). Phase 05 owns the files.

/// `integrations/pi/clawd-home.ts`.
pub const PI_CLAWD_HOME_TS: &[u8] = include_bytes!("../assets/pi/clawd-home.ts");
/// `integrations/opencode/clawd.js`.
pub const OPENCODE_CLAWD_JS: &[u8] = include_bytes!("../assets/opencode/clawd.js");

/// `paths.integration(rel)`: the bytes for `pi/clawd-home.ts` or `opencode/clawd.js`; `None`
/// for any other name.
pub fn integration(rel: &str) -> Option<&'static [u8]> {
    todo!("phase 05")
}
