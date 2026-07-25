use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::identifiers::IntegrationId;

/// How a plugin instance names a credential.
///
/// The inline form exists so the web UI can set a token without the user preparing a file or an
/// environment variable first. It is also the only form an export will not reproduce verbatim:
/// [`SecretRef::exported`] rewrites it into an environment reference, which keeps a copied
/// configuration pasteable into a repository.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SecretRef {
    Environment { env: String },
    File { file: PathBuf },
    Inline(String),
}

impl SecretRef {
    pub fn resolve(&self) -> Result<String, String> {
        match self {
            Self::Environment { env } => {
                std::env::var(env).map_err(|_| format!("environment variable {env} is not set"))
            }
            Self::File { file } => fs::read_to_string(file)
                .map(|contents| contents.trim().to_string())
                .map_err(|error| format!("could not read {}: {error}", file.display())),
            Self::Inline(value) => Ok(value.clone()),
        }
    }

    pub fn is_inline(&self) -> bool {
        matches!(self, Self::Inline(_))
    }

    /// The form this reference takes in an exported document. Inline values become a reference to
    /// an environment variable named after the instance and the field, so the export is not merely
    /// redacted but usable once that variable is set.
    pub fn exported(&self, integration_id: &IntegrationId, field: &str) -> Self {
        match self {
            Self::Inline(_) => Self::Environment {
                env: placeholder_name(integration_id, field),
            },
            other => other.clone(),
        }
    }
}

fn placeholder_name(integration_id: &IntegrationId, field: &str) -> String {
    let mut name = String::from("LAUNCHPI");
    for part in [integration_id.0.as_str(), field] {
        name.push('_');
        name.extend(part.chars().map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        }));
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_string_deserializes_as_an_inline_secret() {
        let parsed: SecretRef = toml::from_str("value = \"hunter2\"")
            .map(|document: toml::Table| document["value"].clone())
            .and_then(SecretRef::deserialize)
            .expect("a string is a valid secret reference");
        assert_eq!(parsed, SecretRef::Inline("hunter2".to_string()));
    }

    #[test]
    fn a_table_deserializes_as_the_reference_it_names() {
        let document: toml::Table = toml::from_str(
            "from_env = { env = \"TOKEN\" }\nfrom_file = { file = \"/run/secret\" }",
        )
        .expect("valid toml");
        assert_eq!(
            SecretRef::deserialize(document["from_env"].clone()).expect("valid reference"),
            SecretRef::Environment {
                env: "TOKEN".to_string()
            }
        );
        assert_eq!(
            SecretRef::deserialize(document["from_file"].clone()).expect("valid reference"),
            SecretRef::File {
                file: PathBuf::from("/run/secret")
            }
        );
    }

    #[test]
    fn a_missing_environment_variable_is_an_error_rather_than_an_empty_credential() {
        let reference = SecretRef::Environment {
            env: "LAUNCHPI_TEST_DEFINITELY_UNSET".to_string(),
        };
        assert!(reference.resolve().is_err());
    }

    #[test]
    fn exporting_replaces_an_inline_value_with_a_usable_environment_reference() {
        let integration_id = IntegrationId("hass.home".to_string());
        let exported = SecretRef::Inline("hunter2".to_string()).exported(&integration_id, "token");
        assert_eq!(
            exported,
            SecretRef::Environment {
                env: "LAUNCHPI_HASS_HOME_TOKEN".to_string()
            }
        );
    }

    #[test]
    fn exporting_leaves_an_indirect_reference_alone() {
        let integration_id = IntegrationId("hass.home".to_string());
        let reference = SecretRef::File {
            file: PathBuf::from("/run/agenix/token"),
        };
        assert_eq!(reference.exported(&integration_id, "token"), reference);
    }
}
