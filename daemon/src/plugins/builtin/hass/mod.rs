mod actions;
mod config;
mod connection;
mod presets;
mod protocol;
mod values;

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, oneshot};

use crate::plugins::{
    builtin::hass::{
        actions::ENTITY_LOOKUP,
        config::{websocket_url, HassConfig},
        connection::{PendingCommand, Shared},
        protocol::ServiceCall,
    },
    engine::SUGGESTION_SOURCE,
    instance::InstanceConfig,
    manifest::{ConfigField, PluginManifest, VariableDefinition, VariableKind},
    plugin::{LookupOption, Plugin, PluginContext, PluginError, PluginFactory, Subscription},
};

/// How long a service call waits for its result before the key that triggered it is told the call
/// failed. Long enough for a hub to reach a bulb, short enough not to queue up presses.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_QUEUE_SIZE: usize = 32;

pub const FACTORY: PluginFactory = PluginFactory {
    plugin_type: "hass",
    manifest,
    start: |config, context| Box::pin(start(config, context)),
};

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_type: "hass",
        display_name: "Home Assistant",
        description: "Entities and services from a Home Assistant installation.",
        config_schema: vec![
            ConfigField::text("url")
                .label("URL")
                .placeholder("http://homeassistant.local:8123")
                .required()
                .help("The address you open Home Assistant at."),
            ConfigField::secret("token")
                .label("Access token")
                .required()
                .help("A long-lived access token from your Home Assistant profile."),
        ],
        actions: actions::definitions(),
        variables: variables(),
    }
}

/// Every value name is `<entity_id>.<field>`, so what a panel can read is described by field rather
/// than enumerated: an installation's entities are its own.
fn variables() -> Vec<VariableDefinition> {
    vec![
        VariableDefinition::new("<entity_id>.state", VariableKind::Text)
            .description("The entity's state, such as on, off or 21.4."),
        VariableDefinition::new("<entity_id>.on", VariableKind::Boolean)
            .description("Whether the entity's state is on."),
        VariableDefinition::new("<entity_id>.color", VariableKind::Text).description(
            "The entity's colour, for background_color and foreground_color. White when an entity \
             without a colour is on, black otherwise.",
        ),
        VariableDefinition::new("<entity_id>.brightness", VariableKind::Number)
            .description("Brightness from 0 to 255."),
        VariableDefinition::new("<entity_id>.brightness_pct", VariableKind::Number)
            .description("Brightness from 0 to 100."),
        VariableDefinition::new("<entity_id>.<attribute>", VariableKind::Text)
            .description("Any other attribute, such as light.kitchen.friendly_name."),
    ]
}

async fn start(
    config: InstanceConfig,
    context: PluginContext,
) -> Result<Arc<dyn Plugin>, PluginError> {
    let settings: HassConfig = config.deserialize().map_err(PluginError::Configuration)?;
    let url = settings
        .url
        .ok_or_else(|| PluginError::Configuration("url is required".to_string()))?;
    let url = websocket_url(&url).map_err(PluginError::Configuration)?;
    let token = config
        .required_secret("token")
        .map_err(PluginError::Configuration)?;
    if token.trim().is_empty() {
        return Err(PluginError::Configuration(
            "token resolved to an empty value".to_string(),
        ));
    }

    let (commands, receiver) = mpsc::channel(COMMAND_QUEUE_SIZE);
    let shared = Arc::new(Shared::new(commands));

    tokio::spawn(connection::run(
        context.clone(),
        url,
        token,
        shared.clone(),
        receiver,
    ));

    Ok(Arc::new(HassPlugin { shared, context }))
}

struct HassPlugin {
    shared: Arc<Shared>,
    context: PluginContext,
}

impl HassPlugin {
    /// Waits for the result rather than firing and forgetting, so a refused service call reaches
    /// the log of the surface whose key was pressed.
    async fn call(&self, call: ServiceCall) -> Result<(), PluginError> {
        if !self.shared.is_connected() {
            return Err(PluginError::Upstream(
                "not connected to Home Assistant".to_string(),
            ));
        }

        let (respond, answered) = oneshot::channel();
        self.shared
            .commands
            .send(PendingCommand { call, respond })
            .await
            .map_err(|_| PluginError::Upstream("the connection has stopped".to_string()))?;

        match tokio::time::timeout(CALL_TIMEOUT, answered).await {
            Ok(Ok(Ok(_))) => Ok(()),
            Ok(Ok(Err(reason))) => Err(PluginError::Upstream(reason)),
            Ok(Err(_)) => Err(PluginError::Upstream(
                "the connection closed before Home Assistant answered".to_string(),
            )),
            Err(_) => Err(PluginError::Upstream(
                "Home Assistant did not answer in time".to_string(),
            )),
        }
    }
}

