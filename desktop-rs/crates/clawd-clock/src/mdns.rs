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

/// How long `stop` waits for the daemon to confirm its shutdown.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

/// `ServiceDaemon::shutdown`, retried once when its command queue is full (`Error::Again`).
fn shutdown_daemon(daemon: &ServiceDaemon) -> Option<mdns_sd::Receiver<mdns_sd::DaemonStatus>> {
    match daemon.shutdown() {
        Ok(status) => Some(status),
        Err(mdns_sd::Error::Again) => {
            std::thread::sleep(Duration::from_millis(50));
            daemon.shutdown().ok()
        }
        Err(_) => None, // DaemonShutdown: already gone
    }
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

    /// The resolved record of `name`, waiting up to `timeout` for the daemon to deliver it
    /// (`ServiceBrowser` resolves on its own as soon as a service is found).
    fn wait_resolved(&self, name: &str, timeout: Duration) -> Option<ClockTarget> {
        let deadline = Instant::now() + timeout;
        let mut seen = self.seen();
        loop {
            if let Some(c) = seen.resolved.get(name) {
                return Some(c.clone());
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            seen = self
                .resolved
                .wait_timeout(seen, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Apply one daemon event to the name set (`Listener.add_service/update_service/
    /// remove_service`).
    fn apply(&self, event: ServiceEvent) {
        match event {
            ServiceEvent::ServiceFound(_, name) => {
                self.seen().names.insert(name);
                self.changed.fetch_add(1, Ordering::SeqCst);
            }
            ServiceEvent::ServiceResolved(info) => {
                let name = info.get_fullname().to_owned();
                let clock = clock_of(&info);
                let mut seen = self.seen();
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
                self.changed.fetch_add(1, Ordering::SeqCst);
                self.resolved.notify_all();
            }
            ServiceEvent::ServiceRemoved(_, name) => {
                let mut seen = self.seen();
                seen.names.remove(&name);
                seen.resolved.remove(&name);
                drop(seen);
                self.changed.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
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

impl Shared {
    fn new() -> Self {
        Self {
            seen: Mutex::new(Seen::default()),
            resolved: Condvar::new(),
            changed: AtomicU64::new(0),
        }
    }
}

impl MdnsBrowser {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::new()),
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
    clock.port = if info.get_port() == 0 {
        80
    } else {
        info.get_port()
    };
    clock.name = id.to_owned();
    clock.version = info.get_property_val_str("ver").unwrap_or("").to_owned();
    Some(clock)
}

fn consume(shared: &Shared, receiver: mdns_sd::Receiver<ServiceEvent>) {
    while let Ok(event) = receiver.recv() {
        shared.apply(event);
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
        self.running().as_ref()?;
        self.shared
            .wait_resolved(name, Duration::from_millis(RESOLVE_TIMEOUT_MS))
    }

    fn stop(&self) {
        let Some(running) = self.running().take() else {
            return;
        };
        // Join our event thread only once the daemon confirmed its shutdown (it then drops the
        // browse channel and the thread's `recv` ends). Otherwise (command queue full twice, a
        // daemon that is already gone or does not answer) detach the thread: it ends by itself
        // when the daemon does, and `stop` must never block for ever.
        let confirmed = match shutdown_daemon(&running.daemon) {
            Some(status) => status.recv_timeout(SHUTDOWN_WAIT).is_ok(),
            None => false,
        };
        if confirmed {
            let _ = running.events.join();
        } else {
            tracing::info!("mDNS daemon did not confirm its shutdown; detaching its event thread");
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use mdns_sd::ServiceInfo;
    use std::collections::HashMap;

    fn resolved(label: &str, ip: &str, port: u16, txt: &[(&str, &str)]) -> ServiceEvent {
        let props: HashMap<String, String> = txt
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        let info = ServiceInfo::new(MDNS_SERVICE, label, "clawd.local.", ip, port, props).unwrap();
        ServiceEvent::ServiceResolved(Box::new(info.as_resolved_service()))
    }

    #[test]
    fn a_resolved_service_becomes_a_clock_like_python_resolve() {
        let shared = Shared::new();
        shared.apply(resolved(
            "clawd",
            "10.0.0.7",
            8080,
            &[("id", "desk-1"), ("ver", "2.16.0")],
        ));
        let c = shared
            .wait_resolved("clawd._clawd._tcp.local.", Duration::ZERO)
            .unwrap();
        assert_eq!(
            (c.host.as_str(), c.port, c.name.as_str()),
            ("10.0.0.7", 8080, "desk-1")
        );
        assert_eq!(
            (c.version.as_str(), c.source),
            ("2.16.0", ClockSource::Mdns)
        );
        assert_eq!(shared.changed.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn without_txt_the_instance_label_is_the_name_and_port_zero_is_80() {
        let shared = Shared::new();
        shared.apply(resolved("clawd-9", "10.0.0.8", 0, &[]));
        let c = shared
            .wait_resolved("clawd-9._clawd._tcp.local.", Duration::ZERO)
            .unwrap();
        assert_eq!(
            (c.name.as_str(), c.port, c.version.as_str()),
            ("clawd-9", 80, "")
        );
    }

    #[test]
    fn a_service_without_an_ipv4_address_is_not_resolvable() {
        let shared = Shared::new();
        shared.apply(resolved("v6", "fe80::1", 80, &[]));
        assert!(shared
            .wait_resolved("v6._clawd._tcp.local.", Duration::ZERO)
            .is_none());
        assert_eq!(shared.seen().names.len(), 1, "the name is still seen");
    }

    #[test]
    fn found_and_removed_services_bump_changed_and_keep_names_sorted() {
        let shared = Shared::new();
        let found = |n: &str| ServiceEvent::ServiceFound(MDNS_SERVICE.into(), n.into());
        shared.apply(found("b._clawd._tcp.local."));
        shared.apply(found("a._clawd._tcp.local."));
        let names: Vec<String> = shared.seen().names.iter().cloned().collect();
        assert_eq!(names, ["a._clawd._tcp.local.", "b._clawd._tcp.local."]);
        shared.apply(ServiceEvent::ServiceRemoved(
            MDNS_SERVICE.into(),
            "a._clawd._tcp.local.".into(),
        ));
        assert_eq!(shared.seen().names.len(), 1);
        assert_eq!(shared.changed.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn resolve_waits_for_the_daemon_and_gives_up_after_the_timeout() {
        let shared = Arc::new(Shared::new());
        assert!(shared
            .wait_resolved("x._clawd._tcp.local.", Duration::from_millis(30))
            .is_none());
        let late = Arc::clone(&shared);
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            late.apply(resolved("x", "10.0.0.5", 80, &[]));
        });
        let c = shared.wait_resolved("x._clawd._tcp.local.", Duration::from_secs(5));
        assert_eq!(c.unwrap().host, "10.0.0.5");
        t.join().unwrap();
    }

    #[test]
    fn an_idle_browser_resolves_nothing_and_stop_is_idempotent() {
        let browser = MdnsBrowser::new();
        assert!(browser.names().is_empty() && browser.changed() == 0);
        assert!(browser.resolve("x._clawd._tcp.local.").is_none());
        browser.stop();
        browser.stop();
    }
}
