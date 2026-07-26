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
    bindings::action::{Action, ActionBinding, ActionTrigger},
    identifiers::{AssetId, IntegrationId},
    panels::{
        control::ControlTemplate,
        rendered_state::{Anchor9, ColorBinding, Fit, Layer, RenderedState, RgbaColor},
    },
    plugins::{
        instance::InstanceConfig,
        manifest::{
            ActionDefinition, ConfigField, PluginManifest, VariableDefinition, VariableKind,
        },
        plugin::{LookupOption, Plugin, PluginContext, PluginError, PluginFactory},
        preset::Preset,
    },
    variables::VariableValue,
};

use self::config::DiscordConfig;

const GUILD_VOICE_STATES: u64 = 1 << 7;
const GUILDS_INTENT: u64 = 1;
const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);
const MUTED_BADGE: &[u8] = include_bytes!("discord-muted.png");
const DEAFENED_BADGE: &[u8] = include_bytes!("discord-deafened.png");
const MUTED_COLOR: &str = "#ed4245";
const GUILD_LOOKUP: &str = "guilds";
const CHANNEL_LOOKUP: &str = "voice_channels";
const MEMBER_LOOKUP: &str = "members";

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
            ConfigField::lookup("guild_id", GUILD_LOOKUP)
                .label("Guild")
                .required(),
            ConfigField::lookup("channel_id", CHANNEL_LOOKUP)
                .label("Fallback voice channel")
                .help("Shown when the followed user is not in voice. Required on its own if no user is followed."),
            ConfigField::lookup("user_id", MEMBER_LOOKUP)
                .label("Followed user")
                .help("The voice channel containing this user takes priority over the fallback channel."),
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
            VariableDefinition::new("channel_source", VariableKind::Text)
                .description("Which setting chose the channel: followed, fallback or none."),
            VariableDefinition::new("member_count", VariableKind::Number),
            VariableDefinition::new("channel_members_<index>", VariableKind::Text)
                .description("Display name of a member in the selected voice channel."),
            VariableDefinition::new("channel_members_<index>_id", VariableKind::Text)
                .description("Discord user ID of a member in the selected voice channel."),
            VariableDefinition::new("channel_members_<index>_avatar", VariableKind::Text)
                .description("Discord avatar CDN URL for a member in the selected voice channel."),
            VariableDefinition::new("channel_members_<index>_image", VariableKind::Image)
                .description("Cached profile image for a member in the selected voice channel."),
            VariableDefinition::new("channel_members_<index>_muted", VariableKind::Boolean)
                .description("Whether the member's microphone is muted, by themselves or by the server."),
            VariableDefinition::new("channel_members_<index>_deafened", VariableKind::Boolean)
                .description("Whether the member is deafened, by themselves or by the server."),
            VariableDefinition::new("channel_members_<index>_server_muted", VariableKind::Boolean)
                .description("Whether the member was muted by the server rather than by themselves."),
            VariableDefinition::new("channel_members_<index>_streaming", VariableKind::Boolean)
                .description("Whether the member is sharing their screen."),
            VariableDefinition::new("channel_members_<index>_video", VariableKind::Boolean)
                .description("Whether the member's camera is on."),
            VariableDefinition::new("channel_members_<index>_status_icon", VariableKind::Image)
                .description("Mute or deafen badge for a member, for a control's overlay image."),
            VariableDefinition::new("channel_members_<index>_status_color", VariableKind::Text)
                .description("Border colour for a member's status, empty when there is nothing to show."),
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

    if !settings.names_a_channel() {
        context.log(
            crate::surfaces::logs::SurfaceLogLevel::Warning,
            "Discord is connected but no guild and channel are configured yet, so no members are published.",
        );
    }
    context.set_presets(presets(&settings));
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
    /// Every guild the bot is in, not only the configured one, so the guild picker has something to
    /// offer before a guild has been chosen.
    guilds: HashMap<String, String>,
    channels: HashMap<String, DiscordChannel>,
    users: HashMap<String, DiscordUser>,
    voice_states: HashMap<String, VoiceState>,
}

