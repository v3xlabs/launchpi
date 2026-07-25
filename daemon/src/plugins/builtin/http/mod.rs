mod config;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tracing::debug;

use crate::{
    plugins::{
        builtin::http::config::{extract_value, HttpConfig, PollConfig, MAX_BODY_VARIABLE_LENGTH},
        instance::InstanceConfig,
        manifest::{
            ActionDefinition, ConfigField, PluginManifest, VariableDefinition, VariableKind,
        },
        plugin::{Plugin, PluginContext, PluginError, PluginFactory},
    },
    surfaces::logs::SurfaceLogLevel,
    variables::VariableValue,
};

pub const FACTORY: PluginFactory = PluginFactory {
    plugin_type: "http",
    manifest,
    start: |config, context| Box::pin(start(config, context)),
};

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_type: "http",
        display_name: "HTTP",
        description: "Call HTTP endpoints and publish values from their responses.",
        config_schema: vec![
            ConfigField::text("base_url")
                .label("Base URL")
                .placeholder("https://api.example.com")
                .help("Prefixed onto any request path that is not already an absolute URL."),
            ConfigField::number("timeout_ms").label("Timeout (ms)"),
            ConfigField::secret("authorization")
                .label("Authorization")
                .help("Sent verbatim as the Authorization header."),
        ],
        actions: vec![ActionDefinition::new("request")
            .label("Send request")
            .description("Performs one HTTP request. Every field resolves $(...) references.")
            .parameters(vec![
                ConfigField::select("method", ["GET", "POST", "PUT", "PATCH", "DELETE"])
                    .label("Method")
                    .required(),
                ConfigField::text("path").label("Path").required(),
                ConfigField::text("body").label("Body"),
                ConfigField::text("content_type")
                    .label("Content type")
                    .placeholder("application/json"),
            ])],
        variables: vec![VariableDefinition::new("<poll name>", VariableKind::Text)
            .description("One variable per [[config.poll]] entry, named by its name field.")],
    }
}

async fn start(
    config: InstanceConfig,
    context: PluginContext,
) -> Result<Arc<dyn Plugin>, PluginError> {
    let settings: HttpConfig = config.deserialize().map_err(PluginError::Configuration)?;
    if settings.poll.iter().any(|poll| poll.name.trim().is_empty()) {
        return Err(PluginError::Configuration(
            "every poll entry needs a name".to_string(),
        ));
    }
    let authorization = config
        .secret("authorization")
        .map_err(PluginError::Configuration)?;

    let plugin = Arc::new(HttpPlugin {
        settings: settings.clone(),
        authorization,
        context: context.clone(),
    });

    for poll in settings.poll {
        let plugin = plugin.clone();
        let context = context.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(poll.interval());
            loop {
                tokio::select! {
                    _ = context.cancel.cancelled() => break,
                    _ = ticker.tick() => plugin.run_poll(&poll).await,
                }
            }
        });
    }

    Ok(plugin)
}


struct HttpPlugin {
    settings: HttpConfig,
    authorization: Option<String>,
    context: PluginContext,
}

impl HttpPlugin {
    async fn run_poll(&self, poll: &PollConfig) {
        let url = match self.settings.resolve_url(&poll.path) {
            Ok(url) => url,
            Err(reason) => {
                self.context.log(SurfaceLogLevel::Warning, reason);
                return;
            }
        };
        let response = match self.send(reqwest::Method::GET, &url, None, None).await {
            Ok(response) => response,
            Err(error) => {
                self.context
                    .log(SurfaceLogLevel::Warning, format!("{}: {error}", poll.name));
                return;
            }
        };

        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        let (variable, value) = match &poll.extract {
            Some(path) => match serde_json::from_str::<JsonValue>(&body) {
                Ok(document) => match extract_value(&document, path) {
                    Some(found) => (json_to_variable(found), found.clone()),
                    None => {
                        debug!(
                            integration_id = self.context.integration_id.0,
                            poll = poll.name,
                            path,
                            "the response had no value at that path"
                        );
                        (VariableValue::Text(String::new()), JsonValue::Null)
                    }
                },
                Err(error) => {
                    self.context.log(
                        SurfaceLogLevel::Warning,
                        format!("{}: response is not JSON: {error}", poll.name),
                    );
                    (VariableValue::Text(String::new()), JsonValue::Null)
                }
            },
            None => {
                let text: String = body.chars().take(MAX_BODY_VARIABLE_LENGTH).collect();
                (VariableValue::Text(text.clone()), JsonValue::String(text))
            }
        };

        self.context.set_value(poll.name.clone(), variable);
    }

