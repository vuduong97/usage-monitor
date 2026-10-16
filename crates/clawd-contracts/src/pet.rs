use serde::{Deserialize, Serialize};

/// `events.PetConfig`: when Clawd sleeps, worries and celebrates. Read from the config file by
/// `clawd_events::read_pet_config` (`quiet_start`, `quiet_end`, `quiet_dim`, `away_after`,
/// `milestone_turns`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PetConfig {
    /// Minutes since local midnight (default 23:00).
    pub quiet_start: u32,
    /// Minutes since local midnight (default 07:00).
    pub quiet_end: u32,
    /// Brightness cap in quiet hours, percent (0-100).
    pub quiet_dim: u32,
    /// HID idle seconds before Clawd dozes off (>= 30 when read from config).
    pub away_after: u32,
    /// Session percent that worries Clawd.
    pub low_pct: u32,
    pub low_every: u32,
    pub yawn_from: u32,
    pub yawn_every: u32,
    /// Turns per day between celebrations (0 = never).
    pub milestone: u32,
}

impl Default for PetConfig {
    fn default() -> Self {
        Self {
            quiet_start: 23 * 60,
            quiet_end: 7 * 60,
            quiet_dim: 15,
            away_after: 300,
            low_pct: 80,
            low_every: 900,
            yawn_from: 22 * 60,
            yawn_every: 1800,
            milestone: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_python() {
        let c = PetConfig::default();
        assert_eq!((c.quiet_start, c.quiet_end, c.quiet_dim), (1380, 420, 15));
        assert_eq!((c.away_after, c.low_pct, c.low_every), (300, 80, 900));
        assert_eq!((c.yawn_from, c.yawn_every, c.milestone), (1320, 1800, 10));
    }
}
