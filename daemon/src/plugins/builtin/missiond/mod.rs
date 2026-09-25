mod config;
mod connection;
mod discovery;
mod presets;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use crate::plugins::{
    builtin::missiond::{
        config::{MissiondConfig, Target},
        connection::Shared,
        discovery::Discovery,
    },
    instance::InstanceConfig,
    manifest::{ActionDefinition, ConfigField, PluginManifest, VariableDefinition, VariableKind},
    plugin::{LookupOption, Plugin, PluginContext, PluginError, PluginFactory},
};
use crate::surfaces::logs::SurfaceLogLevel;

const ACTIVATE_PLAYLIST: &str = "activate_playlist";
const ACTIVATE_TAB: &str = "activate_tab";
const NEXT_TAB: &str = "next_tab";
const PREVIOUS_TAB: &str = "previous_tab";
const PAUSE: &str = "pause";
const RESUME: &str = "resume";
const TOGGLE_PAUSE: &str = "toggle_pause";
const SLEEP_SCREEN: &str = "sleep_screen";
const WAKE_SCREEN: &str = "wake_screen";
const TOGGLE_SCREEN: &str = "toggle_screen";
const PLAYLIST_LOOKUP: &str = "playlists";
const TAB_LOOKUP: &str = "tabs";
const DISPLAY_LOOKUP: &str = "displays";

pub const FACTORY: PluginFactory = PluginFactory {
    plugin_type: "missiond",
    manifest,
    start: |config, context| Box::pin(start(config, context)),
};

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_type: "missiond",
        display_name: "missiond",
        description: "Watch and control an information display run by missiond.",
        config_schema: vec![
            ConfigField::lookup("device_id", DISPLAY_LOOKUP)
                .label("Display")
                .help(
                    "A display found on the network. Followed to whatever address it announces, \
                     so it keeps working when that address changes.",
                ),
            ConfigField::text("url")
                .label("URL")
                .placeholder("http://display.local:3000")
                .help(
                    "The address you open the missiond web UI at. Takes priority over the \
                     display, and is needed where mDNS does not reach.",
                ),
            ConfigField::secret("key").label("Key").help(
                "The control key, or the admin key, when missiond has one. Without it the display \
                 is shown but cannot be controlled.",
            ),
        ],
        actions: vec![
            ActionDefinition::new(ACTIVATE_PLAYLIST)
                .label("Show playlist")
                .parameters(vec![ConfigField::lookup("playlist_id", PLAYLIST_LOOKUP)
                    .label("Playlist")
                    .required()]),
            ActionDefinition::new(ACTIVATE_TAB)
                .label("Show tab")
                .description(
                    "Holds the tab on screen, in the current playlist when that has the tab.",
                )
                .parameters(vec![ConfigField::lookup("tab_id", TAB_LOOKUP)
                    .label("Tab")
                    .required()]),
            ActionDefinition::new(NEXT_TAB).label("Next tab"),
            ActionDefinition::new(PREVIOUS_TAB).label("Previous tab"),
            ActionDefinition::new(PAUSE)
                .label("Pause rotation")
                .description("The tab on screen stays there."),
            ActionDefinition::new(RESUME).label("Resume rotation"),
            ActionDefinition::new(TOGGLE_PAUSE).label("Pause or resume rotation"),
            ActionDefinition::new(SLEEP_SCREEN).label("Sleep screen"),
            ActionDefinition::new(WAKE_SCREEN).label("Wake screen"),
            ActionDefinition::new(TOGGLE_SCREEN).label("Sleep or wake screen"),
        ],
        variables: vec![
            VariableDefinition::new("connected", VariableKind::Boolean),
            VariableDefinition::new("device_name", VariableKind::Text),
            VariableDefinition::new("brightness", VariableKind::Number)
                .description("Panel brightness from 0 to 100."),
            VariableDefinition::new("playlist_id", VariableKind::Text),
            VariableDefinition::new("playlist_name", VariableKind::Text),
            VariableDefinition::new("tab_id", VariableKind::Text),
            VariableDefinition::new("tab_name", VariableKind::Text),
            VariableDefinition::new("awake", VariableKind::Boolean)
                .description("Whether the screen is on."),
            VariableDefinition::new("screen_state", VariableKind::Text)
                .description("Awake or Asleep."),
            VariableDefinition::new("screen_color", VariableKind::Text)
                .description("Green while the screen is awake, empty while it sleeps."),
            VariableDefinition::new("paused", VariableKind::Boolean)
                .description("Whether tab rotation is paused."),
            VariableDefinition::new("playback_state", VariableKind::Text)
                .description("Rotating or Paused."),
            VariableDefinition::new("playback_color", VariableKind::Text)
                .description("Green while tabs rotate, empty while paused."),
            VariableDefinition::new("seconds_to_next_tab", VariableKind::Number)
                .description("Seconds until rotation next steps, 0 while paused."),
            VariableDefinition::new("next_tab_in", VariableKind::Text)
                .description("The same as 42s or 1:05, empty while paused."),
            VariableDefinition::new("playlists.<playlist_id>.name", VariableKind::Text),
            VariableDefinition::new("playlists.<playlist_id>.active", VariableKind::Boolean),
            VariableDefinition::new("playlists.<playlist_id>.color", VariableKind::Text)
                .description("Green while the playlist is on screen, empty otherwise."),
            VariableDefinition::new("tabs.<tab_id>.name", VariableKind::Text),
            VariableDefinition::new("tabs.<tab_id>.active", VariableKind::Boolean),
            VariableDefinition::new("tabs.<tab_id>.color", VariableKind::Text)
                .description("Green while the tab is on screen, empty otherwise."),
        ],
    }
}

