mod config;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tracing::warn;

use crate::plugins::{
    instance::InstanceConfig,
    manifest::{ActionDefinition, ConfigField, PluginManifest, VariableDefinition, VariableKind},
    plugin::{Plugin, PluginContext, PluginError, PluginFactory},
};
use crate::variables::VariableValue;
use config::PrometheusConfig;

pub const FACTORY: PluginFactory = PluginFactory {
    plugin_type: "prometheus",
    manifest,
    start: |config, context| Box::pin(start(config, context)),
};

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_type: "prometheus",
        display_name: "Prometheus",
        description: "Scrape Prometheus metrics and publish them as variables.",
        config_schema: vec![
            ConfigField::text("target")
                .label("Target URL")
                .required()
                .placeholder("http://prometheus:9090")
                .help("The base URL of the Prometheus server."),
            ConfigField::number("scrape_interval_s")
                .label("Scrape interval (seconds)")
                .placeholder("30")
                .help("How often to scrape metrics. Minimum 10 seconds."),
            ConfigField::secret("bearer_token")
                .label("Bearer token")
                .help("Optional Bearer token for authentication."),
            ConfigField::text("path")
                .label("Scrape path")
                .placeholder("/api/v1/query?query=%7B__name__%3D~'.%2B'%7D")
                .help("Path for scraping — use `/metrics` for raw exposition format, or the API query path for standard Prometheus."),
        ],
        actions: vec![
            ActionDefinition::new("query")
                .label("Run PromQL query")
                .description("Execute an arbitrary PromQL query and publish the result.")
                .parameters(vec![
                    ConfigField::text("query")
                        .label("PromQL query")
                        .required()
                        .placeholder("up"),
                    ConfigField::text("variable_name")
                        .label("Variable name")
                        .required()
                        .placeholder("custom_metric"),
                ]),
        ],
        variables: vec![
            VariableDefinition::new("last_scrape", VariableKind::Text)
                .description("Timestamp of last successful scrape."),
        ],
    }
}

pub async fn start(
    config: InstanceConfig,
    context: PluginContext,
) -> Result<Arc<dyn Plugin>, PluginError> {
    let cfg: PrometheusConfig = config
        .deserialize()
        .map_err(|e| PluginError::Configuration(e.to_string()))?;

    let scrape_path = cfg.scrape_path();

    // Build HTTP client headers
    let mut headers = cfg.headers.clone();
    if let Some(ref token) = cfg.bearer_token {
        headers.insert("Authorization".to_string(), format!("Bearer {}", token));
    }

    let plugin = Arc::new(PrometheusPlugin {
        context: context.clone(),
        base_url: cfg.base_url().map_err(|e| PluginError::Configuration(e))?,
        scrape_path,
        interval: cfg.scrape_interval(),
        headers,
    });

    tokio::spawn(scrape_loop(
        plugin.context.clone(),
        plugin.base_url.clone(),
        plugin.scrape_path.clone(),
        plugin.interval,
        plugin.headers.clone(),
    ));

    Ok(plugin)
}

struct PrometheusPlugin {
    context: PluginContext,
    base_url: String,
    scrape_path: String,
    interval: std::time::Duration,
    headers: std::collections::BTreeMap<String, String>,
}

async fn scrape_loop(
    context: PluginContext,
    base_url: String,
    scrape_path: String,
    interval: std::time::Duration,
    headers: std::collections::BTreeMap<String, String>,
) {
    let client = reqwest::Client::new();

    loop {
        tokio::select! {
            _ = context.cancel.cancelled() => return,
            _ = tokio::time::sleep(interval) => {}
        }

        match scrape_metrics(&client, &base_url, &scrape_path, &headers).await {
            Ok(body) => {
                publish_metrics(&context, &body);
            }
            Err(e) => {
                warn!(error = %e, "Failed to scrape metrics");
            }
        }
    }
}

async fn scrape_metrics(
    client: &reqwest::Client,
    base_url: &str,
    scrape_path: &str,
    headers: &std::collections::BTreeMap<String, String>,
) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", base_url, scrape_path);
    let mut builder = client.get(&url);

    // Add custom headers
    for (key, value) in headers {
        builder = builder.header(key, value);
    }

    let response = builder
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Body read failed: {}", e))?;

    // Detect format: JSON API returns {"status":"success","data":{...}}
    let trimmed = text.trim();
    if trimmed.starts_with('{') {
        serde_json::from_str(&text)
            .map_err(|e| format!("JSON parse failed: {}", e))
    } else {
        // Raw exposition format — wrap in JSON for publish_metrics
        let mut metrics: Vec<(String, String)> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            if parts.len() == 2 {
                let (name, value) = (parts[0], parts[1]);
                if let Ok(num) = value.parse::<f64>() {
                    metrics.push((name.to_string(), num.to_string()));
                } else {
                    metrics.push((name.to_string(), value.to_string()));
                }
            }
        }
        Ok(serde_json::json!({ "metrics": metrics }))
    }
}

