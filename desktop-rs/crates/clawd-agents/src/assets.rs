//! The integration files the app copies into an agent, embedded in the binary so this crate does
//! not depend on any directory tree outside itself (`paths.integration(rel)` in Python, which
//! read `_MEIPASS/clawd_desktop/integrations` (branch python-0.1.x) when frozen). Phase 05 owns the files.

/// `integrations/pi/clawd-home.ts`.
pub const PI_CLAWD_HOME_TS: &[u8] = include_bytes!("../assets/pi/clawd-home.ts");
/// `integrations/opencode/clawd.js`.
pub const OPENCODE_CLAWD_JS: &[u8] = include_bytes!("../assets/opencode/clawd.js");

/// `paths.integration(rel)`: the bytes for `pi/clawd-home.ts` or `opencode/clawd.js`; `None`
/// for any other name.
pub fn integration(rel: &str) -> Option<&'static [u8]> {
    match rel {
        "pi/clawd-home.ts" => Some(PI_CLAWD_HOME_TS),
        "opencode/clawd.js" => Some(OPENCODE_CLAWD_JS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_names_resolve_and_carry_their_marks() {
        let pi = std::str::from_utf8(integration("pi/clawd-home.ts").unwrap()).unwrap();
        assert!(pi.contains(crate::pi::MARK));
        let oc = std::str::from_utf8(integration("opencode/clawd.js").unwrap()).unwrap();
        assert!(oc.contains(crate::opencode::MARK));
        assert!(integration("cli/clawd").is_none());
    }
}