#[derive(Clone, Debug)]
struct DiscordChannel {
    name: String,
    kind: u64,
}

impl DiscordChannel {
    const GUILD_VOICE: u64 = 2;
    const GUILD_STAGE: u64 = 13;

    fn is_voice(&self) -> bool {
        self.kind == Self::GUILD_VOICE || self.kind == Self::GUILD_STAGE
    }
}

#[derive(Clone, Debug)]
struct DiscordUser {
    id: String,
    name: String,
    avatar: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct VoiceState {
    channel_id: Option<String>,
    is_server_muted: bool,
    is_server_deafened: bool,
    is_self_muted: bool,
    is_self_deafened: bool,
    is_streaming: bool,
    is_video_on: bool,
}

impl VoiceState {
    /// A server mute and a self mute are the same fact to a key showing whether a mic is live.
    fn is_muted(&self) -> bool {
        self.is_self_muted || self.is_server_muted
    }

    fn is_deafened(&self) -> bool {
        self.is_self_deafened || self.is_server_deafened
    }
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

    async fn lookup(&self, source: &str, query: &str) -> Result<Vec<LookupOption>, PluginError> {
        let state = self.state.lock().unwrap();
        match source {
            GUILD_LOOKUP => Ok(guild_options(&state, query)),
            CHANNEL_LOOKUP => Ok(channel_options(&state, query)),
            MEMBER_LOOKUP => Ok(member_options(&state, query)),
            other => Err(PluginError::Configuration(format!(
                "unknown lookup {other}"
            ))),
        }
    }
}

/// One ready-made button per member slot, plus the two channel readouts. Published once at start
/// rather than on every voice event: the slots are what `max_members` says, and naming a preset
/// after whoever currently occupies one would rewrite the picker every time somebody spoke.
fn presets(settings: &DiscordConfig) -> Vec<Preset> {
    let mut presets = vec![
        channel_preset("channel-name", "Channel name", "$(self:channel_name)"),
        channel_preset("member-count", "Members in channel", "$(self:member_count)"),
    ];
    for index in 0..settings.max_members {
        let slot = index + 1;
        presets.push(Preset {
            preset_id: format!("member-{index}"),
            category: "Members".to_string(),
            name: format!("Member {slot}"),
            description: Some(
                "Avatar, name, and a badge and outline for muted or deafened.".to_string(),
            ),
            control: ControlTemplate {
                name: format!("Member {slot}"),
                default_state: RenderedState {
                    layers: vec![
                        Layer::Fill {
                            color: RgbaColor::opaque(30, 41, 59).into(),
                        },
                        Layer::Image {
                            image: AssetId(format!("$(self:channel_members_{index}_avatar)")),
                            fit: Fit::Cover,
                            anchor: Anchor9::Center,
                            scale_percent: 100,
                            tint: None,
                        },
                        // An avatar is arbitrary, so a name over it can land on anything.
                        Layer::Fill {
                            color: RgbaColor {
                                red: 0,
                                green: 0,
                                blue: 0,
                                alpha: 140,
                            }
                            .into(),
                        },
                        Layer::Text {
                            text: format!("$(self:channel_members_{index})"),
                            color: RgbaColor::opaque(255, 255, 255).into(),
                            anchor: Anchor9::BottomStart,
                        },
                        Layer::Image {
                            image: AssetId(format!("$(self:channel_members_{index}_status_icon)")),
                            fit: Fit::Contain,
                            anchor: Anchor9::BottomEnd,
                            scale_percent: 32,
                            tint: None,
                        },
                        Layer::Border {
                            color: ColorBinding::Reference(format!(
                                "$(self:channel_members_{index}_status_color)"
                            )),
                            width: 5,
                        },
                    ],
                    is_pressed: false,
                },
                pressed_state: None,
                action_bindings: Vec::new(),
            },
        });
        presets.push(Preset {
            preset_id: format!("mute-member-{index}"),
            category: "Member actions".to_string(),
            name: format!("Server-mute member {slot}"),
            description: None,
            control: ControlTemplate {
                name: format!("Mute member {slot}"),
                default_state: RenderedState::labelled(
                    format!("Mute\n$(self:channel_members_{index})"),
                    RgbaColor::opaque(255, 255, 255),
                    RgbaColor::opaque(30, 41, 59),
                    false,
                ),
                pressed_state: None,
                action_bindings: vec![ActionBinding {
                    gesture: ActionTrigger::Press,
                    actions: vec![Action::InvokeIntegration {
                        integration_id: IntegrationId("self".to_string()),
                        action_name: "mute_member".to_string(),
                        parameters: json!({
                            "user_id": format!("$(self:channel_members_{index}_id)"),
                            "mute": true
                        }),
                    }],
                }],
            },
        });
    }
    presets
}

fn channel_preset(preset_id: &str, name: &str, text: &str) -> Preset {
    Preset {
        preset_id: preset_id.to_string(),
        category: "Channel".to_string(),
        name: name.to_string(),
        description: None,
        control: ControlTemplate {
            name: name.to_string(),
            default_state: RenderedState::labelled(
                text,
                RgbaColor::opaque(255, 255, 255),
                RgbaColor::opaque(30, 41, 59),
                false,
            ),
            pressed_state: None,
            action_bindings: Vec::new(),
        },
    }
}

/// Case-insensitive substring match on either half of an option, so a snowflake pasted from Discord
/// finds its own entry just as a typed name does.
fn matches(query: &str, label: &str, value: &str) -> bool {
    let needle = query.trim().to_lowercase();
    needle.is_empty()
        || label.to_lowercase().contains(&needle)
        || value.to_lowercase().contains(&needle)
}

fn sorted(mut options: Vec<LookupOption>) -> Vec<LookupOption> {
    options.sort_by(|left, right| {
        left.label
            .to_lowercase()
            .cmp(&right.label.to_lowercase())
            .then(left.value.cmp(&right.value))
    });
    options
}

fn guild_options(state: &DiscordState, query: &str) -> Vec<LookupOption> {
    sorted(
        state
            .guilds
            .iter()
            .filter(|(id, name)| matches(query, name, id))
            .map(|(id, name)| LookupOption::new(id.clone(), name.clone()))
            .collect(),
    )
}

fn channel_options(state: &DiscordState, query: &str) -> Vec<LookupOption> {
    sorted(
        state
            .channels
            .iter()
            .filter(|(_, channel)| channel.is_voice())
            .filter(|(id, channel)| matches(query, &channel.name, id))
            .map(|(id, channel)| {
                LookupOption::new(id.clone(), channel.name.clone()).group(
                    if channel.kind == DiscordChannel::GUILD_STAGE {
                        "Stage"
                    } else {
                        "Voice"
                    },
                )
            })
            .collect(),
    )
}

fn member_options(state: &DiscordState, query: &str) -> Vec<LookupOption> {
    sorted(
        state
            .users
            .values()
            .filter(|user| matches(query, &user.name, &user.id))
            .map(|user| LookupOption::new(user.id.clone(), user.name.clone()))
            .collect(),
    )
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
    let mut state = plugin.state.lock().unwrap();
    if !update_guild(&mut state, data, plugin.settings.guild_id.as_deref()) {
        return;
    }
    drop(state);
    publish(plugin);
    let plugin = Arc::clone(plugin);
    tokio::spawn(async move { hydrate_missing_members(plugin).await });
}

/// Answers whether this was the configured guild, and so whether anything worth publishing changed.
fn update_guild(state: &mut DiscordState, data: &JsonValue, configured: Option<&str>) -> bool {
    let Some(guild_id) = data.get("id").and_then(JsonValue::as_str) else {
        return false;
    };
    // Recorded before the guild filter, so an instance that has not chosen a guild yet can still
    // offer the ones its bot can see.
    if let Some(name) = data.get("name").and_then(JsonValue::as_str) {
        state.guilds.insert(guild_id.to_string(), name.to_string());
    }
    if Some(guild_id) != configured {
        return false;
    }
    if let Some(channels) = data.get("channels").and_then(JsonValue::as_array) {
        for channel in channels {
            if let (Some(id), Some(name)) = (
                channel.get("id").and_then(JsonValue::as_str),
                channel.get("name").and_then(JsonValue::as_str),
            ) {
                state.channels.insert(
                    id.to_string(),
                    DiscordChannel {
                        name: name.to_string(),
                        kind: channel.get("type").and_then(JsonValue::as_u64).unwrap_or(0),
                    },
                );
            }
        }
    }
    if let Some(members) = data.get("members").and_then(JsonValue::as_array) {
        for member in members {
            update_member(state, member);
        }
    }
    if let Some(voice_states) = data.get("voice_states").and_then(JsonValue::as_array) {
        for voice_state in voice_states {
            update_state(state, voice_state);
        }
    }
    true
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
        return;
    }
    let flag = |name: &str| data.get(name).and_then(JsonValue::as_bool).unwrap_or(false);
    state.voice_states.insert(
        user_id.to_string(),
        VoiceState {
            channel_id,
            is_server_muted: flag("mute"),
            is_server_deafened: flag("deaf"),
            is_self_muted: flag("self_mute"),
            is_self_deafened: flag("self_deaf"),
            is_streaming: flag("self_stream"),
            is_video_on: flag("self_video"),
        },
    );
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

/// Which channel the followed user is in, falling back to the fixed channel when they are not in
/// voice or not visible to the bot. `update_state` drops users whose channel is null, so leaving
/// voice and never having joined are the same lookup miss here.
fn selected_channel(
    settings: &DiscordConfig,
    state: &DiscordState,
) -> (Option<String>, &'static str) {
    let followed = settings
        .user_id
        .as_ref()
        .and_then(|user_id| state.voice_states.get(user_id))
        .and_then(|voice_state| voice_state.channel_id.clone());
    match followed {
        Some(channel_id) => (Some(channel_id), "followed"),
        None => match settings.channel_id.clone() {
            Some(channel_id) => (Some(channel_id), "fallback"),
            None => (None, "none"),
        },
    }
}

fn selected_channel_id(settings: &DiscordConfig, state: &DiscordState) -> Option<String> {
    selected_channel(settings, state).0
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
    let (channel_id, channel_source) = selected_channel(&plugin.settings, &state);
    let mut members: Vec<(DiscordUser, VoiceState)> = state
        .voice_states
        .iter()
        .filter(|(_, voice_state)| voice_state.channel_id == channel_id)
        .filter_map(|(user_id, voice_state)| {
            state
                .users
                .get(user_id)
                .map(|user| (user.clone(), voice_state.clone()))
        })
        .collect();
    members.sort_by(|(left, _), (right, _)| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then(left.id.cmp(&right.id))
    });
    let channel_name = channel_id
        .as_ref()
        .and_then(|id| state.channels.get(id))
        .map(|channel| channel.name.clone())
        .unwrap_or_default();
    drop(state);

