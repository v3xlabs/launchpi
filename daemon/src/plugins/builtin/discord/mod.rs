mod config;

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value as JsonValue};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    identifiers::AssetId,
    plugins::{
        instance::InstanceConfig,
        manifest::{
            ActionDefinition, ConfigField, PluginManifest, VariableDefinition, VariableKind,
        },
        plugin::{Plugin, PluginContext, PluginError, PluginFactory},
    },
    variables::VariableValue,
};

use self::config::DiscordConfig;

const GUILD_VOICE_STATES: u64 = 1 << 7;
const GUILDS_INTENT: u64 = 1;
const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

pub const FACTORY: PluginFactory = PluginFactory {
    plugin_type: "discord",
    manifest,
    start: |config, context| Box::pin(start(config, context)),
};

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_type: "discord",
        display_name: "Discord",
        description: "Track members in a Discord voice channel and control guild voice state.",
        config_schema: vec![
            ConfigField::secret("token")
                .label("Bot token")
                .required()
                .help("A Discord bot token. Never use a normal user token."),
            ConfigField::text("guild_id").label("Guild ID").required(),
            ConfigField::text("channel_id")
                .label("Fixed voice channel ID")
                .help("Leave blank to follow user_id instead."),
            ConfigField::text("user_id")
                .label("Followed user ID")
                .help("The voice channel containing this user is displayed."),
            ConfigField::number("max_members")
                .label("Members to publish")
                .placeholder("4"),
        ],
        actions: vec![
            ActionDefinition::new("mute_member")
                .label("Mute member")
                .parameters(vec![
                    ConfigField::text("user_id").label("User ID").required(),
                    ConfigField::boolean("mute").label("Muted").required(),
                ]),
            ActionDefinition::new("deafen_member")
                .label("Deafen member")
                .parameters(vec![
                    ConfigField::text("user_id").label("User ID").required(),
                    ConfigField::boolean("deaf").label("Deafened").required(),
                ]),
            ActionDefinition::new("disconnect_member")
                .label("Disconnect member")
                .parameters(vec![ConfigField::text("user_id")
                    .label("User ID")
                    .required()]),
        ],
        variables: vec![
            VariableDefinition::new("channel_id", VariableKind::Text),
            VariableDefinition::new("channel_name", VariableKind::Text),
            VariableDefinition::new("member_count", VariableKind::Number),
            VariableDefinition::new("channel_members_<index>", VariableKind::Text)
                .description("Display name of a member in the selected voice channel."),
            VariableDefinition::new("channel_members_<index>_id", VariableKind::Text)
                .description("Discord user ID of a member in the selected voice channel."),
            VariableDefinition::new("channel_members_<index>_avatar", VariableKind::Text)
                .description("Discord avatar CDN URL for a member in the selected voice channel."),
            VariableDefinition::new("channel_members_<index>_image", VariableKind::Image)
                .description("Cached profile image for a member in the selected voice channel."),
        ],
    }
}

async fn start(
    config: InstanceConfig,
    context: PluginContext,
) -> Result<Arc<dyn Plugin>, PluginError> {
    let settings: DiscordConfig = config.deserialize().map_err(PluginError::Configuration)?;
    settings.validate().map_err(PluginError::Configuration)?;
    let token = config
        .required_secret("token")
        .map_err(PluginError::Configuration)?;
    if token.trim().is_empty() {
        return Err(PluginError::Configuration(
            "token resolved to an empty value".to_string(),
        ));
    }

    let plugin = Arc::new(DiscordPlugin {
        settings,
        token,
        context: context.clone(),
        state: Mutex::new(DiscordState::default()),
        publish_generation: AtomicU64::new(0),
    });
    tokio::spawn(run_gateway(plugin.clone()));
    Ok(plugin)
}

struct DiscordPlugin {
    settings: DiscordConfig,
    token: String,
    context: PluginContext,
    state: Mutex<DiscordState>,
    publish_generation: AtomicU64,
}

#[derive(Default)]
struct DiscordState {
    channels: HashMap<String, String>,
    users: HashMap<String, DiscordUser>,
    voice_states: HashMap<String, VoiceState>,
}

#[derive(Clone, Debug)]
struct DiscordUser {
    id: String,
    name: String,
    avatar: String,
}

#[derive(Clone, Debug)]
struct VoiceState {
    channel_id: Option<String>,
}

