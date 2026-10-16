//! `events.StatePusher` (`events.py:893-1035`): pushes `Brain.desired()` to every clock: at once
//! on a change (after a short coalesce), then every `RESEND_EVERY` seconds while something is up
//! so the clock's TTL never lapses, and one final clear when it all goes quiet.
//!
//! Stops while a pause lease holds (the *engine* decides: it passes a `targets` function that
//! returns nothing then, see `clawd_store::pause::PauseReader`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clawd_contracts::{Clock, DesiredState, StopRx, SystemClock, TargetsFn, WireAnim, WireState};
use hyper::Method;
use tokio::task::JoinHandle;

use crate::brain::SharedBrain;
use crate::httpc;
use crate::idle::read_hid_idle;
use crate::pet::DESK_NEW_MIN;

/// `events.STATE_PATH`, `events.HEAP_PATH`.
const STATE_PATH: &str = "/api/state";
const HEAP_PATH: &str = "/api/heap";
/// `events.STATE_TTL`: seconds the clock keeps a state without a resend.
pub const STATE_TTL: u32 = 60;
/// `events.RESEND_EVERY`: resend an unchanged live state this often.
pub const RESEND_EVERY: f64 = 20.0;
/// `events.PUSH_TIMEOUT`.
pub const PUSH_TIMEOUT: Duration = Duration::from_millis(1500);
/// `events.COALESCE`: gather an event burst before pushing.
pub const COALESCE: Duration = Duration::from_millis(300);
/// `events.DESK_PROBE_EVERY`: re-read the clocks' animation count this often (seconds).
pub const DESK_PROBE_EVERY: f64 = 300.0;
/// `events.DESK_PROBE_TIMEOUT`: the clock answers HTTP slowly for a while after boot.
pub const DESK_PROBE_TIMEOUT: Duration = Duration::from_secs(12);
/// The HID idle probe runs this often (seconds).
const IDLE_CHECK_EVERY: f64 = 10.0;

