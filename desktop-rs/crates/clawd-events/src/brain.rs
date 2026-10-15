//! `events.Brain` (`events.py:836-892`): owns every layer and decides the one state the clock
//! should show.

use std::sync::{Arc, Mutex};

use clawd_contracts::{DesiredState, UsagePayload};
use tokio::sync::Notify;

use crate::pet::Pet;
use crate::pokes::Pokes;
use crate::tracker::Tracker;

/// The Brain is shared by the event server (ingests), the state pusher (reads `desired`) and the
/// engine (publishes `usage`): one lock, held briefly, never across an `.await`. To wake the
/// pusher without holding the lock, clone [`Brain::wake`] out first.
pub type SharedBrain = Arc<Mutex<Brain>>;

/// `events.Brain`.
pub struct Brain {
    pub pet: Pet,
    pub tracker: Tracker,
    pub pokes: Pokes,
    /// `Brain.wake` (`asyncio.Event` in Python): notified when an event or poke changed
    /// something; the pusher waits on it. Cloneable out of the lock.
    pub wake: Arc<Notify>,
    /// Latest usage payload (set by the engine from the Publisher's `on_payload`).
    pub usage: Option<UsagePayload>,
}

impl Brain {
    /// `Brain(pet, usage_only)`: `usage_only()` is true while the clock's main screen is Usage
    /// (then only a poke or an agent that needs you interrupts it).
    pub fn new(pet: Pet, usage_only: Box<dyn Fn() -> bool + Send + Sync>) -> Self {
        todo!("phase 04")
    }

    /// Wrap into the shared handle the server and pusher take.
    pub fn shared(self) -> SharedBrain {
        Arc::new(Mutex::new(self))
    }

    /// `Brain.desired(now)`: `{a, seq, rep, v, u, txt, cd, dim, focus_until}` for this instant
    /// (poke moment > agent > focus > pet moment > idle pick), honouring the minDisplay holds.
    pub fn desired(&mut self, now: f64) -> DesiredState {
        todo!("phase 04")
    }
}