#[async_trait]
impl Plugin for DiscordPlugin {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue) -> Result<(), PluginError> {
        let user_id = parameters
            .get("user_id")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| PluginError::Configuration("user_id is required".to_string()))?;
        let user_id = self.context.interpolate(user_id);
        if user_id.is_empty() || !user_id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(PluginError::Configuration(
                "user_id must be a Discord snowflake".to_string(),
            ));
        }
        let guild_id = self.settings.guild_id.as_deref().unwrap_or_default();
        let body = match action_name {
            "mute_member" => json!({
                "mute": parameters.get("mute").and_then(JsonValue::as_bool).ok_or_else(|| {
                    PluginError::Configuration("mute is required".to_string())
                })?
            }),
            "deafen_member" => json!({
                "deaf": parameters.get("deaf").and_then(JsonValue::as_bool).ok_or_else(|| {
                    PluginError::Configuration("deaf is required".to_string())
                })?
            }),
            "disconnect_member" => json!({"channel_id": null}),
            _ => return Err(PluginError::UnknownAction(action_name.to_string())),
        };
        let response = self
            .context
            .http
            .patch(format!(
                "https://discord.com/api/v10/guilds/{guild_id}/members/{user_id}"
            ))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bot {}", self.token),
            )
            .json(&body)
            .send()
            .await
            .map_err(|error| PluginError::Upstream(format!("Discord request failed: {error}")))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(PluginError::Upstream(format!(
                "Discord returned {} for {action_name}",
                response.status()
            )))
        }
    }
}