async fn start(
    config: InstanceConfig,
    context: PluginContext,
) -> Result<Arc<dyn Plugin>, PluginError> {
    let settings: MissiondConfig = config.deserialize().map_err(PluginError::Configuration)?;
    let target = settings.target().map_err(PluginError::Configuration)?;
    let key = config
        .secret("key")
        .map_err(PluginError::Configuration)?
        .filter(|key| !key.trim().is_empty());

    let discovery = match target {
        Target::Address(_) => None,
        Target::Discovered(_) | Target::Unchosen => Some(Discovery::start(context.cancel.clone())?),
    };
    if target == Target::Unchosen {
        context.log(
            SurfaceLogLevel::Warning,
            "No display is chosen yet. Pick one found on the network, or enter its URL.",
        );
    }

    let shared = Arc::new(Shared::new(key, context.http.clone()));
    tokio::spawn(connection::run(
        context.clone(),
        shared.clone(),
        target,
        discovery.clone(),
    ));

    Ok(Arc::new(MissiondPlugin {
        shared,
        discovery,
        context,
    }))
}

struct MissiondPlugin {
    shared: Arc<Shared>,
    /// Only while the instance is not pinned to an address.
    discovery: Option<Arc<Discovery>>,
    context: PluginContext,
}

impl MissiondPlugin {
    fn identifier(&self, parameters: &JsonValue, key: &str) -> Result<String, PluginError> {
        let value = parameters
            .get(key)
            .and_then(JsonValue::as_str)
            .map(|template| self.context.interpolate(template))
            .unwrap_or_default();
        let value = value.trim();
        if value.is_empty() {
            return Err(PluginError::Configuration(format!("{key} is required")));
        }
        Ok(value.to_string())
    }
}