    plugin.context.set_value(
        "channel_id",
        VariableValue::Text(channel_id.unwrap_or_default()),
    );
    plugin
        .context
        .set_value("channel_name", VariableValue::Text(channel_name));
    plugin.context.set_value(
        "channel_source",
        VariableValue::Text(channel_source.to_string()),
    );
    plugin
        .context
        .set_value("member_count", VariableValue::Number(members.len() as f64));
    for index in 0..plugin.settings.max_members {
        let prefix = format!("channel_members_{index}");
        if let Some((member, voice)) = members.get(index) {
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
            publish_voice_state(plugin, &prefix, voice);
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
            publish_voice_state(plugin, &prefix, &VoiceState::default());
        }
    }
}

/// The badge bytes are stored synchronously, so a status change repaints on the next flush rather
/// than waiting for an asset to arrive the way an avatar does.
fn publish_voice_state(plugin: &Arc<DiscordPlugin>, prefix: &str, voice: &VoiceState) {
    let context = &plugin.context;
    context.set_value(
        format!("{prefix}_muted"),
        VariableValue::Boolean(voice.is_muted()),
    );
    context.set_value(
        format!("{prefix}_deafened"),
        VariableValue::Boolean(voice.is_deafened()),
    );
    context.set_value(
        format!("{prefix}_server_muted"),
        VariableValue::Boolean(voice.is_server_muted),
    );
    context.set_value(
        format!("{prefix}_streaming"),
        VariableValue::Boolean(voice.is_streaming),
    );
    context.set_value(
        format!("{prefix}_video"),
        VariableValue::Boolean(voice.is_video_on),
    );
    context.set_value(
        format!("{prefix}_status_color"),
        VariableValue::Text(status_color(voice).to_string()),
    );

    let icon_name = format!("{prefix}_status_icon");
    let badge = match (voice.is_deafened(), voice.is_muted()) {
        (true, _) => Some(DEAFENED_BADGE),
        (_, true) => Some(MUTED_BADGE),
        _ => None,
    };
    match badge {
        Some(bytes) => {
            if let Err(error) = context.set_image(icon_name.clone(), bytes) {
                context.log(
                    crate::surfaces::logs::SurfaceLogLevel::Warning,
                    format!("Unable to store the Discord status badge: {error}"),
                );
                context.set_value(icon_name, VariableValue::Image(AssetId(String::new())));
            }
        }
        None => context.set_value(icon_name, VariableValue::Image(AssetId(String::new()))),
    }
}

