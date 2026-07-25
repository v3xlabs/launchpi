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
    pub fn validate(&self) -> Result<(), String> {
        let guild_id = self
            .guild_id
            .as_deref()
            .ok_or_else(|| "guild_id is required".to_string())?;
        validate_id("guild_id", guild_id)?;
        if let Some(channel_id) = self.channel_id.as_deref() {
            validate_id("channel_id", channel_id)?;
        }
        if let Some(user_id) = self.user_id.as_deref() {
            validate_id("user_id", user_id)?;
        }
        if self.channel_id.is_some() && self.user_id.is_some() {
            return Err("configure only one of channel_id or user_id".to_string());
        }
        if self.channel_id.is_none() && self.user_id.is_none() {
            return Err("one of channel_id or user_id is required".to_string());
        }
        if self.max_members == 0 || self.max_members > 32 {
            return Err("max_members must be between 1 and 32".to_string());
        }
        Ok(())
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

    #[test]
    fn requires_a_fixed_channel_or_followed_user() {
        let config = DiscordConfig {
            token: None,
            guild_id: Some("1".to_string()),
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
}
