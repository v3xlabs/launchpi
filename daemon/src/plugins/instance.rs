use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{models::identifiers::IntegrationId, plugins::secret::SecretRef};

pub const INSTANCE_DOCUMENT_VERSION: u8 = 1;

/// One `plugins/<type>.<name>.toml` file.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstanceDocument {
    pub version: u8,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub config: toml::Table,
}

fn enabled_by_default() -> bool {
    true
}

impl Default for InstanceDocument {
    fn default() -> Self {
        Self {
            version: INSTANCE_DOCUMENT_VERSION,
            enabled: true,
            display_name: None,
            config: toml::Table::new(),
        }
    }
}

/// The identity carried by a plugin instance file name. The file name is the identity, which is
/// why there is no index file to keep in sync.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstanceIdentity {
    pub plugin_type: String,
    pub name: String,
}

impl InstanceIdentity {
    pub fn integration_id(&self) -> IntegrationId {
        IntegrationId(format!("{}.{}", self.plugin_type, self.name))
    }

    pub fn file_name(&self) -> String {
        format!("{}.{}.toml", self.plugin_type, self.name)
    }
}

/// Parses `<type>.<name>` out of a plugin file stem.
pub fn parse_instance_stem(stem: &str) -> Result<InstanceIdentity, String> {
    let Some((plugin_type, name)) = stem.split_once('.') else {
        return Err(format!(
            "{stem} is not a plugin instance name; expected <type>.<name>"
        ));
    };
    if !is_identifier(plugin_type) {
        return Err(format!("{plugin_type} is not a valid plugin type"));
    }
    if !is_identifier(name) {
        return Err(format!(
            "{name} is not a valid instance name; use lower-case letters, digits and dashes"
        ));
    }
    Ok(InstanceIdentity {
        plugin_type: plugin_type.to_string(),
        name: name.to_string(),
    })
}

fn is_identifier(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

/// A validated instance configuration handed to a plugin's `start`.
#[derive(Clone, Debug)]
pub struct InstanceConfig {
    pub integration_id: IntegrationId,
    pub values: toml::Table,
}

impl InstanceConfig {
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, String> {
        toml::Value::Table(self.values.clone())
            .try_into()
            .map_err(|error| error.to_string())
    }

    /// Resolves a secret field once, at start. A missing environment variable or an unreadable
    /// file is an error here rather than an empty credential used later.
    pub fn secret(&self, key: &str) -> Result<Option<String>, String> {
        let Some(value) = self.values.get(key) else {
            return Ok(None);
        };
        let reference = SecretRef::deserialize(value.clone())
            .map_err(|error| format!("{key} is not a valid secret reference: {error}"))?;
        reference
            .resolve()
            .map(Some)
            .map_err(|reason| format!("{key}: {reason}"))
    }

    pub fn required_secret(&self, key: &str) -> Result<String, String> {
        self.secret(key)?
            .ok_or_else(|| format!("{key} is required"))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PluginInstance {
    pub integration_id: IntegrationId,
    pub plugin_type: String,
    pub name: String,
    pub display_name: String,
    pub is_enabled: bool,
    pub status: PluginInstanceStatus,
    /// Current configuration with every declared secret removed. The browser needs the rest to
    /// populate its form, and must never be handed a credential to echo back.
    pub config: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PluginInstanceStatus {
    Starting,
    Running,
    Error { reason: String },
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stem_splits_into_type_and_name() {
        let identity = parse_instance_stem("http.weather").expect("valid stem");
        assert_eq!(identity.plugin_type, "http");
        assert_eq!(identity.name, "weather");
        assert_eq!(
            identity.integration_id(),
            IntegrationId("http.weather".to_string())
        );
        assert_eq!(identity.file_name(), "http.weather.toml");
    }

    #[test]
    fn a_stem_without_a_name_is_rejected() {
        assert!(parse_instance_stem("http").is_err());
    }

    #[test]
    fn names_are_restricted_to_lower_case_letters_digits_and_dashes() {
        assert!(parse_instance_stem("http.living-room").is_ok());
        assert!(parse_instance_stem("http.Living").is_err());
        assert!(parse_instance_stem("http.-leading").is_err());
        assert!(parse_instance_stem("http.with space").is_err());
        assert!(parse_instance_stem("http.").is_err());
    }

    #[test]
    fn a_second_dot_is_rejected_rather_than_folded_into_the_name() {
        assert!(parse_instance_stem("http.a.b").is_err());
    }

    #[test]
    fn a_secret_field_resolves_through_its_reference() {
        let config = InstanceConfig {
            integration_id: IntegrationId("http.local".to_string()),
            values: toml::from_str("token = \"hunter2\"").expect("valid toml"),
        };
        assert_eq!(config.secret("token"), Ok(Some("hunter2".to_string())));
        assert_eq!(config.secret("absent"), Ok(None));
        assert!(config.required_secret("absent").is_err());
    }
}