async fn run_gateway(plugin: Arc<DiscordPlugin>) {
    let mut delay = Duration::from_secs(1);
    loop {
        if plugin.context.cancel.is_cancelled() {
            return;
        }
        match connect_and_read(plugin.clone()).await {
            Ok(()) => delay = Duration::from_secs(1),
            Err(error) => {
                plugin.context.log(
                    crate::surfaces::logs::SurfaceLogLevel::Warning,
                    format!("Discord Gateway stopped: {error}"),
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(MAX_RECONNECT_DELAY);
            }
        }
    }
}

async fn connect_and_read(plugin: Arc<DiscordPlugin>) -> Result<(), String> {
    let (mut socket, _) = connect_async(GATEWAY_URL)
        .await
        .map_err(|error| format!("unable to connect: {error}"))?;
    let hello = socket
        .next()
        .await
        .ok_or_else(|| "Gateway closed before Hello".to_string())?
        .map_err(|error| error.to_string())?;
    let hello = parse_message(hello)?;
    let heartbeat_ms = hello
        .get("d")
        .and_then(|value| value.get("heartbeat_interval"))
        .and_then(JsonValue::as_u64)
        .ok_or_else(|| "Gateway Hello did not contain heartbeat_interval".to_string())?;
    socket
        .send(Message::Text(
            json!({
                "op": 2,
                "d": {
                "token": plugin.token.clone(),
                    "intents": GUILDS_INTENT | GUILD_VOICE_STATES,
                    "properties": {"os": "linux", "browser": "launchpi", "device": "launchpi"}
                }
            })
            .to_string(),
        ))
        .await
        .map_err(|error| error.to_string())?;

    let mut heartbeat = tokio::time::interval(Duration::from_millis(heartbeat_ms));
    loop {
        tokio::select! {
            _ = plugin.context.cancel.cancelled() => return Ok(()),
            _ = heartbeat.tick() => {
                socket.send(Message::Text(json!({"op": 1, "d": null}).to_string()))
                    .await.map_err(|error| error.to_string())?;
            }
            message = socket.next() => {
                let Some(message) = message else { return Err("Gateway connection closed".to_string()); };
                let message = parse_message(message.map_err(|error| error.to_string())?)?;
                match message.get("op").and_then(JsonValue::as_u64) {
                    Some(0) => handle_event(&plugin, &message),
                    Some(1) => socket.send(Message::Text(json!({"op": 1, "d": null}).to_string()))
                        .await.map_err(|error| error.to_string())?,
                    Some(7) => return Err("Gateway requested reconnect".to_string()),
                    Some(9) => return Err("Gateway rejected the session".to_string()),
                    _ => {}
                }
            }
        }
    }
}

fn parse_message(message: Message) -> Result<JsonValue, String> {
    match message {
        Message::Text(text) => serde_json::from_str(&text).map_err(|error| error.to_string()),
        Message::Close(_) => Err("Gateway closed the connection".to_string()),
        _ => Err("Gateway sent an unsupported frame".to_string()),
    }
}

fn handle_event(plugin: &Arc<DiscordPlugin>, message: &JsonValue) {
    match message.get("t").and_then(JsonValue::as_str) {
        Some("GUILD_CREATE") => {
            handle_guild_create(plugin, message.get("d").unwrap_or(&JsonValue::Null))
        }
        Some("VOICE_STATE_UPDATE") => {
            handle_voice_state_update(plugin, message.get("d").unwrap_or(&JsonValue::Null))
        }
        _ => {}
    }
}

fn handle_guild_create(plugin: &Arc<DiscordPlugin>, data: &JsonValue) {
    if data.get("id").and_then(JsonValue::as_str) != plugin.settings.guild_id.as_deref() {
        return;
    }
    let mut state = plugin.state.lock().unwrap();
    if let Some(channels) = data.get("channels").and_then(JsonValue::as_array) {
        for channel in channels {
            if let (Some(id), Some(name)) = (
                channel.get("id").and_then(JsonValue::as_str),
                channel.get("name").and_then(JsonValue::as_str),
            ) {
                state.channels.insert(id.to_string(), name.to_string());
            }
        }
    }
    if let Some(members) = data.get("members").and_then(JsonValue::as_array) {
        for member in members {
            update_member(&mut state, member);
        }
    }
    if let Some(voice_states) = data.get("voice_states").and_then(JsonValue::as_array) {
        for voice_state in voice_states {
            update_state(&mut state, voice_state);
        }
    }
    drop(state);
    publish(plugin);
    let plugin = Arc::clone(plugin);
    tokio::spawn(async move { hydrate_missing_members(plugin).await });
}

fn handle_voice_state_update(plugin: &Arc<DiscordPlugin>, data: &JsonValue) {
    if data.get("guild_id").and_then(JsonValue::as_str) != plugin.settings.guild_id.as_deref() {
        return;
    }
    let mut state = plugin.state.lock().unwrap();
    update_state(&mut state, data);
    drop(state);
    publish(plugin);
}

fn update_state(state: &mut DiscordState, data: &JsonValue) {
    let Some(user_id) = data.get("user_id").and_then(JsonValue::as_str) else {
        return;
    };
    if let Some(member) = data.get("member") {
        update_member(state, member);
    }
    let channel_id = data
        .get("channel_id")
        .and_then(JsonValue::as_str)
        .map(str::to_string);
    if channel_id.is_none() {
        state.voice_states.remove(user_id);
    } else {
        state
            .voice_states
            .insert(user_id.to_string(), VoiceState { channel_id });
    }
}

fn update_member(state: &mut DiscordState, member: &JsonValue) {
    let Some(user) = member.get("user") else {
        return;
    };
    let Some(user_id) = user.get("id").and_then(JsonValue::as_str) else {
        return;
    };
    state
        .users
        .insert(user_id.to_string(), user_from_json(user_id, member, user));
}

async fn hydrate_missing_members(plugin: Arc<DiscordPlugin>) {
    let missing_ids = {
        let state = plugin.state.lock().unwrap();
        let channel_id = selected_channel_id(&plugin.settings, &state);
        state
            .voice_states
            .iter()
            .filter(|(_, voice_state)| voice_state.channel_id == channel_id)
            .filter_map(|(user_id, _)| {
                (!state.users.contains_key(user_id)).then_some(user_id.clone())
            })
            .collect::<Vec<_>>()
    };

    for user_id in missing_ids {
        let guild_id = plugin.settings.guild_id.as_deref().unwrap_or_default();
        let response = match plugin
            .context
            .http
            .get(format!(
                "https://discord.com/api/v10/guilds/{guild_id}/members/{user_id}"
            ))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bot {}", plugin.token),
            )
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                response.json::<JsonValue>().await.ok()
            }
            _ => None,
        };
        if let Some(member) = response {
            update_member_if_present(&plugin, &member, &user_id);
        }
    }
    publish(&plugin);
}

fn update_member_if_present(plugin: &DiscordPlugin, member: &JsonValue, user_id: &str) {
    let Some(user) = member.get("user") else {
        return;
    };
    let mut state = plugin.state.lock().unwrap();
    if state.voice_states.contains_key(user_id) {
        state
            .users
            .insert(user_id.to_string(), user_from_json(user_id, member, user));
    }
}

fn selected_channel_id(settings: &DiscordConfig, state: &DiscordState) -> Option<String> {
    settings.channel_id.clone().or_else(|| {
        settings
            .user_id
            .as_ref()
            .and_then(|user_id| state.voice_states.get(user_id))
            .and_then(|voice_state| voice_state.channel_id.clone())
    })
}