#[async_trait]
impl Plugin for MissiondPlugin {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue) -> Result<(), PluginError> {
        match action_name {
            ACTIVATE_PLAYLIST => {
                let playlist_id = self.identifier(parameters, "playlist_id")?;
                self.shared
                    .post(&["playlists", &playlist_id, "activate"])
                    .await
            }
            ACTIVATE_TAB => {
                let tab_id = self.identifier(parameters, "tab_id")?;
                self.shared.post(&["tabs", &tab_id, "activate"]).await
            }
            NEXT_TAB => self.shared.post(&["playback", "next"]).await,
            PREVIOUS_TAB => self.shared.post(&["playback", "previous"]).await,
            PAUSE => self.shared.post(&["playback", "pause"]).await,
            RESUME => self.shared.post(&["playback", "resume"]).await,
            TOGGLE_PAUSE => self.shared.post(&["playback", "toggle"]).await,
            SLEEP_SCREEN => self.shared.post(&["display", "power", "false"]).await,
            WAKE_SCREEN => self.shared.post(&["display", "power", "true"]).await,
            TOGGLE_SCREEN => self.shared.post(&["display", "power", "toggle"]).await,
            _ => Err(PluginError::UnknownAction(action_name.to_string())),
        }
    }

    async fn lookup(&self, source: &str, query: &str) -> Result<Vec<LookupOption>, PluginError> {
        if source == DISPLAY_LOOKUP {
            return Ok(self
                .discovery
                .iter()
                .flat_map(|discovery| discovery.displays())
                .filter(|display| matches(query, &display.name, &display.device_id))
                .map(|display| {
                    LookupOption::new(display.device_id, display.name)
                        .preview(display.api.to_string())
                })
                .collect());
        }
        let view = self.shared.view.lock().unwrap();
        match source {
            PLAYLIST_LOOKUP => Ok(view
                .playlists
                .iter()
                .filter(|playlist| matches(query, &playlist.name, &playlist.playlist_id))
                .map(|playlist| {
                    LookupOption::new(playlist.playlist_id.clone(), playlist.name.clone())
                })
                .collect()),
            TAB_LOOKUP => Ok(view
                .tabs()
                .into_iter()
                .filter(|(_, tab)| matches(query, &tab.name, &tab.tab_id))
                .map(|(playlist, tab)| {
                    LookupOption::new(tab.tab_id.clone(), tab.name.clone())
                        .group(playlist.name.clone())
                })
                .collect()),
            other => Err(PluginError::Configuration(format!(
                "unknown lookup {other}"
            ))),
        }
    }
}