    async fn send(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<String>,
        content_type: Option<String>,
    ) -> Result<reqwest::Response, String> {
        let mut request = self
            .context
            .http
            .request(method, url)
            .timeout(self.settings.timeout());
        for (name, value) in &self.settings.headers {
            request = request.header(name, value);
        }
        if let Some(authorization) = &self.authorization {
            request = request.header(reqwest::header::AUTHORIZATION, authorization);
        }
        if let Some(content_type) = content_type {
            request = request.header(reqwest::header::CONTENT_TYPE, content_type);
        }
        if let Some(body) = body {
            request = request.body(body);
        }
        request.send().await.map_err(|error| error.to_string())
    }

}

#[async_trait]
impl Plugin for HttpPlugin {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue) -> Result<(), PluginError> {
        if action_name != "request" {
            return Err(PluginError::UnknownAction(action_name.to_string()));
        }
        let method = self
            .context
            .interpolate(&string_parameter(parameters, "method")?);
        let method = reqwest::Method::from_bytes(method.trim().to_uppercase().as_bytes())
            .map_err(|_| PluginError::Configuration(format!("{method} is not an HTTP method")))?;
        let path = self
            .context
            .interpolate(&string_parameter(parameters, "path")?);
        let url = self
            .settings
            .resolve_url(&path)
            .map_err(PluginError::Configuration)?;
        let body = optional_string(parameters, "body").map(|body| self.context.interpolate(&body));
        let content_type = optional_string(parameters, "content_type");

        let response = self
            .send(method, &url, body, content_type)
            .await
            .map_err(PluginError::Upstream)?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        Err(PluginError::Upstream(format!(
            "{url} answered {}",
            status.as_u16()
        )))
    }

}

fn json_to_variable(value: &JsonValue) -> VariableValue {
    match value {
        JsonValue::Bool(value) => VariableValue::Boolean(*value),
        JsonValue::Number(value) => VariableValue::Number(value.as_f64().unwrap_or_default()),
        JsonValue::String(value) => VariableValue::Text(value.clone()),
        JsonValue::Null => VariableValue::Text(String::new()),
        other => VariableValue::Text(other.to_string()),
    }
}

fn string_parameter(parameters: &JsonValue, key: &str) -> Result<String, PluginError> {
    optional_string(parameters, key)
        .ok_or_else(|| PluginError::Configuration(format!("{key} is required")))
}

