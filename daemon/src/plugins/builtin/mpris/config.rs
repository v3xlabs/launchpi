use std::time::Duration;

use serde::Deserialize;

const DEFAULT_POSITION_INTERVAL_MS: u64 = 1_000;
/// A progress ring is 24 segments wide; reading the elapsed time faster than this only costs
/// round trips to the player.
const MINIMUM_POSITION_INTERVAL_MS: u64 = 200;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MprisConfig {
    /// Matched as a case-insensitive substring of the player's bus name. Players that do not match
    /// are ignored entirely, so an instance can be pinned to one application.
    #[serde(default)]
    pub preferred_player: Option<String>,
    #[serde(default)]
    pub position_interval_ms: Option<u64>,
}

impl MprisConfig {
    pub fn position_interval(&self) -> Duration {
        Duration::from_millis(
            self.position_interval_ms
                .unwrap_or(DEFAULT_POSITION_INTERVAL_MS)
                .max(MINIMUM_POSITION_INTERVAL_MS),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_configuration_still_polls_the_position() {
        assert_eq!(
            MprisConfig::default().position_interval(),
            Duration::from_millis(1_000)
        );
    }

    #[test]
    fn a_position_interval_never_drops_below_the_floor() {
        let config = MprisConfig {
            position_interval_ms: Some(5),
            ..MprisConfig::default()
        };
        assert_eq!(config.position_interval(), Duration::from_millis(200));
    }

    #[test]
    fn an_unknown_configuration_key_is_rejected_rather_than_ignored() {
        let parsed: Result<MprisConfig, _> = toml::from_str("preferred_playr = \"typo\"");
        assert!(parsed.is_err());
    }
}
