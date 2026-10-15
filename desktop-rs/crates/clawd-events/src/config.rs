//! Event-related settings read from the environment and the config file
//! (`events.read_event_port`, `events.read_pet_config`). They take the config reader as a
//! function (`usage.read_config_value` in Python) so tests can feed values directly.

use clawd_contracts::PetConfig;

/// A config lookup: `read_value("event_port")` -> the raw value, if set
/// (`clawd_config::read_config_value` bound to an `AppPaths`).
pub type ReadValue<'a> = &'a dyn Fn(&str) -> Option<String>;

/// `events.read_event_port(read_value)`: env `CLAWD_EVENT_PORT`, else config `event_port`, else
/// 47913. Anything unparsable or outside 0..=65535 falls back to 47913; `0` means "any free
/// port" (tests). Also used by the hook installer and the CLI integrations.
pub fn read_event_port(read_value: ReadValue<'_>) -> u16 {
    todo!("phase 04")
}

/// `events.read_pet_config(read_value)`: [`PetConfig`] from `quiet_start`/`quiet_end` (`HH:MM`),
/// `quiet_dim` (0-100), `away_after` (>= 30), `milestone_turns` (>= 0).
pub fn read_pet_config(read_value: ReadValue<'_>) -> PetConfig {
    todo!("phase 04")
}
