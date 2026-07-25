use std::{collections::BTreeMap, time::Duration};

use serde::Deserialize;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_INTERVAL_MS: u64 = 60_000;
/// A poll with no `extract` publishes the whole body. Long bodies are not useful on a 96 by 96 key
/// and holding them in the variable store serves nothing.
pub const MAX_BODY_VARIABLE_LENGTH: usize = 512;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Sent verbatim as the `Authorization` header when set.
    #[serde(default)]
    pub authorization: Option<toml::Value>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub poll: Vec<PollConfig>,
}

impl HttpConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS))
    }

    /// Joins a request path onto the configured base. An absolute URL in the path wins, so one
    /// instance can serve both a base-relative poll and a one-off call elsewhere.
    pub fn resolve_url(&self, path: &str) -> Result<String, String> {
        if path.starts_with("http://") || path.starts_with("https://") {
            return Ok(path.to_string());
        }
        let Some(base) = &self.base_url else {
            return Err(format!(
                "{path} is relative and this instance has no base_url"
            ));
        };
        Ok(format!(
            "{}/{}",
            base.trim_end_matches('/'),
            path.trim_start_matches('/')
        ))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PollConfig {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub interval_ms: Option<u64>,
    /// A dotted path into the JSON response. Absent means publish the body as text.
    #[serde(default)]
    pub extract: Option<String>,
}

impl PollConfig {
    pub fn interval(&self) -> Duration {
        Duration::from_millis(self.interval_ms.unwrap_or(DEFAULT_INTERVAL_MS).max(100))
    }
}

/// Walks a dotted path into a JSON document. Numeric segments index arrays, so
/// `results.0.temperature` works.
pub fn extract_value<'a>(
    document: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = document;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = match current {
            serde_json::Value::Object(map) => map.get(segment)?,
            serde_json::Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_joins_onto_the_base_url() {
        let config = HttpConfig {
            base_url: Some("https://api.example.com/".to_string()),
            ..HttpConfig::default()
        };
        assert_eq!(
            config.resolve_url("/v1/forecast"),
            Ok("https://api.example.com/v1/forecast".to_string())
        );
    }

    #[test]
    fn an_absolute_url_ignores_the_base_url() {
        let config = HttpConfig {
            base_url: Some("https://api.example.com".to_string()),
            ..HttpConfig::default()
        };
        assert_eq!(
            config.resolve_url("http://other.local/status"),
            Ok("http://other.local/status".to_string())
        );
    }

    #[test]
    fn a_relative_path_without_a_base_url_is_an_error() {
        assert!(HttpConfig::default().resolve_url("/v1/forecast").is_err());
    }

    #[test]
    fn a_dotted_path_walks_objects_and_arrays() {
        let document = serde_json::json!({
            "current": { "temperature_2m": 21.4 },
            "results": [{ "name": "first" }, { "name": "second" }],
        });
        assert_eq!(
            extract_value(&document, "current.temperature_2m"),
            Some(&serde_json::json!(21.4))
        );
        assert_eq!(
            extract_value(&document, "results.1.name"),
            Some(&serde_json::json!("second"))
        );
        assert_eq!(extract_value(&document, "current.missing"), None);
        assert_eq!(
            extract_value(&document, "current.temperature_2m.deeper"),
            None
        );
    }

    #[test]
    fn an_empty_path_returns_the_whole_document() {
        let document = serde_json::json!({ "a": 1 });
        assert_eq!(extract_value(&document, ""), Some(&document));
    }

    #[test]
    fn an_unknown_configuration_key_is_rejected_rather_than_ignored() {
        let parsed: Result<HttpConfig, _> = toml::from_str("base_ur1 = \"typo\"");
        assert!(parsed.is_err());
    }

    #[test]
    fn a_poll_interval_never_drops_below_the_floor() {
        let poll = PollConfig {
            name: "value".to_string(),
            path: "/".to_string(),
            interval_ms: Some(1),
            extract: None,
        };
        assert_eq!(poll.interval(), Duration::from_millis(100));
    }
}
