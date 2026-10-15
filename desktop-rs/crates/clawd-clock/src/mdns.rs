//! mDNS browse of `_clawd._tcp.local.` (`discovery.MdnsBrowser`), on `mdns-sd` in its own single
//! daemon thread (stoppable). The finder talks to the [`MdnsSource`] trait so tests use a fake.
//! Windows can fail to receive responses (mdns-sd issue #33): the manual address and the
//! remembered clock keep working.

use clawd_contracts::ClockTarget;

/// `discovery.RESOLVE_TIMEOUT_MS`.
pub const RESOLVE_TIMEOUT_MS: u64 = 4000;

/// A live set of `_clawd._tcp` instance names that can be resolved on demand.
pub trait MdnsSource: Send + Sync {
    /// `MdnsBrowser.start()`: false when mDNS cannot be started (discovery then relies on the
    /// manual and remembered addresses).
    fn start(&self) -> bool;
    /// `MdnsBrowser.names()`: sorted instance names currently seen.
    fn names(&self) -> Vec<String>;
    /// `MdnsBrowser.changed`: bumps on every add/update/remove.
    fn changed(&self) -> u64;
    /// `MdnsBrowser.resolve(name)`: the address and TXT (`id`, `ver`) of one instance. Blocking:
    /// call it from `spawn_blocking`, like Python's `asyncio.to_thread`.
    fn resolve(&self, name: &str) -> Option<ClockTarget>;
    /// `MdnsBrowser.stop()` (`close_mdns`): shut the daemon thread down; idempotent.
    fn stop(&self);
}

/// `discovery.MdnsBrowser` on `mdns-sd`.
pub struct MdnsBrowser {
    _private: (),
}

impl MdnsBrowser {
    pub fn new() -> Self {
        todo!("phase 08")
    }
}

impl MdnsSource for MdnsBrowser {
    fn start(&self) -> bool {
        todo!("phase 08")
    }
    fn names(&self) -> Vec<String> {
        todo!("phase 08")
    }
    fn changed(&self) -> u64 {
        todo!("phase 08")
    }
    fn resolve(&self, name: &str) -> Option<ClockTarget> {
        todo!("phase 08")
    }
    fn stop(&self) {
        todo!("phase 08")
    }
}
