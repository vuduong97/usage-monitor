//! Small runtime vocabulary shared by the async crates (`asyncio.Event` stop, injected clocks,
//! `Callable[[], list[str]]` target lists).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::watch;

/// A boxed `Send` future (callbacks that return futures, e.g. `BootWatch.on_restart`).
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// `Callable[[], list[str]]`: the clocks' `/api/usage` URLs to push to right now
/// (`ClockFinder.targets`, `Engine.targets`, `http_targets`).
pub type TargetsFn = Arc<dyn Fn() -> Vec<String> + Send + Sync>;

/// Receiving end of the stop signal that replaces Python's `asyncio.Event` `stop`.
/// The value turns `true` once, when the loop must end (`rx.wait_for(|s| *s)`).
pub type StopRx = watch::Receiver<bool>;

/// A stop signal: send `true` on the sender to stop every loop holding a clone of the receiver.
pub fn new_stop() -> (watch::Sender<bool>, StopRx) {
    watch::channel(false)
}

/// Injected time source, in seconds as `f64` like Python's `time.time()` / `time.monotonic()`.
/// The testkit's `FakeClock` implements it so golden tests can drive time by hand.
pub trait Clock: Send + Sync {
    fn now(&self) -> f64;
}

impl<T: Clock + ?Sized> Clock for Arc<T> {
    fn now(&self) -> f64 {
        (**self).now()
    }
}

/// Wall clock: seconds since the Unix epoch (`time.time()`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
}

/// Monotonic clock: seconds since this value was created (`time.monotonic()`; only
/// differences mean anything).
#[derive(Debug, Clone, Copy)]
pub struct MonotonicClock {
    start: Instant,
}

impl MonotonicClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_signal_reaches_every_receiver() {
        let (tx, rx) = new_stop();
        let rx2 = rx.clone();
        assert!(!*rx.borrow());
        tx.send(true).unwrap();
        assert!(*rx2.borrow());
    }

    #[test]
    fn clocks_move_forward() {
        assert!(SystemClock.now() > 1_700_000_000.0);
        let m = MonotonicClock::new();
        assert!(m.now() >= 0.0);
    }
}
