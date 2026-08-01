mod config;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;
use tracing::warn;

use crate::panels::control::ControlTemplate;
use crate::panels::rendered_state::{Anchor9, Edge, Layer, RenderedState, RgbaColor, ValueBinding};
use crate::plugins::{
    instance::InstanceConfig,
    manifest::{ActionDefinition, ConfigField, PluginManifest, VariableDefinition, VariableKind},
    plugin::{Plugin, PluginContext, PluginError, PluginFactory},
    preset::Preset,
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
        ],
        actions: vec![
            ActionDefinition::new("query")
                .label("Run PromQL query")
                .description("Execute an arbitrary PromQL query and publish the result.")
                .parameters(vec![
                    ConfigField::text("query")
                        .label("PromQL query")
                        .required()
                        .placeholder("up or 100 * (1 - node_cpu_seconds_total{mode=\"idle\"} / rate(node_cpu_seconds_total[5m]))"),
                    ConfigField::text("variable_name")
                        .label("Variable name")
                        .required()
                        .placeholder("custom_metric"),
                ]),
        ],
        variables: vec![
            VariableDefinition::new("metrics", VariableKind::Text)
                .description("Last scraped metrics as a JSON text blob (truncated)."),
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

    let base_url = cfg.base_url().map_err(|e| PluginError::Configuration(e))?;

    // Build HTTP client headers
    let mut headers = cfg.headers.clone();
    if let Some(ref token) = cfg.bearer_token {
        headers.insert("Authorization".to_string(), format!("Bearer {}", token));
    }

    context.set_presets(presets());

    let plugin = Arc::new(PrometheusPlugin {
        context: context.clone(),
        base_url,
        interval: cfg.scrape_interval(),
        headers,
        scrape_state: Mutex::new(ScrapeState::default()),
    });

    tokio::spawn(scrape_loop(
        plugin.context.clone(),
        plugin.base_url.clone(),
        plugin.interval,
        plugin.headers.clone(),
    ));

    Ok(plugin)
}

struct PrometheusPlugin {
    context: PluginContext,
    base_url: String,
    interval: std::time::Duration,
    headers: std::collections::BTreeMap<String, String>,
    scrape_state: Mutex<ScrapeState>,
}

struct ScrapeState {
    last_success: Option<std::time::Instant>,
    last_error: Option<String>,
}

impl Default for ScrapeState {
    fn default() -> Self {
        Self {
            last_success: None,
            last_error: None,
        }
    }
}

async fn scrape_loop(
    context: PluginContext,
    base_url: String,
    interval: std::time::Duration,
    headers: std::collections::BTreeMap<String, String>,
) {
    let client = reqwest::Client::new();

    loop {
        tokio::select! {
            _ = context.cancel.cancelled() => return,
            _ = tokio::time::sleep(interval) => {}
        }

        match scrape_metrics(&client, &context, &base_url, &headers).await {
            Ok(metrics) => {
                publish_metrics(&context, &metrics);
            }
            Err(e) => {
                warn!(error = %e, "Failed to scrape Prometheus metrics");
            }
        }
    }
}

async fn scrape_metrics(
    client: &reqwest::Client,
    _context: &PluginContext,
    base_url: &str,
    headers: &std::collections::BTreeMap<String, String>,
) -> Result<serde_json::Value, String> {
    // Query for all metrics
    let url = format!("{}/api/v1/query", base_url);
    let mut builder = client.get(&url).query(&[("query", "{__name__=~'.+'}")]);

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

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("JSON parse failed: {}", e))?;

    Ok(body)
}

fn publish_metrics(context: &PluginContext, body: &serde_json::Value) {
    let result = match body
        .get("data")
        .and_then(|d| d.get("result"))
        .and_then(|r| r.as_array())
    {
        Some(arr) => arr,
        None => return,
    };

    let mut metrics_text = String::new();

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

        if metrics_text.len() < config::MAX_BODY_VARIABLE_LENGTH {
            if !metrics_text.is_empty() {
                metrics_text.push(',');
            }
            metrics_text.push_str(&format!("{}:{}", metric_name, value));
        }
    }

    context.set_value("metrics", VariableValue::Text(metrics_text));
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

// Preset generation - common Prometheus metrics
fn presets() -> Vec<Preset> {
    vec![
        Preset {
            preset_id: "cpu_usage".to_string(),
            category: "Prometheus".to_string(),
            name: "CPU Usage".to_string(),
            description: Some("CPU utilization percentage.".to_string()),
            control: ControlTemplate {
                name: "CPU Usage".to_string(),
                default_state: percentage_state("$(self:100 * (1 - avg(rate(node_cpu_seconds_total{mode=\"idle\"}[5m]))))".to_string()),
                pressed_state: None,
                action_bindings: Vec::new(),
            },
        },
        Preset {
            preset_id: "memory_usage".to_string(),
            category: "Prometheus".to_string(),
            name: "Memory Usage".to_string(),
            description: Some("Memory utilization percentage.".to_string()),
            control: ControlTemplate {
                name: "Memory Usage".to_string(),
                default_state: percentage_state("$(self:100 * (1 - node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes))".to_string()),
                pressed_state: None,
                action_bindings: Vec::new(),
            },
        },
        Preset {
            preset_id: "disk_usage".to_string(),
            category: "Prometheus".to_string(),
            name: "Disk Usage".to_string(),
            description: Some("Disk utilization percentage.".to_string()),
            control: ControlTemplate {
                name: "Disk Usage".to_string(),
                default_state: percentage_state("$(self:100 * (1 - node_filesystem_avail_bytes{mountpoint=\"/\"} / node_filesystem_size_bytes{mountpoint=\"/\"}))".to_string()),
                pressed_state: None,
                action_bindings: Vec::new(),
            },
        },
    ]
}

fn percentage_state(promql: String) -> RenderedState {
    RenderedState {
        layers: vec![
            Layer::Fill {
                color: RgbaColor::opaque(0, 0, 0).into(),
            },
            Layer::Text {
                text: promql.clone(),
                color: RgbaColor::opaque(255, 255, 255).into(),
                anchor: Anchor9::Center,
                font_family: None,
                font_size: None,
            },
            Layer::Bar {
                value: ValueBinding::Literal(0),
                maximum: 100.into(),
                color: RgbaColor::opaque(255, 255, 255).into(),
                edge: Edge::Bottom,
                thickness: 5,
            },
        ],
        is_pressed: false,
    }
}
