use std::time::Duration;

use serde::Deserialize;

const DEFAULT_SCRAPE_INTERVAL_S: u64 = 30;

/// A poll with no `extract` publishes the whole body. Long bodies are not useful on a 96 by 96 key
/// and holding them in the variable store serves nothing.
pub const MAX_BODY_VARIABLE_LENGTH: usize = 512;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrometheusConfig {
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub scrape_interval_s: Option<u64>,
    #[serde(default)]
    pub bearer_token: Option<String>,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    /// Scrape path — `/api/v1/query?query=%7B__name__%3D~'.%2B'%7D` for standard Prometheus,
    /// or `/metrics` for raw exposition format.
    #[serde(default)]
    pub path: Option<String>,
}

impl PrometheusConfig {
    pub fn scrape_interval(&self) -> Duration {
        Duration::from_secs(
            self.scrape_interval_s
                .unwrap_or(DEFAULT_SCRAPE_INTERVAL_S)
                .max(10),
        )
    }

    pub fn base_url(&self) -> Result<String, String> {
        if self.target.is_empty() {
            return Err("target is required".to_string());
        }
        if !self.target.starts_with("http://") && !self.target.starts_with("https://") {
            return Err(format!(
                "target must be an absolute URL (http:// or https://), got: {}",
                self.target
            ));
        }
        Ok(self.target.clone())
    }

    pub fn scrape_path(&self) -> String {
        self.path
            .clone()
            .unwrap_or_else(|| "/api/v1/query?query=%7B__name__%3D~'.%2B'%7D".to_string())
    }
}