fn user_from_json(id: &str, member: &JsonValue, user: &JsonValue) -> DiscordUser {
    let name = member
        .get("nick")
        .and_then(JsonValue::as_str)
        .or_else(|| user.get("global_name").and_then(JsonValue::as_str))
        .or_else(|| user.get("username").and_then(JsonValue::as_str))
        .unwrap_or(id)
        .to_string();
    let avatar = user
        .get("avatar")
        .and_then(JsonValue::as_str)
        .map(|hash| format!("https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=128"))
        .unwrap_or_else(|| {
            format!(
                "https://cdn.discordapp.com/embed/avatars/{}.png",
                avatar_index(id)
            )
        });
    DiscordUser {
        id: id.to_string(),
        name,
        avatar,
    }
}

fn avatar_index(id: &str) -> u64 {
    id.parse::<u64>().unwrap_or_default() % 6
}

fn publish(plugin: &Arc<DiscordPlugin>) {
    let generation = plugin.publish_generation.fetch_add(1, Ordering::Relaxed) + 1;
    let state = plugin.state.lock().unwrap();
    let channel_id = selected_channel_id(&plugin.settings, &state);
    let mut members: Vec<_> = state
        .voice_states
        .iter()
        .filter(|(_, voice_state)| voice_state.channel_id == channel_id)
        .filter_map(|(user_id, _)| state.users.get(user_id))
        .cloned()
        .collect();
    members.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then(left.id.cmp(&right.id))
    });
    let channel_name = channel_id
        .as_ref()
        .and_then(|id| state.channels.get(id))
        .cloned()
        .unwrap_or_default();
    drop(state);

    plugin.context.set_value(
        "channel_id",
        VariableValue::Text(channel_id.unwrap_or_default()),
    );
    plugin
        .context
        .set_value("channel_name", VariableValue::Text(channel_name));
    plugin
        .context
        .set_value("member_count", VariableValue::Number(members.len() as f64));
    for index in 0..plugin.settings.max_members {
        let prefix = format!("channel_members_{index}");
        if let Some(member) = members.get(index) {
            plugin
                .context
                .set_value(prefix.clone(), VariableValue::Text(member.name.clone()));
            plugin.context.set_value(
                format!("{prefix}_id"),
                VariableValue::Text(member.id.clone()),
            );
            plugin.context.set_value(
                format!("{prefix}_avatar"),
                VariableValue::Text(member.avatar.clone()),
            );
            plugin.context.set_value(
                format!("{prefix}_image"),
                VariableValue::Image(AssetId("builtin:none".to_string())),
            );
            let context = plugin.context.clone();
            let plugin = Arc::clone(plugin);
            let image_name = format!("{prefix}_image");
            let avatar_url = member.avatar.clone();
            tokio::spawn(async move {
                let image = match context.http.get(&avatar_url).send().await {
                    Ok(response) if response.status().is_success() => response.bytes().await.ok(),
                    _ => None,
                };
                if plugin.publish_generation.load(Ordering::Relaxed) != generation {
                    return;
                }
                match image {
                    Some(bytes) => {
                        if let Err(error) = context.set_image(image_name, &bytes) {
                            context.log(
                                crate::surfaces::logs::SurfaceLogLevel::Warning,
                                format!("Unable to cache Discord avatar: {error}"),
                            );
                        }
                    }
                    None => context.set_value(
                        image_name,
                        VariableValue::Image(AssetId("builtin:none".to_string())),
                    ),
                }
            });
        } else {
            plugin
                .context
                .set_value(prefix.clone(), VariableValue::Text(String::new()));
            plugin.context.set_value(
                format!("{prefix}_image"),
                VariableValue::Image(AssetId("builtin:none".to_string())),
            );
            plugin
                .context
                .set_value(format!("{prefix}_id"), VariableValue::Text(String::new()));
            plugin.context.set_value(
                format!("{prefix}_avatar"),
                VariableValue::Text(String::new()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_avatar_index_uses_the_discord_user_id() {
        assert_eq!(avatar_index("7"), 1);
        assert_eq!(avatar_index("12"), 0);
    }

    #[test]
    fn member_updates_remove_users_that_leave_voice() {
        let mut state = DiscordState::default();
        update_state(
            &mut state,
            &json!({
                "user_id": "1",
                "channel_id": "2",
                "member": {"user": {"username": "Ada", "avatar": null}}
            }),
        );
        assert!(state.voice_states.contains_key("1"));
        update_state(&mut state, &json!({"user_id": "1", "channel_id": null}));
        assert!(!state.voice_states.contains_key("1"));
    }
}