/// Empty rather than a neutral colour: an unresolvable colour leaves a key unbordered, which is how
/// "nothing to report" is spelled in the render path.
fn status_color(voice: &VoiceState) -> &'static str {
    if voice.is_deafened() || voice.is_muted() {
        MUTED_COLOR
    } else {
        ""
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

    fn settings(channel_id: Option<&str>, user_id: Option<&str>) -> DiscordConfig {
        DiscordConfig {
            token: None,
            guild_id: Some("1".to_string()),
            channel_id: channel_id.map(str::to_string),
            user_id: user_id.map(str::to_string),
            max_members: 4,
        }
    }

    #[test]
    fn the_followed_user_wins_and_the_fixed_channel_is_the_fallback() {
        let settings = settings(Some("100"), Some("7"));
        let mut state = DiscordState::default();
        assert_eq!(
            selected_channel(&settings, &state),
            (Some("100".to_string()), "fallback")
        );

        update_state(&mut state, &json!({"user_id": "7", "channel_id": "200"}));
        assert_eq!(
            selected_channel(&settings, &state),
            (Some("200".to_string()), "followed")
        );

        update_state(&mut state, &json!({"user_id": "7", "channel_id": null}));
        assert_eq!(
            selected_channel(&settings, &state),
            (Some("100".to_string()), "fallback")
        );
    }

    #[test]
    fn a_followed_user_with_no_fallback_selects_nothing_while_they_are_out_of_voice() {
        assert_eq!(
            selected_channel(&settings(None, Some("7")), &DiscordState::default()),
            (None, "none")
        );
    }

    #[test]
    fn a_voice_state_keeps_the_mute_and_deafen_flags() {
        let mut state = DiscordState::default();
        update_state(
            &mut state,
            &json!({
                "user_id": "1",
                "channel_id": "2",
                "self_mute": true,
                "self_deaf": true,
                "self_stream": true,
                "self_video": true
            }),
        );
        let voice = state.voice_states.get("1").expect("state was recorded");
        assert!(voice.is_muted() && voice.is_deafened());
        assert!(voice.is_streaming && voice.is_video_on);
        assert!(!voice.is_server_muted, "a self mute is not a server mute");
    }

    #[test]
    fn a_server_mute_and_a_self_mute_read_the_same_to_a_key() {
        let mut state = DiscordState::default();
        update_state(
            &mut state,
            &json!({"user_id": "1", "channel_id": "2", "mute": true, "deaf": true}),
        );
        let voice = state.voice_states.get("1").expect("state was recorded");
        assert!(voice.is_muted() && voice.is_deafened());
        assert_eq!(status_color(voice), MUTED_COLOR);
    }

    #[test]
    fn an_unmuted_member_has_no_status_colour_so_the_key_stays_unbordered() {
        assert_eq!(status_color(&VoiceState::default()), "");
    }

    #[test]
    fn a_guild_the_bot_is_in_can_be_picked_before_one_is_configured() {
        let mut state = DiscordState::default();
        let changed = update_guild(
            &mut state,
            &json!({"id": "900", "name": "Elsewhere", "channels": []}),
            None,
        );

        assert!(!changed, "an unconfigured guild publishes nothing");
        assert_eq!(
            guild_options(&state, "").len(),
            1,
            "but it is still offered"
        );
        assert!(state.channels.is_empty());
    }

    #[test]
    fn only_voice_and_stage_channels_are_offered() {
        let mut state = DiscordState::default();
        update_guild(
            &mut state,
            &json!({
                "id": "1",
                "name": "Home",
                "channels": [
                    {"id": "10", "name": "general", "type": 0},
                    {"id": "11", "name": "Lounge", "type": 2},
                    {"id": "12", "name": "Announcements", "type": 5},
                    {"id": "13", "name": "Main Stage", "type": 13}
                ]
            }),
            Some("1"),
        );

        let offered: Vec<_> = channel_options(&state, "")
            .into_iter()
            .map(|option| option.label)
            .collect();
        assert_eq!(offered, vec!["Lounge", "Main Stage"]);
    }

    #[test]
    fn a_member_preset_is_offered_for_every_slot_and_binds_only_its_own_index() {
        let mut settings = settings(Some("100"), None);
        settings.max_members = 2;
        let offered = presets(&settings);

        let ids: Vec<_> = offered
            .iter()
            .map(|preset| preset.preset_id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "channel-name",
                "member-count",
                "member-0",
                "mute-member-0",
                "member-1",
                "mute-member-1"
            ]
        );

        let second = offered
            .iter()
            .find(|preset| preset.preset_id == "member-1")
            .expect("the second slot is offered");
        // Every reference in the stack names slot 1 and nothing else; the sigil is rewritten to
        // the publishing instance on the way into the store.
        let layers = &second.control.default_state.layers;
        assert!(layers.contains(&Layer::Text {
            text: "$(self:channel_members_1)".to_string(),
            color: RgbaColor::opaque(255, 255, 255).into(),
            anchor: Anchor9::BottomStart,
        }));
        assert!(layers.contains(&Layer::Border {
            color: ColorBinding::Reference("$(self:channel_members_1_status_color)".to_string()),
            width: 5,
        }));
        assert!(layers.contains(&Layer::Image {
            image: AssetId("$(self:channel_members_1_status_icon)".to_string()),
            fit: Fit::Contain,
            anchor: Anchor9::BottomEnd,
            scale_percent: 32,
            tint: None,
        }));
    }

    #[test]
    fn a_lookup_matches_a_pasted_snowflake_as_well_as_a_typed_name() {
        let mut state = DiscordState::default();
        state
            .guilds
            .insert("389006437613043712".to_string(), "Home".to_string());

        assert_eq!(guild_options(&state, "hom").len(), 1);
        assert_eq!(guild_options(&state, "3890064").len(), 1);
        assert_eq!(guild_options(&state, "elsewhere").len(), 0);
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
