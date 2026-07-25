use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscordConfig {
    /// Resolved separately by `InstanceConfig::required_secret`.
    pub token: Option<toml::Value>,
    pub guild_id: Option<String>,
    pub channel_id: Option<String>,
    pub user_id: Option<String>,
    #[serde(default = "default_max_members")]
    pub max_members: usize,
}

fn default_max_members() -> usize {
    4
}

impl DiscordConfig {
    /// Only the shape of what is set is checked. A half-configured instance still starts, because
    /// the guild and channel pickers are answered by a *running* instance: refusing to start
    /// without a guild would mean never being able to pick one.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("guild_id", self.guild_id.as_deref()),
            ("channel_id", self.channel_id.as_deref()),
            ("user_id", self.user_id.as_deref()),
        ] {
            if let Some(value) = value {
                validate_id(name, value)?;
            }
        }
        if self.max_members == 0 || self.max_members > 32 {
            return Err("max_members must be between 1 and 32".to_string());
        }
        Ok(())
    }

    /// Whether there is enough here to show anything. Reported rather than rejected.
    pub fn names_a_channel(&self) -> bool {
        self.guild_id.is_some() && (self.channel_id.is_some() || self.user_id.is_some())
    }
}

fn validate_id(name: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{name} must be a Discord snowflake"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picker behind `guild_id` is answered by a running instance, so a token-only instance has
    /// to start for the guild list to ever appear.
    #[test]
    fn a_token_only_instance_starts_but_names_no_channel() {
        let config = DiscordConfig {
            token: None,
            guild_id: None,
            channel_id: None,
            user_id: None,
            max_members: 4,
        };
        assert!(config.validate().is_ok());
        assert!(!config.names_a_channel());
    }

    #[test]
    fn a_malformed_snowflake_is_still_rejected() {
        let config = DiscordConfig {
            token: None,
            guild_id: Some("not-a-snowflake".to_string()),
            channel_id: None,
            user_id: None,
            max_members: 4,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_a_followed_user() {
        let config = DiscordConfig {
            token: None,
            guild_id: Some("1".to_string()),
            channel_id: None,
            user_id: Some("2".to_string()),
            max_members: 4,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn accepts_a_followed_user_with_a_fallback_channel() {
        let config = DiscordConfig {
            token: None,
            guild_id: Some("1".to_string()),
            channel_id: Some("2".to_string()),
            user_id: Some("3".to_string()),
            max_members: 4,
        };
        assert!(config.validate().is_ok());
    }
}