/// Accepts a number or boolean where a string is expected, because a generated form and a
/// hand-written TOML disagree about how `200` should be typed.
fn optional_string(parameters: &JsonValue, key: &str) -> Option<String> {
    match parameters.get(key)? {
        JsonValue::String(value) if value.is_empty() => None,
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Null => None,
        other => Some(other.to_string()),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        identifiers::IntegrationId,
        plugins::plugin::cancellation,
        variables::{VariableRef, VariableStore},
    };
    use std::time::Duration;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    /// Answers every connection with the same JSON body, so a poll has something real to read
    /// without depending on a service being installed.
    async fn serve(body: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
        let port = listener.local_addr().expect("has an address").port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = [0_u8; 1024];
                let _ = socket.read(&mut request).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        port
    }

    /// Holds the pieces an instance's tasks depend on. Dropping the cancel handle stops the poll
    /// loop, and dropping the signal receiver closes the sink the plugin publishes through, so the
    /// test has to keep both alive for as long as the plugin.
    struct Started {
        plugin: Arc<dyn Plugin>,
        variables: Arc<VariableStore>,
        _cancel: crate::plugins::plugin::CancelHandle,
        _signals: tokio::sync::mpsc::Receiver<crate::plugins::engine::EngineSignal>,
    }

    async fn started(config: String) -> Started {
        let variables = Arc::new(VariableStore::default());
        let (signals, receiver) = tokio::sync::mpsc::channel(64);
        let (cancel, token) = cancellation();
        let integration_id = IntegrationId("http.local".to_string());
        let context = PluginContext::new(
            integration_id.clone(),
            variables.clone(),
            signals,
            token,
            reqwest::Client::new(),
        );
        let plugin = start(
            InstanceConfig {
                integration_id,
                values: toml::from_str(&config).expect("valid toml"),
            },
            context,
        )
        .await
        .expect("the instance starts");
        Started {
            plugin,
            variables,
            _cancel: cancel,
            _signals: receiver,
        }
    }

    async fn await_variable(variables: &VariableStore, name: &str) -> VariableValue {
        let reference = VariableRef::new("http.local", name);
        for _ in 0..100 {
            if let Some(value) = variables.get(&reference) {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("{name} was never published");
    }

    #[tokio::test]
    async fn a_poll_publishes_the_extracted_value() {
        let port = serve(r#"{"current":{"temperature_2m":21.4},"state":"on"}"#).await;
        let started = started(format!(
            "base_url = \"http://127.0.0.1:{port}\"\n\
             [[poll]]\n\
             name = \"temperature\"\n\
             path = \"/forecast\"\n\
             interval_ms = 100\n\
             extract = \"current.temperature_2m\"\n"
        ))
        .await;

        assert_eq!(
            await_variable(&started.variables, "temperature").await,
            VariableValue::Number(21.4)
        );
    }

    #[tokio::test]
    async fn a_poll_without_an_extract_publishes_the_body() {
        let port = serve("plain body").await;
        let started = started(format!(
            "base_url = \"http://127.0.0.1:{port}\"\n\
             [[poll]]\n\
             name = \"raw\"\n\
             path = \"/\"\n\
             interval_ms = 100\n"
        ))
        .await;

        assert_eq!(
            await_variable(&started.variables, "raw").await,
            VariableValue::Text("plain body".to_string())
        );
    }


    #[tokio::test]
    async fn an_unknown_action_and_feedback_are_reported_by_name() {
        let port = serve("{}").await;
        let started = started(format!("base_url = \"http://127.0.0.1:{port}\"\n")).await;

        assert_eq!(
            started.plugin.invoke("nope", &serde_json::json!({})).await,
            Err(PluginError::UnknownAction("nope".to_string()))
        );
    }

    #[tokio::test]
    async fn a_request_action_reaches_the_server() {
        let port = serve("{}").await;
        let started = started(format!("base_url = \"http://127.0.0.1:{port}\"\n")).await;

        assert_eq!(
            started
                .plugin
                .invoke(
                    "request",
                    &serde_json::json!({ "method": "POST", "path": "/toggle", "body": "{}" })
                )
                .await,
            Ok(())
        );
    }

    #[test]
    fn a_numeric_parameter_is_accepted_as_a_string() {
        let parameters = serde_json::json!({ "status": 200, "poll": "value" });
        assert_eq!(
            optional_string(&parameters, "status"),
            Some("200".to_string())
        );
    }


    #[test]
    fn an_empty_string_parameter_reads_as_absent() {
        let parameters = serde_json::json!({ "body": "" });
        assert_eq!(optional_string(&parameters, "body"), None);
        assert!(string_parameter(&parameters, "body").is_err());
    }

    #[test]
    fn json_scalars_map_onto_variable_values() {
        assert_eq!(
            json_to_variable(&serde_json::json!(21.5)),
            VariableValue::Number(21.5)
        );
        assert_eq!(
            json_to_variable(&serde_json::json!("on")),
            VariableValue::Text("on".to_string())
        );
        assert_eq!(
            json_to_variable(&serde_json::json!(true)),
            VariableValue::Boolean(true)
        );
        assert_eq!(
            json_to_variable(&serde_json::json!(null)),
            VariableValue::Text(String::new())
        );
    }

    #[test]
    fn a_json_object_falls_back_to_its_serialized_form() {
        assert_eq!(
            json_to_variable(&serde_json::json!({ "a": 1 })),
            VariableValue::Text("{\"a\":1}".to_string())
        );
    }
}
