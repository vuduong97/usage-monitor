//! mDNS browse of `_clawd._tcp.local.` (`discovery.MdnsBrowser`), on `mdns-sd` in its own single
//! daemon thread (stoppable). The finder talks to the [`MdnsSource`] trait so tests use a fake.
//! Windows can fail to receive responses (mdns-sd issue #33): the manual address and the
//! remembered clock keep working.
//!
//! Threads: the `mdns-sd` daemon (one) plus one small thread of ours that turns its events into
//! the name set; [`MdnsSource::stop`] shuts the daemon down and joins ours. Only IPv4 multicast
//! (UDP 5353) is used; nothing else is bound.

use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use clawd_contracts::{ClockSource, ClockTarget, MDNS_SERVICE};
use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent};

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

/// What the event thread has learned (`MdnsBrowser._names` plus the resolved records).
#[derive(Default)]
struct Seen {
    names: BTreeSet<String>,
    resolved: BTreeMap<String, ClockTarget>,
}

struct Shared {
    seen: Mutex<Seen>,
    /// Signalled when a service gets resolved (wakes `resolve`).
    resolved: Condvar,
    changed: AtomicU64,
}

impl Shared {
    fn seen(&self) -> MutexGuard<'_, Seen> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The running daemon and the thread that reads its events.
struct Running {
    daemon: ServiceDaemon,
    events: JoinHandle<()>,
}

/// `discovery.MdnsBrowser` on `mdns-sd`.
pub struct MdnsBrowser {
    shared: Arc<Shared>,
    running: Mutex<Option<Running>>,
}

impl MdnsBrowser {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                seen: Mutex::new(Seen::default()),
                resolved: Condvar::new(),
                changed: AtomicU64::new(0),
            }),
            running: Mutex::new(None),
        }
    }

    fn running(&self) -> MutexGuard<'_, Option<Running>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for MdnsBrowser {
    fn default() -> Self {
        Self::new()
    }
}

/// `MdnsBrowser.resolve`'s record: the first IPv4 address, TXT `id` (else the instance label)
/// and `ver`.
fn clock_of(info: &mdns_sd::ResolvedService) -> Option<ClockTarget> {
    let host: Ipv4Addr = info.get_addresses_v4().into_iter().min()?;
    let label = info.get_fullname().split('.').next().unwrap_or("");
    let id = info
        .get_property_val_str("id")
        .filter(|v| !v.is_empty())
        .unwrap_or(label);
    let mut clock = ClockTarget::new(host.to_string(), ClockSource::Mdns);
    clock.port = if info.get_port() == 0 { 80 } else { info.get_port() };
    clock.name = id.to_owned();
    clock.version = info.get_property_val_str("ver").unwrap_or("").to_owned();
    Some(clock)
}

fn consume(shared: &Shared, receiver: mdns_sd::Receiver<ServiceEvent>) {
    while let Ok(event) = receiver.recv() {
        match event {
            ServiceEvent::ServiceFound(_, name) => {
                shared.seen().names.insert(name);
                shared.changed.fetch_add(1, Ordering::SeqCst);
            }
            ServiceEvent::ServiceResolved(info) => {
                let name = info.get_fullname().to_owned();
                let clock = clock_of(&info);
                let mut seen = shared.seen();
                seen.names.insert(name.clone());
                match clock {
                    Some(c) => {
                        seen.resolved.insert(name, c);
                    }
                    None => {
                        seen.resolved.remove(&name);
                    }
                }
                drop(seen);
                shared.changed.fetch_add(1, Ordering::SeqCst);
                shared.resolved.notify_all();
            }
            ServiceEvent::ServiceRemoved(_, name) => {
                let mut seen = shared.seen();
                seen.names.remove(&name);
                seen.resolved.remove(&name);
                drop(seen);
                shared.changed.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

impl MdnsSource for MdnsBrowser {
    fn start(&self) -> bool {
        let mut running = self.running();
        if running.is_some() {
            return true;
        }
        let started = (|| {
            let daemon = ServiceDaemon::new()?;
            daemon.disable_interface(IfKind::IPv6)?;
            let receiver = daemon.browse(MDNS_SERVICE)?;
            Ok::<_, mdns_sd::Error>((daemon, receiver))
        })();
        match started {
            Ok((daemon, receiver)) => {
                let shared = Arc::clone(&self.shared);
                let events = std::thread::Builder::new()
                    .name("clawd-mdns-events".into())
                    .spawn(move || consume(&shared, receiver));
                match events {
                    Ok(events) => {
                        *running = Some(Running { daemon, events });
                        true
                    }
                    Err(e) => {
                        tracing::info!("mDNS discovery unavailable: {e}");
                        let _ = daemon.shutdown();
                        false
                    }
                }
            }
            Err(e) => {
                // no multicast-capable interface, port in use...
                tracing::info!("mDNS discovery unavailable: {e}");
                false
            }
        }
    }

    fn names(&self) -> Vec<String> {
        self.shared.seen().names.iter().cloned().collect()
    }

    fn changed(&self) -> u64 {
        self.shared.changed.load(Ordering::SeqCst)
    }

    fn resolve(&self, name: &str) -> Option<ClockTarget> {
        if self.running().is_none() {
            return None;
        }
        let deadline = Instant::now() + Duration::from_millis(RESOLVE_TIMEOUT_MS);
        let mut seen = self.shared.seen();
        loop {
            if let Some(c) = seen.resolved.get(name) {
                return Some(c.clone());
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            let (guard, timeout) = self
                .shared
                .resolved
                .wait_timeout(seen, left)
                .unwrap_or_else(|e| e.into_inner());
            seen = guard;
            if timeout.timed_out() && !seen.resolved.contains_key(name) {
                return None;
            }
        }
    }

    fn stop(&self) {
        let Some(running) = self.running().take() else {
            return;
        };
        // the daemon may already be half closed: ignore its answer
        let _ = running.daemon.shutdown();
        let _ = running.events.join();
        let mut seen = self.shared.seen();
        seen.names.clear();
        seen.resolved.clear();
    }
}

impl Drop for MdnsBrowser {
    fn drop(&mut self) {
        self.stop();
    }
}