/// `urlunsplit((scheme, netloc, path, "", ""))` of a push URL: scheme and host of `target`
/// with `path` (what `urllib.parse` gives for a URL with a scheme; none gives just the path).
fn rebuild_url(target: &str, path: &str) -> String {
    let scheme_len = target.find("://").filter(|i| {
        *i > 0
            && target[..*i]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    let Some(i) = scheme_len else {
        return path.to_owned();
    };
    let scheme = target[..i].to_ascii_lowercase();
    let rest = &target[i + 3..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    format!("{scheme}://{}{path}", &rest[..end])
}

/// `events.state_url(target)`: an `http_targets` push URL (`.../api/usage`) -> that clock's
/// `/api/state` URL.
pub fn state_url(target: &str) -> String {
    rebuild_url(target, STATE_PATH)
}

/// `events.heap_url(target)`: -> that clock's `/api/heap` URL.
pub fn heap_url(target: &str) -> String {
    rebuild_url(target, HEAP_PATH)
}

/// `events.wire(d)`: the `POST /api/state` body for a desired state (byte-exact JSON key order).
pub fn wire(d: &DesiredState) -> WireState {
    WireState {
        a: if d.seq.len() > 1 {
            WireAnim::Many(d.seq.clone())
        } else {
            WireAnim::One(d.a.clone())
        },
        ttl: STATE_TTL,
        rep: d.rep.then_some(true),
        v: (!d.v.is_empty()).then(|| d.v.clone()),
        u: d.u.then_some(true),
        txt: (!d.txt.is_empty()).then(|| d.txt.clone()),
        cd: (d.cd != 0).then_some(d.cd),
        dim: (d.dim >= 0).then_some(d.dim),
    }
}

/// Seconds since the last keyboard/mouse input; `None` if unknown.
pub type IdleFn = Arc<dyn Fn() -> Option<f64> + Send + Sync>;

/// `StatePusher.key(d)`: what makes two desired states "the same" for the push decision.
type StateKey = (String, Vec<String>, bool, Vec<String>, String, i64, i64);

fn key_of(d: &DesiredState) -> StateKey {
    (
        d.a.clone(),
        d.seq.clone(),
        d.rep,
        d.v.clone(),
        d.txt.clone(),
        d.dim,
        d.focus_until,
    )
}

struct PushState {
    last_key: Option<StateKey>,
    last_sent: f64,
    /// Per push URL: did the last delivery succeed (logs only on a change).
    ok: HashMap<String, bool>,
    last_idle_check: f64,
    last_desk_probe: f64,
    desk_task: Option<JoinHandle<()>>,
}

/// `events.StatePusher`. Shared (`Arc`) between the engine, which calls
/// [`Self::invalidate`] when a clock restarts, and its own [`Self::run`] loop.
pub struct StatePusher {
    brain: SharedBrain,
    targets: TargetsFn,
    clock: Arc<dyn Clock>,
    idle: IdleFn,
    state: Mutex<PushState>,
}

impl StatePusher {
    /// `StatePusher(brain, targets)`: system clock and [`crate::read_hid_idle`].
    pub fn new(brain: SharedBrain, targets: TargetsFn) -> Self {
        Self::with_clock(
            brain,
            targets,
            Arc::new(SystemClock),
            Arc::new(read_hid_idle),
        )
    }

    /// Same with an injected clock and idle probe (tests).
    pub fn with_clock(
        brain: SharedBrain,
        targets: TargetsFn,
        clock: Arc<dyn Clock>,
        idle: IdleFn,
    ) -> Self {
        Self {
            brain,
            targets,
            clock,
            idle,
            state: Mutex::new(PushState {
                last_key: None,
                last_sent: 0.0,
                ok: HashMap::new(),
                last_idle_check: -1e9,
                last_desk_probe: -1e9,
                desk_task: None,
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, PushState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `pusher.last_key = None`: forget what was last sent so the next step resends the state
    /// (a clock that restarted lost it). The engine calls this from `BootWatch.on_restart`.
    pub fn invalidate(&self) {
        self.state().last_key = None;
    }

    /// `StatePusher.probe_desk()`: read every clock's animation count (`desk` in `/api/heap`);
    /// the newer idle picks play only when all clocks that answer have them.
    pub async fn probe_desk(&self) {
        probe_desk(&self.brain, &self.targets).await;
    }

    fn maybe_probe_desk(&self, now: f64) {
        let mut st = self.state();
        if now - st.last_desk_probe < DESK_PROBE_EVERY {
            return;
        }
        if st.desk_task.as_ref().is_some_and(|t| !t.is_finished()) {
            return;
        }
        st.last_desk_probe = now;
        let (brain, targets) = (Arc::clone(&self.brain), Arc::clone(&self.targets));
        st.desk_task = Some(tokio::spawn(async move {
            probe_desk(&brain, &targets).await;
        }));
    }

    /// How long the loop may sleep: at most 1 s, less when an idle pick ends sooner (the clock
    /// drops it the moment the clear arrives).
    pub(crate) fn wait_for(&self, now: f64) -> f64 {
        let ends = self
            .brain
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pet
            .pick_ends();
        match ends {
            None => 1.0,
            Some(ends) => (ends - now).clamp(0.05, 1.0),
        }
    }

    /// `StatePusher.step()`: one decision cycle; true when something was sent.
    pub async fn step(&self) -> bool {
        let now = self.clock.now();
        let check_idle = {
            let mut st = self.state();
            let due = now - st.last_idle_check >= IDLE_CHECK_EVERY;
            if due {
                st.last_idle_check = now;
            }
            due
        };
        if check_idle {
            let probe = Arc::clone(&self.idle);
            let idle = tokio::task::spawn_blocking(move || probe())
                .await
                .unwrap_or(None);
            let mut brain = self.brain.lock().unwrap_or_else(|e| e.into_inner());
            let usage = brain.usage.clone();
            brain.pet.observe(now, idle, usage.as_ref());
        }
        let desired = self
            .brain
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .desired(now);
        let key = key_of(&desired);
        let live = !desired.a.is_empty() || desired.dim >= 0;
        let changed = {
            let st = self.state();
            let same = st.last_key.as_ref() == Some(&key);
            if same && !(live && now - st.last_sent >= RESEND_EVERY) {
                return false;
            }
            !same
        };
        let urls: Vec<String> = (self.targets)().iter().map(|t| state_url(t)).collect();
        if urls.is_empty() {
            return false;
        }
        if changed {
            let what = if desired.a.is_empty() {
                "(usage)"
            } else {
                &desired.a
            };
            if desired.txt.is_empty() {
                tracing::info!("Clock state: {what}");
            } else {
                tracing::info!("Clock state: {what} - {}", desired.txt);
            }
        }
        if self.send(&urls, &wire(&desired)).await {
            let mut st = self.state();
            st.last_key = Some(key);
            st.last_sent = now;
            return true;
        }
        false
    }

    /// POST the state to every URL at once; true when any took it.
    async fn send(&self, urls: &[String], payload: &WireState) -> bool {
        let body = crate::pyjson::to_py_json_compact(payload);
        let posts: Vec<_> = urls
            .iter()
            .map(|url| {
                let (url, body) = (url.clone(), body.clone());
                tokio::spawn(async move {
                    let result = httpc::request(Method::POST, &url, Some(body), PUSH_TIMEOUT).await;
                    (url, result)
                })
            })
            .collect();
        let mut any_ok = false;
        for post in posts {
            let Ok((url, result)) = post.await else {
                continue;
            };
            let (ok, why) = match result {
                Ok(reply) => (reply.status == 200, format!("HTTP {}", reply.status)),
                Err(why) => (false, why),
            };
            let previous = self.state().ok.insert(url.clone(), ok);
            if previous != Some(ok) {
                if ok {
                    tracing::info!("State push to {url}: ok");
                } else {
                    tracing::warn!("State push to {url} failed: {why}");
                }
            }
            any_ok |= ok;
        }
        any_ok
    }

    /// `StatePusher.run(stop)`: loop until `stop`; a failing cycle is logged and the loop goes on.
    pub async fn run(&self, mut stop: StopRx) {
        let wake = self
            .brain
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .wake
            .clone();
        while !*stop.borrow() {
            self.maybe_probe_desk(self.clock.now());
            self.step().await;
            let wait = Duration::from_secs_f64(self.wait_for(self.clock.now()));
            let woken = tokio::select! {
                _ = crate::stop_requested(&mut stop) => break,
                _ = wake.notified() => true,
                _ = tokio::time::sleep(wait) => false,
            };
            if woken {
                tokio::select! {
                    _ = crate::stop_requested(&mut stop) => break,
                    _ = tokio::time::sleep(COALESCE) => {}
                }
            }
        }
        if let Some(task) = self.state().desk_task.take() {
            task.abort();
        }
    }
}

/// `StatePusher.probe_desk`: no answer keeps what was known.
async fn probe_desk(brain: &SharedBrain, targets: &TargetsFn) {
    let mut counts: Vec<i64> = Vec::new();
    for target in targets() {
        let reply = httpc::request(Method::GET, &heap_url(&target), None, DESK_PROBE_TIMEOUT).await;
        let count = reply
            .ok()
            .filter(|r| r.status == 200)
            .and_then(|r| serde_json::from_slice::<serde_json::Value>(&r.body).ok())
            .and_then(|v| v.get("desk").and_then(serde_json::Value::as_i64));
        counts.extend(count);
    }
    let Some(least) = counts.iter().copied().min() else {
        return;
    };
    let new = least >= DESK_NEW_MIN;
    let mut brain = brain.lock().unwrap_or_else(|e| e.into_inner());
    if new != brain.pet.desk_new {
        tracing::info!(
            "Clock desk.pak has {least} animations: {}",
            if new {
                "all idle picks on"
            } else {
                "newer idle picks off"
            }
        );
    }
    brain.pet.desk_new = new;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::Brain;
    use crate::pet::Pet;
    use clawd_contracts::PetConfig;

    #[test]
    fn state_and_heap_urls_keep_only_scheme_and_host() {
        assert_eq!(
            state_url("http://10.0.0.5/api/usage"),
            "http://10.0.0.5/api/state"
        );
        assert_eq!(
            state_url("http://clock.local:8080/api/usage?x=1"),
            "http://clock.local:8080/api/state"
        );
        assert_eq!(
            heap_url("http://10.0.0.5/api/usage"),
            "http://10.0.0.5/api/heap"
        );
        assert_eq!(state_url("HTTP://Host/api/usage"), "http://Host/api/state");
        assert_eq!(state_url("no-scheme/api/usage"), "/api/state");
    }

    #[test]
    fn wait_for_follows_the_idle_pick_end() {
        let brain = Brain::new(Pet::new(PetConfig::default(), None), Box::new(|| false)).shared();
        let pusher = StatePusher::with_clock(
            Arc::clone(&brain),
            Arc::new(Vec::new),
            Arc::new(SystemClock),
            Arc::new(|| None),
        );
        let t0 = 1_760_000_000.0;
        assert_eq!(pusher.wait_for(t0), 1.0);
        {
            let mut b = brain.lock().unwrap();
            b.pet.desk_new = true;
            b.pet.next_pick = t0;
            assert!(!b.desired(t0).a.is_empty());
        }
        let ends = brain.lock().unwrap().pet.pick_ends().unwrap();
        assert!((pusher.wait_for(ends - 0.3) - 0.3).abs() < 1e-6);
        assert_eq!(pusher.wait_for(ends + 1.0), 0.05);
    }
}
