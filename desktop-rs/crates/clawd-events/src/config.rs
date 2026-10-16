//! Event-related settings read from the environment and the config file
//! (`events.read_event_port`, `events.read_pet_config`). They take the config reader as a
//! function (`usage.read_config_value` in Python) so tests can feed values directly.

use std::sync::LazyLock;

use clawd_contracts::{Env, PetConfig, EVENT_PORT, EVENT_PORT_ENV};
use regex::Regex;

/// A config lookup: `read_value("event_port")` -> the raw value, if set
/// (`clawd_config::read_config_value` bound to an `AppPaths`).
pub type ReadValue<'a> = &'a dyn Fn(&str) -> Option<String>;

/// `events.read_event_port(read_value)`: `env` (`Env::system()` in the app) `CLAWD_EVENT_PORT`, else config `event_port`, else
/// 47913. Anything unparsable or outside 0..=65535 falls back to 47913; `0` means "any free
/// port" (tests). Also used by the hook installer and the CLI integrations.
pub fn read_event_port(env: &Env, read_value: ReadValue<'_>) -> u16 {
    let raw = env
        .get(EVENT_PORT_ENV)
        .or_else(|| read_value("event_port").filter(|v| !v.is_empty()));
    raw.and_then(|raw| raw.trim().parse::<i64>().ok())
        .and_then(|port| u16::try_from(port).ok())
        .unwrap_or(EVENT_PORT) // 0 = any free port (tests)
}

static HOUR_MINUTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([0-9]{1,2}):([0-9]{2})\s*$").expect("HOUR_MINUTE"));

/// `events.read_pet_config(read_value)`: [`PetConfig`] from `quiet_start`/`quiet_end` (`HH:MM`),
/// `quiet_dim` (0-100), `away_after` (>= 30), `milestone_turns` (>= 0).
pub fn read_pet_config(read_value: ReadValue<'_>) -> PetConfig {
    let mut cfg = PetConfig::default();
    let hour_minute = |name: &str, default: u32| -> u32 {
        read_value(name)
            .and_then(|raw| {
                let caps = HOUR_MINUTE.captures(&raw)?;
                let hours: u32 = caps[1].parse().ok()?;
                let minutes: u32 = caps[2].parse().ok()?;
                Some((hours % 24) * 60 + minutes % 60)
            })
            .unwrap_or(default)
    };
    let number = |name: &str, default: i64| -> i64 {
        read_value(name)
            .and_then(|raw| raw.trim().parse::<i64>().ok())
            .unwrap_or(default)
    };
    cfg.quiet_start = hour_minute("quiet_start", cfg.quiet_start);
    cfg.quiet_end = hour_minute("quiet_end", cfg.quiet_end);
    cfg.quiet_dim = number("quiet_dim", i64::from(cfg.quiet_dim)).clamp(0, 100) as u32;
    cfg.away_after =
        number("away_after", i64::from(cfg.away_after)).clamp(30, i64::from(u32::MAX)) as u32;
    cfg.milestone =
        number("milestone_turns", i64::from(cfg.milestone)).clamp(0, i64::from(u32::MAX)) as u32;
    cfg
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn reader(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn event_port_order_and_fallbacks() {
        let env =
            |v: &str| Env::from_map(HashMap::from([(EVENT_PORT_ENV.to_owned(), v.to_owned())]));
        let none = reader(&[]);
        assert_eq!(read_event_port(&Env::empty(), &none), 47913);
        assert_eq!(
            read_event_port(&Env::empty(), &reader(&[("event_port", " 5000 ")])),
            5000
        );
        assert_eq!(
            read_event_port(&env("6000"), &reader(&[("event_port", "5000")])),
            6000
        );
        assert_eq!(read_event_port(&env("0"), &none), 0);
        assert_eq!(read_event_port(&env("70000"), &none), 47913);
        assert_eq!(read_event_port(&env("abc"), &none), 47913);
        assert_eq!(read_event_port(&env("-1"), &none), 47913);
        // an unparsable env value does not fall through to the config (Python `or`)
        assert_eq!(
            read_event_port(&env("abc"), &reader(&[("event_port", "5000")])),
            47913
        );
    }

    #[test]
    fn pet_config_parses_and_clamps() {
        let c = read_pet_config(&reader(&[
            ("quiet_start", " 22:30 "),
            ("quiet_end", "25:75"),
            ("quiet_dim", "250"),
            ("away_after", "5"),
            ("milestone_turns", "-3"),
        ]));
        assert_eq!(c.quiet_start, 22 * 60 + 30);
        assert_eq!(c.quiet_end, 60 + 15);
        assert_eq!((c.quiet_dim, c.away_after, c.milestone), (100, 30, 0));
        let d = read_pet_config(&reader(&[("quiet_start", "7pm"), ("quiet_dim", "x")]));
        assert_eq!(d, PetConfig::default());
    }
}