#[async_trait]
impl Plugin for HassPlugin {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue) -> Result<(), PluginError> {
        let call = actions::service_call(action_name, parameters, &|template| {
            self.context.interpolate(template)
        })?;
        self.call(call).await
    }

    async fn lookup(&self, source: &str, query: &str) -> Result<Vec<LookupOption>, PluginError> {
        match source {
            ENTITY_LOOKUP => Ok(self.shared.entity_options(query)),
            SUGGESTION_SOURCE => Ok(self.shared.value_options(query)),
            other => Err(PluginError::Configuration(format!(
                "unknown lookup {other}"
            ))),
        }
    }

    async fn subscribe(&self, subscriptions: &[Subscription]) -> Result<(), PluginError> {
        self.shared.watch(subscriptions);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bindings::action::Action,
        identifiers::IntegrationId,
        panels::rendered_state::RgbaColor,
        plugins::{
            engine::EngineSignal,
            plugin::{cancellation, CancelHandle},
            preset::{Preset, PresetStore},
        },
        variables::{VariableRef, VariableStore, VariableValue},
    };
    use futures::{SinkExt, StreamExt};
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    const TOKEN: &str = "hunter2";
    /// Comfortably past the connection's first retry delay.
    const FIRST_RETRY_ALLOWANCE: Duration = Duration::from_millis(1_500);

    fn kitchen_state(is_on: bool) -> JsonValue {
        json!({
            "entity_id": "light.kitchen",
            "state": if is_on { "on" } else { "off" },
            "attributes": {
                "friendly_name": "Kitchen",
                "brightness": if is_on { 128 } else { 0 },
                "rgb_color": if is_on { json!([232, 185, 35]) } else { JsonValue::Null },
            },
        })
    }

    fn frame(message: JsonValue) -> Message {
        Message::Text(message.to_string())
    }

    /// Speaks enough of the websocket API for a connection to complete its handshake, seed its
    /// states and receive a service call. Every service call it is given is forwarded to the test.
    async fn home_assistant(calls: mpsc::UnboundedSender<JsonValue>) -> u16 {
        home_assistant_closing(calls, 0).await
    }

    /// `closes_first` connections are accepted and then hung up on immediately after the handshake,
    /// which is what a restarting installation looks like from here.
    async fn home_assistant_closing(
        calls: mpsc::UnboundedSender<JsonValue>,
        closes_first: usize,
    ) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
        let port = listener.local_addr().expect("has an address").port();
        let accepted = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let calls = calls.clone();
                let is_early_close =
                    accepted.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < closes_first;
                tokio::spawn(async move {
                    let mut socket = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("the client speaks websocket");
                    let _ = socket
                        .send(frame(
                            json!({ "type": "auth_required", "ha_version": "2026.7.1" }),
                        ))
                        .await;

                    let Some(Ok(Message::Text(payload))) = socket.next().await else {
                        return;
                    };
                    let authentication: JsonValue =
                        serde_json::from_str(&payload).expect("the client sends JSON");
                    if authentication["access_token"] != json!(TOKEN) {
                        let _ = socket
                            .send(frame(json!({
                                "type": "auth_invalid",
                                "message": "Invalid access token",
                            })))
                            .await;
                        return;
                    }
                    let _ = socket.send(frame(json!({ "type": "auth_ok" }))).await;

                    while let Some(Ok(message)) = socket.next().await {
                        let Message::Text(payload) = message else {
                            continue;
                        };
                        let command: JsonValue =
                            serde_json::from_str(&payload).expect("the client sends JSON");
                        if is_early_close {
                            let _ = socket.close(None).await;
                            return;
                        }
                        let id = command["id"].clone();
                        let result = match command["type"].as_str().unwrap_or_default() {
                            "get_states" => json!([
                                kitchen_state(false),
                                { "entity_id": "sensor.outside_temp", "state": "9.6", "attributes": {} },
                            ]),
                            "call_service" => {
                                let _ = calls.send(command.clone());
                                JsonValue::Null
                            }
                            _ => JsonValue::Null,
                        };
                        let _ = socket
                            .send(frame(
                                json!({ "id": id, "type": "result", "success": true, "result": result }),
                            ))
                            .await;

                        if command["type"] == json!("get_states") {
                            let _ = socket
                                .send(frame(json!({
                                    "id": 1,
                                    "type": "event",
                                    "event": {
                                        "event_type": "state_changed",
                                        "data": {
                                            "entity_id": "light.kitchen",
                                            "old_state": kitchen_state(false),
                                            "new_state": kitchen_state(true),
                                        },
                                    },
                                })))
                                .await;
                        }
                    }
                });
            }
        });
        port
    }

    struct Started {
        plugin: Arc<dyn Plugin>,
        variables: Arc<VariableStore>,
        presets: Arc<PresetStore>,
        signals: mpsc::Receiver<EngineSignal>,
        _cancel: CancelHandle,
    }

    async fn started(config: String) -> Result<Started, PluginError> {
        let variables = Arc::new(VariableStore::default());
        let presets = Arc::new(PresetStore::default());
        let (signals, receiver) = mpsc::channel(256);
        let (cancel, token) = cancellation();
        let integration_id = IntegrationId("hass.home".to_string());
        let context = PluginContext::new(
            integration_id.clone(),
            variables.clone(),
            presets.clone(),
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
        .await?;

        Ok(Started {
            plugin,
            variables,
            presets,
            signals: receiver,
            _cancel: cancel,
        })
    }

    /// Waits for the instance to have recommended something and answers with it.
    async fn await_presets(presets: &PresetStore) -> Vec<Preset> {
        for _ in 0..250 {
            if let Some((_, offered)) = presets.snapshot().into_iter().next() {
                return offered;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the instance never recommended anything");
    }

    /// Waits for a value to settle on what is expected. A seeded state and the event that follows
    /// it both publish, so an assertion on whatever arrived first would be a race.
    async fn await_value(variables: &VariableStore, name: &str, expected: VariableValue) {
        let reference = VariableRef::new("hass.home", name);
        let mut seen = None;
        for _ in 0..250 {
            seen = variables.get(&reference);
            if seen.as_ref() == Some(&expected) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("{name} settled on {seen:?} rather than {expected:?}");
    }

    /// Waits for the `occurrences`th log line mentioning `needle` and answers with it.
    async fn await_log(
        signals: &mut mpsc::Receiver<EngineSignal>,
        needle: &str,
        occurrences: usize,
    ) -> String {
        let found = tokio::time::timeout(Duration::from_secs(5), async {
            let mut seen = 0;
            while let Some(signal) = signals.recv().await {
                if let EngineSignal::InstanceLog { message, .. } = signal {
                    if message.contains(needle) {
                        seen += 1;
                        if seen == occurrences {
                            return message;
                        }
                    }
                }
            }
            panic!("the instance stopped logging");
        })
        .await;
        found.unwrap_or_else(|_| panic!("fewer than {occurrences} log lines mentioned {needle}"))
    }

    async fn no_log(signals: &mut mpsc::Receiver<EngineSignal>, needle: &str, within: Duration) {
        let _ = tokio::time::timeout(within, async {
            while let Some(signal) = signals.recv().await {
                if let EngineSignal::InstanceLog { message, .. } = signal {
                    assert!(!message.contains(needle), "{message}");
                }
            }
        })
        .await;
    }

    fn subscriptions(names: &[&str]) -> Vec<Subscription> {
        names
            .iter()
            .map(|name| Subscription {
                name: (*name).to_string(),
            })
            .collect()
    }

    #[tokio::test]
    async fn subscribed_values_are_published_from_seeded_states_and_events() {
        let (calls, _received) = mpsc::unbounded_channel();
        let port = home_assistant(calls).await;
        let started = started(format!(
            "url = \"http://127.0.0.1:{port}\"\ntoken = \"{TOKEN}\"\n"
        ))
        .await
        .expect("the instance starts");

        started
            .plugin
            .subscribe(&subscriptions(&[
                "light.kitchen.state",
                "light.kitchen.color",
                "light.kitchen.brightness_pct",
                "light.kitchen.friendly_name",
            ]))
            .await
            .expect("subscriptions are accepted");

        await_value(
            &started.variables,
            "light.kitchen.state",
            VariableValue::Text("on".to_string()),
        )
        .await;
        await_value(
            &started.variables,
            "light.kitchen.color",
            VariableValue::Color(RgbaColor::opaque(232, 185, 35)),
        )
        .await;
        await_value(
            &started.variables,
            "light.kitchen.brightness_pct",
            VariableValue::Number(50.0),
        )
        .await;
        await_value(
            &started.variables,
            "light.kitchen.friendly_name",
            VariableValue::Text("Kitchen".to_string()),
        )
        .await;
    }

    #[tokio::test]
    async fn an_entity_nothing_watches_is_never_published() {
        let (calls, _received) = mpsc::unbounded_channel();
        let port = home_assistant(calls).await;
        let started = started(format!(
            "url = \"http://127.0.0.1:{port}\"\ntoken = \"{TOKEN}\"\n"
        ))
        .await
        .expect("the instance starts");

        started
            .plugin
            .subscribe(&subscriptions(&["light.kitchen.state"]))
            .await
            .expect("subscriptions are accepted");
        await_value(
            &started.variables,
            "light.kitchen.state",
            VariableValue::Text("on".to_string()),
        )
        .await;

        assert_eq!(
            started
                .variables
                .get(&VariableRef::new("hass.home", "sensor.outside_temp.state")),
            None,
            "the installation was mirrored instead of the subscription"
        );
    }

    #[tokio::test]
    async fn a_dropped_connection_is_re_established_and_reseeded() {
        let (calls, _received) = mpsc::unbounded_channel();
        let port = home_assistant_closing(calls, 1).await;
        let mut started = started(format!(
            "url = \"http://127.0.0.1:{port}\"\ntoken = \"{TOKEN}\"\n"
        ))
        .await
        .expect("the instance starts");

        started
            .plugin
            .subscribe(&subscriptions(&["light.kitchen.state"]))
            .await
            .expect("subscriptions are accepted");

        await_log(&mut started.signals, "connected to", 2).await;
        await_value(
            &started.variables,
            "light.kitchen.state",
            VariableValue::Text("on".to_string()),
        )
        .await;
    }

    /// Nothing subscribes here: what an installation recommends is what it can see, not what a
    /// panel happens to be watching.
    #[tokio::test]
    async fn the_seeded_installation_recommends_a_key_for_every_entity_worth_pressing() {
        let (calls, _received) = mpsc::unbounded_channel();
        let port = home_assistant(calls).await;
        let started = started(format!(
            "url = \"http://127.0.0.1:{port}\"\ntoken = \"{TOKEN}\"\n"
        ))
        .await
        .expect("the instance starts");

        let offered = await_presets(&started.presets).await;
        let ids: Vec<_> = offered
            .iter()
            .map(|preset| preset.preset_id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["light.kitchen"],
            "the seed also carried a sensor, which has nothing to press"
        );

        let kitchen = &offered[0];
        assert_eq!(kitchen.name, "Kitchen");
        assert_eq!(
            kitchen
                .control
                .default_state
                .layers
                .iter()
                .find_map(|layer| match layer {
                    crate::panels::rendered_state::Layer::Text { text, .. } => Some(text.as_str()),
                    _ => None,
                }),
            Some("Kitchen"),
            "the generated label does not need the state because its border reports it"
        );

        let Action::InvokeIntegration { integration_id, .. } =
            &kitchen.control.action_bindings[0].actions[0]
        else {
            panic!("a recommended key presses its own instance");
        };
        assert_eq!(integration_id.0, "hass.home");
    }

    /// A reconnect reseeds every entity again, and a browser told the recommendations changed
    /// refetches all of them.
    #[tokio::test]
    async fn a_reseed_of_an_unchanged_installation_does_not_announce_again() {
        let (calls, _received) = mpsc::unbounded_channel();
        let port = home_assistant(calls).await;
        let mut started = started(format!(
            "url = \"http://127.0.0.1:{port}\"\ntoken = \"{TOKEN}\"\n"
        ))
        .await
        .expect("the instance starts");

        await_presets(&started.presets).await;
        started
            .plugin
            .subscribe(&subscriptions(&["light.kitchen.state"]))
            .await
            .expect("subscriptions are accepted");
        await_value(
            &started.variables,
            "light.kitchen.state",
            VariableValue::Text("on".to_string()),
        )
        .await;

        let mut announcements = 0;
        while let Ok(signal) = started.signals.try_recv() {
            if matches!(signal, EngineSignal::PresetsChanged(_)) {
                announcements += 1;
            }
        }
        assert_eq!(announcements, 1, "the subscription forced a second seed");
    }

    #[tokio::test]
    async fn a_service_call_reaches_home_assistant_as_a_targeted_command() {
        let (calls, mut received) = mpsc::unbounded_channel();
        let port = home_assistant(calls).await;
        let started = started(format!(
            "url = \"http://127.0.0.1:{port}\"\ntoken = \"{TOKEN}\"\n"
        ))
        .await
        .expect("the instance starts");

        started
            .plugin
            .subscribe(&subscriptions(&["light.kitchen.state"]))
            .await
            .expect("subscriptions are accepted");
        await_value(
            &started.variables,
            "light.kitchen.state",
            VariableValue::Text("on".to_string()),
        )
        .await;

        assert_eq!(
            started
                .plugin
                .invoke(
                    "light.turn_on",
                    &json!({ "entity_id": "light.kitchen", "brightness_pct": 40 }),
                )
                .await,
            Ok(())
        );

        let call = received.recv().await.expect("the call arrived");
        assert_eq!(call["domain"], json!("light"));
        assert_eq!(call["service"], json!("turn_on"));
        assert_eq!(call["target"]["entity_id"], json!("light.kitchen"));
        assert_eq!(call["service_data"]["brightness_pct"], json!(40.0));
        assert!(call["id"].as_u64().is_some(), "a command needs an id");
    }

    #[tokio::test]
    async fn an_unknown_action_is_reported_rather_than_sent() {
        let (calls, _received) = mpsc::unbounded_channel();
        let port = home_assistant(calls).await;
        let started = started(format!(
            "url = \"http://127.0.0.1:{port}\"\ntoken = \"{TOKEN}\"\n"
        ))
        .await
        .expect("the instance starts");

        assert_eq!(
            started.plugin.invoke("nope", &json!({})).await,
            Err(PluginError::UnknownAction("nope".to_string()))
        );
    }

    #[tokio::test]
    async fn a_rejected_token_is_reported_and_not_retried_forever() {
        let (calls, _received) = mpsc::unbounded_channel();
        let port = home_assistant(calls).await;
        let mut started = started(format!(
            "url = \"http://127.0.0.1:{port}\"\ntoken = \"wrong\"\n"
        ))
        .await
        .expect("a bad token is only discovered once connected");

        let message = await_log(&mut started.signals, "rejected the access token", 1).await;
        assert!(
            !message.contains("wrong"),
            "the token was logged: {message}"
        );

        // A token that was refused will not become valid, and Home Assistant bans a caller that
        // keeps trying, so the retry loop has to end here.
        no_log(
            &mut started.signals,
            "rejected the access token",
            FIRST_RETRY_ALLOWANCE,
        )
        .await;
    }

    #[tokio::test]
    async fn an_unreachable_installation_keeps_the_instance_running_and_says_why() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
        let port = listener.local_addr().expect("has an address").port();
        drop(listener);

        let mut started = started(format!(
            "url = \"http://127.0.0.1:{port}\"\ntoken = \"{TOKEN}\"\n"
        ))
        .await
        .expect("an unreachable installation still starts");

        await_log(&mut started.signals, "is not answering", 1).await;
        assert_eq!(
            started
                .plugin
                .invoke("light.toggle", &json!({ "entity_id": "light.kitchen" }))
                .await,
            Err(PluginError::Upstream(
                "not connected to Home Assistant".to_string()
            ))
        );
    }

    #[tokio::test]
    async fn a_configuration_without_a_url_or_token_is_refused_by_name() {
        assert_eq!(
            started("token = \"hunter2\"\n".to_string())
                .await
                .err()
                .expect("url is required"),
            PluginError::Configuration("url is required".to_string())
        );
        assert_eq!(
            started("url = \"http://127.0.0.1:8123\"\n".to_string())
                .await
                .err()
                .expect("token is required"),
            PluginError::Configuration("token is required".to_string())
        );
    }

    #[tokio::test]
    async fn an_unresolvable_token_is_refused_rather_than_used_empty() {
        let error = started(
            "url = \"http://127.0.0.1:8123\"\ntoken = { env = \"LAUNCHPI_TEST_DEFINITELY_UNSET\" }\n"
                .to_string(),
        )
        .await
        .err()
        .expect("an unset environment variable is a configuration error");
        assert!(
            matches!(error, PluginError::Configuration(reason) if reason.contains("token")),
            "the reason should name the field"
        );
    }

    #[tokio::test]
    async fn an_address_that_is_not_a_url_is_refused() {
        let error = started("url = \"homeassistant.local\"\ntoken = \"hunter2\"\n".to_string())
            .await
            .err()
            .expect("a scheme is required");
        assert!(matches!(error, PluginError::Configuration(_)));
    }

    #[test]
    fn every_declared_action_has_parameters_the_web_form_can_render() {
        for definition in manifest().actions {
            assert!(
                !definition.parameters.is_empty(),
                "{} has no parameters",
                definition.name
            );
        }
        assert!(manifest()
            .config_schema
            .iter()
            .any(|field| field.key == "token" && field.is_secret()));
    }
}