fn publish_metrics(context: &PluginContext, body: &serde_json::Value) {
    // Try JSON API format first: data.result
    if let Some(result) = body
        .get("data")
        .and_then(|d| d.get("result"))
        .and_then(|r| r.as_array())
    {
        publish_json_metrics(context, result);
        return;
    }

    // Try raw metrics format: { "metrics": [{ "name": "...", "value": "..." }, ...] }
    if let Some(metrics) = body
        .get("metrics")
        .and_then(|m| m.as_array())
    {
        publish_raw_metrics(context, metrics);
        return;
    }
}

fn publish_json_metrics(context: &PluginContext, result: &[serde_json::Value]) {
    for metric in result {
        let metric_name = match metric
            .get("metric")
            .and_then(|m| m.get("__name__"))
            .and_then(|n| n.as_str())
        {
            Some(name) => name,
            None => continue,
        };

        let value = match metric
            .get("value")
            .and_then(|v| v.get(1))
            .and_then(|v| v.as_str())
        {
            Some(v) => v,
            None => continue,
        };

        // Try to parse as number
        if let Ok(num) = value.parse::<f64>() {
            context.set_value(metric_name, VariableValue::Number(num));
        } else {
            context.set_value(metric_name, VariableValue::Text(value.to_string()));
        }
    }

    context.set_value(
        "last_scrape",
        VariableValue::Text(chrono::Local::now().format("%H:%M:%S").to_string()),
    );
}

fn publish_raw_metrics(context: &PluginContext, metrics: &[serde_json::Value]) {
    for m in metrics {
        let name = match m.get("name").and_then(|n| n.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let value = match m.get("value").and_then(|v| v.as_str()) {
            Some(v) => v,
            None => continue,
        };

        // Try to parse as number
        if let Ok(num) = value.parse::<f64>() {
            context.set_value(name, VariableValue::Number(num));
        } else {
            context.set_value(name, VariableValue::Text(value.to_string()));
        }
    }

    context.set_value(
        "last_scrape",
        VariableValue::Text(chrono::Local::now().format("%H:%M:%S").to_string()),
    );
}

#[async_trait]
impl Plugin for PrometheusPlugin {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue) -> Result<(), PluginError> {
        match action_name {
            "query" => {
                let promql = parameters
                    .get("query")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| PluginError::Configuration("query is required".to_string()))?;
                let variable_name = parameters
                    .get("variable_name")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| {
                        PluginError::Configuration("variable_name is required".to_string())
                    })?;

                self.run_query(promql, variable_name).await
            }
            _ => Err(PluginError::UnknownAction(action_name.to_string())),
        }
    }
}

impl PrometheusPlugin {
    async fn run_query(&self, promql: &str, variable_name: &str) -> Result<(), PluginError> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/query", self.base_url);

        let mut builder = client.get(&url).query(&[("query", promql)]);

        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }

        let response = builder
            .send()
            .await
            .map_err(|e| PluginError::Upstream(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(PluginError::Upstream(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PluginError::Upstream(format!("JSON parse failed: {}", e)))?;

        // Extract first result value
        let result = body
            .get("data")
            .and_then(|d| d.get("result"))
            .and_then(|r| r.as_array());

        let first = match result {
            Some(arr) => arr.first(),
            None => {
                return Err(PluginError::Upstream(
                    "No results returned from query".to_string(),
                ))
            }
        };

        let value_str = match first
            .and_then(|v| v.get("value"))
            .and_then(|v| v.get(1))
            .and_then(|v| v.as_str())
        {
            Some(s) => s,
            None => {
                return Err(PluginError::Upstream(
                    "No value in query result".to_string(),
                ))
            }
        };

        if let Ok(num) = value_str.parse::<f64>() {
            self.context
                .set_value(variable_name, VariableValue::Number(num));
        } else {
            self.context
                .set_value(variable_name, VariableValue::Text(value_str.to_string()));
        }

        Ok(())
    }
}