/// Case-insensitive substring match on either the name or the id, so a person finds a playlist
/// by whichever of the two they remember.
fn matches(query: &str, name: &str, id: &str) -> bool {
    let needle = query.trim().to_lowercase();
    needle.is_empty()
        || name.to_lowercase().contains(&needle)
        || id.to_lowercase().contains(&needle)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        sync::{mpsc, Mutex},
    };

    use super::*;
    use crate::{
        bindings::action::Action,
        identifiers::IntegrationId,
        plugins::{plugin::cancellation, preset::PresetStore},
        variables::{VariableRef, VariableStore, VariableValue},
    };

    const KEY: &str = "hunter2";

    /// A request the display was asked to act on: its path and the authorization it carried.
    type Received = (String, Option<String>);

    /// Serves enough of missiond for one instance: an event stream that forwards whatever frames
    /// the test pushes, and a record of every action taken.
    async fn missiond(
        received: mpsc::UnboundedSender<Received>,
        pushes: mpsc::UnboundedReceiver<String>,
    ) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
        let port = listener.local_addr().expect("has an address").port();
        let pushes = Arc::new(Mutex::new(pushes));
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(answer(stream, received.clone(), pushes.clone()));
            }
        });
        port
    }

    async fn answer(
        mut stream: TcpStream,
        received: mpsc::UnboundedSender<Received>,
        pushes: Arc<Mutex<mpsc::UnboundedReceiver<String>>>,
    ) {
        let mut head = Vec::new();
        let mut buffer = [0; 1024];
        while !head.windows(4).any(|window| window == b"\r\n\r\n") {
            match stream.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(read) => head.extend_from_slice(&buffer[..read]),
            }
        }
        let head = String::from_utf8_lossy(&head);
        let mut request_line = head.lines().next().unwrap_or_default().split(' ');
        let method = request_line.next().unwrap_or_default();
        let path = request_line.next().unwrap_or_default();
        let authorization = head.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("authorization")
                .then(|| value.trim().to_string())
        });

        let body = match (method, path) {
            ("GET", "/api/events") => {
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\r\n")
                    .await;
                let mut pushes = pushes.lock().await;
                while let Some(frame) = pushes.recv().await {
                    let frame = format!(": keep-alive\n\n{frame}\n\n");
                    if stream.write_all(frame.as_bytes()).await.is_err() {
                        return;
                    }
                }
                return;
            }
            ("POST", _) => {
                let _ = received.send((path.to_string(), authorization));
                json!({ "persisted": true })
            }
            _ => {
                let _ = stream
                    .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n")
                    .await;
                return;
            }
        };
        let body = body.to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
    }

    fn state_frame(screen_on: bool) -> String {
        let state = json!({
            "device_id": "lobby",
            "device_name": "Lobby Display",
            "current_playlist_id": "lobby",
            "current_tab_id": "overview",
            "auto_rotate": true,
            "next_rotation_at": null,
            "screen_on": screen_on,
            "brightness": 80,
            "requires_auth": true,
            "access": "control",
        });
        format!("event: state\ndata: {state}")
    }

    fn catalogue_frame(playlist_id: &str) -> String {
        let catalogue = json!({
            "playlists": [{
                "playlist_id": playlist_id,
                "name": "Lobby",
                "tabs": [{ "tab_id": "overview", "name": "Overview", "enabled": true }],
            }],
        });
        format!("event: catalogue\ndata: {catalogue}")
    }

    fn offered(presets: &PresetStore, preset_id: &str) -> (String, JsonValue) {
        let preset = presets
            .snapshot()
            .into_iter()
            .flat_map(|(_, offered)| offered)
            .find(|preset| preset.preset_id == preset_id)
            .unwrap_or_else(|| panic!("recommends no {preset_id}"));
        let Action::InvokeIntegration {
            action_name,
            parameters,
            ..
        } = &preset.control.action_bindings[0].actions[0]
        else {
            panic!("{preset_id} does not invoke anything");
        };
        (action_name.clone(), parameters.clone())
    }

    async fn await_value(variables: &VariableStore, name: &str, expected: VariableValue) {
        let reference = VariableRef::new("missiond.lobby", name);
        for _ in 0..250 {
            if variables.get(&reference).as_ref() == Some(&expected) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!(
            "{name} is {:?}, expected {expected:?}",
            variables.get(&reference)
        );
    }

    /// The whole loop a key relies on: both frames reach the values a key shows, a frame moves
    /// them, an edit withdraws what it removed, and pressing a toggle or a recommended key reaches
    /// missiond with the key.
    #[tokio::test]
    async fn follows_the_display_and_drives_it() {
        let (received_sender, mut received) = mpsc::unbounded_channel();
        let (push, pushes) = mpsc::unbounded_channel();
        let port = missiond(received_sender, pushes).await;
        push.send(state_frame(true)).unwrap();
        push.send(catalogue_frame("lobby")).unwrap();

        let variables = Arc::new(VariableStore::default());
        let presets = Arc::new(PresetStore::default());
        let (signals, _signals) = mpsc::channel(256);
        let (_cancel, token) = cancellation();
        let integration_id = IntegrationId("missiond.lobby".to_string());
        let context = PluginContext::new(
            integration_id.clone(),
            variables.clone(),
            presets.clone(),
            signals,
            token,
            reqwest::Client::new(),
        );
        let values = toml::from_str(&format!(
            "url = \"http://127.0.0.1:{port}\"\nkey = \"{KEY}\""
        ))
        .expect("valid toml");
        let plugin = start(
            InstanceConfig {
                integration_id,
                values,
            },
            context,
        )
        .await
        .expect("starts");

        await_value(
            &variables,
            "playlist_name",
            VariableValue::Text("Lobby".to_string()),
        )
        .await;
        await_value(
            &variables,
            "tab_name",
            VariableValue::Text("Overview".to_string()),
        )
        .await;
        await_value(&variables, "awake", VariableValue::Boolean(true)).await;

        push.send(state_frame(false)).unwrap();
        await_value(&variables, "awake", VariableValue::Boolean(false)).await;

        plugin
            .invoke(TOGGLE_SCREEN, &json!({}))
            .await
            .expect("toggles the screen");
        assert_eq!(
            received.recv().await,
            Some((
                "/api/display/power/toggle".to_string(),
                Some(format!("Bearer {KEY}"))
            ))
        );

        for (preset_id, path) in [
            ("playlist:lobby", "/api/playlists/lobby/activate"),
            ("tab:overview", "/api/tabs/overview/activate"),
        ] {
            let (action_name, parameters) = offered(&presets, preset_id);
            plugin
                .invoke(&action_name, &parameters)
                .await
                .expect("the recommended key works");
            assert_eq!(
                received.recv().await.map(|(path, _)| path),
                Some(path.to_string())
            );
        }

        push.send(catalogue_frame("night")).unwrap();
        await_value(
            &variables,
            "playlists.night.name",
            VariableValue::Text("Lobby".to_string()),
        )
        .await;
        assert_eq!(
            variables.get(&VariableRef::new("missiond.lobby", "playlists.lobby.name")),
            None
        );
    }
}
