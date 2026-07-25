mod config;
mod player;

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::Value as JsonValue;
use zbus::{
    fdo::{DBusProxy, PropertiesProxy},
    names::{BusName, InterfaceName},
    zvariant::OwnedValue,
    CacheProperties, Connection, MatchRule, Message, MessageStream, MessageType,
};

use crate::{
    plugins::{
        builtin::mpris::{
            config::MprisConfig,
            player::{
                read_update, PlayerRoster, PlayerUpdate, MICROSECONDS_PER_SECOND, NAME_NAMESPACE,
                NAME_PREFIX, OBJECT_PATH, PLAYER_INTERFACE,
            },
        },
        instance::InstanceConfig,
        manifest::{
            ActionDefinition, ConfigField, PluginManifest, VariableDefinition, VariableKind,
        },
        plugin::{Plugin, PluginContext, PluginError, PluginFactory, Subscription},
    },
    surfaces::logs::SurfaceLogLevel,
    variables::VariableValue,
};

/// The value name whose presence on a panel is what turns position polling on. MPRIS never signals
/// the elapsed time, so it is the one reading this plugin has to ask for.
const POSITION_VALUE: &str = "position";
const ART_URL_VALUE: &str = "art_url";
const ART_VALUE: &str = "art";

/// `file://` URLs arrive percent-encoded, and cover art lives in directories with spaces in them
/// more often than not.
fn percent_decoded(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[at + 1..at + 3], 16) {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub const FACTORY: PluginFactory = PluginFactory {
    plugin_type: "mpris",
    manifest,
    start: |config, context| Box::pin(start(config, context)),
};

/// The player-control half of MPRIS. Only what this plugin drives is declared.
#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2",
    gen_blocking = false
)]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn stop(&self) -> zbus::Result<()>;
    /// Relative to where the player is now, in microseconds, and negative to go back.
    fn seek(&self, offset: i64) -> zbus::Result<()>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn set_volume(&self, volume: f64) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2",
    default_path = "/org/mpris/MediaPlayer2",
    gen_blocking = false
)]
trait Application {
    fn raise(&self) -> zbus::Result<()>;
}

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_type: "mpris",
        display_name: "Local media",
        description: "Whatever is playing on this machine, over MPRIS.",
        config_schema: vec![
            ConfigField::text("preferred_player")
                .label("Preferred player")
                .placeholder("spotify")
                .help(
                    "Only players whose bus name contains this are followed. \
                     Empty follows whichever player is playing.",
                ),
            ConfigField::number("position_interval_ms")
                .label("Position interval (ms)")
                .help("How often the elapsed time is read while something is playing."),
        ],
        actions: vec![
            ActionDefinition::new("play_pause").label("Play or pause"),
            ActionDefinition::new("next").label("Next track"),
            ActionDefinition::new("previous").label("Previous track"),
            ActionDefinition::new("stop").label("Stop"),
            ActionDefinition::new("seek")
                .label("Seek")
                .description("Moves the playhead relative to where it is now.")
                .parameters(vec![ConfigField::number("offset_seconds")
                    .label("Offset (seconds)")
                    .placeholder("10")
                    .required()
                    .help("Negative goes back.")]),
            ActionDefinition::new("set_volume")
                .label("Set volume")
                .parameters(vec![ConfigField::number("volume")
                    .label("Volume")
                    .placeholder("0.5")
                    .required()
                    .help("Between 0 and 1.")]),
            ActionDefinition::new("raise")
                .label("Raise window")
                .description("Brings the player's window to the front."),
        ],
        variables: vec![
            VariableDefinition::new("player", VariableKind::Text)
                .description("Bus name of the player being followed."),
            VariableDefinition::new("title", VariableKind::Text),
            VariableDefinition::new("artist", VariableKind::Text),
            VariableDefinition::new("album", VariableKind::Text),
            VariableDefinition::new("art", VariableKind::Image)
                .description("The cover art itself, ready to show as a key background."),
            VariableDefinition::new("art_url", VariableKind::Text)
                .description("Where the player says its cover art lives."),
            VariableDefinition::new("status", VariableKind::Text)
                .description("Playing, Paused or Stopped."),
            VariableDefinition::new("position", VariableKind::Number)
                .description("Elapsed seconds, read while something is playing."),
            VariableDefinition::new("length", VariableKind::Number).description("Track seconds."),
            VariableDefinition::new("volume", VariableKind::Number).description("Between 0 and 1."),
        ],
    }
}

async fn start(
    config: InstanceConfig,
    context: PluginContext,
) -> Result<Arc<dyn Plugin>, PluginError> {
    let settings: MprisConfig = config.deserialize().map_err(PluginError::Configuration)?;
    let connection = Connection::session().await.map_err(|error| {
        PluginError::Configuration(format!("no session bus to watch for players: {error}"))
    })?;

    let plugin = Arc::new(MprisPlugin {
        connection,
        context: context.clone(),
        watched: Mutex::new(Watched::new(settings.preferred_player.clone())),
        wants_position: AtomicBool::new(false),
        wants_art: AtomicBool::new(false),
        fetched_art: Arc::new(Mutex::new(String::new())),
    });

    tokio::spawn(watch(plugin.clone()));

    let interval = settings.position_interval();
    let poller = plugin.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            tokio::select! {
                _ = context.cancel.cancelled() => break,
                _ = ticker.tick() => poller.refresh_position(true).await,
            }
        }
    });

    Ok(plugin)
}

/// Everything keyed by a well-known `org.mpris.MediaPlayer2.*` name, plus the mapping needed to get
/// there from what a signal actually carries.
struct Watched {
    roster: PlayerRoster,
    /// Signals arrive from a unique name like `:1.42`, not from the name the player owns.
    owners: HashMap<String, String>,
    /// Unique names at the MPRIS object path that turned out not to own a player name. Unique names
    /// are never reused, so one failed lookup settles it for good and a stranger cannot make this
    /// plugin re-list the bus on every signal it sends.
    strangers: HashSet<String>,
}

impl Watched {
    fn new(preferred_player: Option<String>) -> Self {
        Self {
            roster: PlayerRoster::new(preferred_player),
            owners: HashMap::new(),
            strangers: HashSet::new(),
        }
    }
}

struct MprisPlugin {
    connection: Connection,
    context: PluginContext,
    watched: Mutex<Watched>,
    wants_position: AtomicBool,
    wants_art: AtomicBool,
    /// The art URL whose bytes are already stored, so a track that keeps its cover is fetched once
    /// rather than on every property change.
    fetched_art: Arc<Mutex<String>>,
}

/// Follows the bus for as long as the instance lives: property changes from every player, and the
/// name traffic that says which players exist at all.
async fn watch(plugin: Arc<MprisPlugin>) {
    let streams = tokio::select! {
        _ = plugin.context.cancel.cancelled() => return,
        streams = plugin.subscribe_to_the_bus() => streams,
    };
    let (mut player_signals, mut name_changes) = match streams {
        Ok(streams) => streams,
        Err(error) => {
            plugin.warn(format!("cannot watch the session bus: {error}"));
            return;
        }
    };

    plugin.adopt_running_players().await;

    loop {
        tokio::select! {
            _ = plugin.context.cancel.cancelled() => break,
            Some(message) = player_signals.next() => {
                if let Ok(message) = message {
                    plugin.on_player_signal(&message).await;
                }
            }
            Some(message) = name_changes.next() => {
                if let Ok(message) = message {
                    plugin.on_name_change(&message).await;
                }
            }
        }
    }
}

impl MprisPlugin {
    async fn subscribe_to_the_bus(&self) -> Result<(MessageStream, MessageStream), zbus::Error> {
        // Matching on the object path rather than per player: every player signals from the same
        // path, and one rule keeps working as players come and go.
        let player_signals = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .path(OBJECT_PATH)?
            .build();
        let name_changes = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .arg0ns(NAME_NAMESPACE)?
            .build();

        Ok((
            MessageStream::for_match_rule(player_signals, &self.connection, None).await?,
            MessageStream::for_match_rule(name_changes, &self.connection, None).await?,
        ))
    }

    /// Reads the players that were already running when this instance started, and doubles as the
    /// repair path when a signal arrives from a unique name nothing has mapped yet.
    async fn adopt_running_players(&self) {
        let dbus = match DBusProxy::new(&self.connection).await {
            Ok(dbus) => dbus,
            Err(error) => return self.warn(format!("cannot talk to the session bus: {error}")),
        };
        let names = match dbus.list_names().await {
            Ok(names) => names,
            Err(error) => return self.warn(format!("cannot list the session bus: {error}")),
        };

        for name in names {
            let name = name.as_str();
            if !name.starts_with(NAME_PREFIX) {
                continue;
            }
            let Ok(bus_name) = BusName::try_from(name) else {
                continue;
            };
            let Ok(owner) = dbus.get_name_owner(bus_name).await else {
                continue;
            };
            self.adopt(owner.as_str(), name).await;
        }
        self.publish();
    }

    async fn adopt(&self, unique_name: &str, bus_name: &str) {
        {
            let mut watched = self.watched.lock().unwrap();
            if !watched.roster.accepts(bus_name) {
                return;
            }
            watched.strangers.remove(unique_name);
            watched
                .owners
                .insert(unique_name.to_string(), bus_name.to_string());
        }
        let update = self.read_player(bus_name).await;
        self.watched.lock().unwrap().roster.apply(bus_name, update);
    }

    /// One round trip for everything a player is currently doing, so a player that started before
    /// this instance is not blank until it happens to change something.
    async fn read_player(&self, bus_name: &str) -> PlayerUpdate {
        match self.read_player_properties(bus_name).await {
            Ok(properties) => read_update(&properties),
            Err(error) => {
                self.warn(format!("{bus_name} would not describe itself: {error}"));
                PlayerUpdate::default()
            }
        }
    }

    async fn read_player_properties(
        &self,
        bus_name: &str,
    ) -> Result<HashMap<String, OwnedValue>, zbus::Error> {
        let properties = PropertiesProxy::builder(&self.connection)
            .destination(bus_name.to_string())?
            .path(OBJECT_PATH)?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        let interface = InterfaceName::try_from(PLAYER_INTERFACE)?;
        Ok(properties.get_all(Some(interface).into()).await?)
    }

    async fn on_player_signal(&self, message: &Message) {
        let header = message.header();
        let (Some(sender), Some(member)) = (header.sender(), header.member()) else {
            return;
        };
        let sender = sender.to_string();
        let body = message.body();

        match member.as_str() {
            "PropertiesChanged" => {
                let Ok((interface, changed, _invalidated)) =
                    body.deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
                else {
                    return;
                };
                if interface != PLAYER_INTERFACE {
                    return;
                }
                let Some(bus_name) = self.owner_of(&sender).await else {
                    return;
                };
                let update = read_update(&changed);
                let follows_the_playhead = update.status.is_some() || update.metadata.is_some();
                self.watched.lock().unwrap().roster.apply(&bus_name, update);
                self.publish();
                // Pausing or changing track moves the elapsed time without a tick to catch it, and
                // a paused player is never polled.
                if follows_the_playhead {
                    self.refresh_position(false).await;
                }
            }
            "Seeked" => {
                let Ok(position) = body.deserialize::<i64>() else {
                    return;
                };
                let Some(bus_name) = self.owner_of(&sender).await else {
                    return;
                };
                self.watched
                    .lock()
                    .unwrap()
                    .roster
                    .set_position(&bus_name, position as f64 / MICROSECONDS_PER_SECOND);
                self.publish();
            }
            _ => {}
        }
    }

    async fn on_name_change(&self, message: &Message) {
        let body = message.body();
        let Ok((bus_name, old_owner, new_owner)) = body.deserialize::<(String, String, String)>()
        else {
            return;
        };
        if !bus_name.starts_with(NAME_PREFIX) {
            return;
        }

        if !old_owner.is_empty() {
            self.watched.lock().unwrap().owners.remove(&old_owner);
        }
        if new_owner.is_empty() {
            self.watched.lock().unwrap().roster.forget(&bus_name);
        } else {
            self.adopt(&new_owner, &bus_name).await;
        }
        self.publish();
    }

    async fn owner_of(&self, unique_name: &str) -> Option<String> {
        {
            let watched = self.watched.lock().unwrap();
            if let Some(bus_name) = watched.owners.get(unique_name) {
                return Some(bus_name.clone());
            }
            if watched.strangers.contains(unique_name) {
                return None;
            }
        }
        // A player is free to publish its first property change before the bus tells anyone it
        // exists, so an unknown sender is a reason to look again rather than to drop the signal.
        self.adopt_running_players().await;

        let mut watched = self.watched.lock().unwrap();
        match watched.owners.get(unique_name) {
            Some(bus_name) => Some(bus_name.clone()),
            None => {
                watched.strangers.insert(unique_name.to_string());
                None
            }
        }
    }

    /// Reads the elapsed time off the active player. `require_playing` is what keeps the timer from
    /// waking a paused player up every second.
    async fn refresh_position(&self, require_playing: bool) {
        if !self.wants_position.load(Ordering::Relaxed) {
            return;
        }
        let Some((bus_name, is_playing)) = self.active_player() else {
            return;
        };
        if require_playing && !is_playing {
            return;
        }
        let Ok(player) = self.player_proxy(&bus_name).await else {
            return;
        };
        let Ok(position) = player.position().await else {
            return;
        };
        self.watched
            .lock()
            .unwrap()
            .roster
            .set_position(&bus_name, position as f64 / MICROSECONDS_PER_SECOND);
        self.publish();
    }

    fn active_player(&self) -> Option<(String, bool)> {
        self.watched
            .lock()
            .unwrap()
            .roster
            .active()
            .map(|(bus_name, state)| (bus_name.to_string(), state.status.is_playing()))
    }

    /// Property caching is off because `Position` moves without any signal saying so, and a cached
    /// one would freeze the moment it was first read.
    async fn player_proxy(&self, bus_name: &str) -> Result<PlayerProxy<'static>, PluginError> {
        PlayerProxy::builder(&self.connection)
            .destination(bus_name.to_string())
            .map(|builder| builder.cache_properties(CacheProperties::No))
            .map_err(upstream)?
            .build()
            .await
            .map_err(upstream)
    }

    async fn application_proxy(
        &self,
        bus_name: &str,
    ) -> Result<ApplicationProxy<'static>, PluginError> {
        ApplicationProxy::builder(&self.connection)
            .destination(bus_name.to_string())
            .map(|builder| builder.cache_properties(CacheProperties::No))
            .map_err(upstream)?
            .build()
            .await
            .map_err(upstream)
    }

    fn publish(&self) {
        let values = self.watched.lock().unwrap().roster.published_values();
        let mut art_url = String::new();
        for (name, value) in values {
            if name == ART_URL_VALUE {
                art_url = value.to_string();
            }
            self.context.set_value(name, value);
        }
        self.refresh_art(art_url);
    }

    /// Fetched off the publish path: the render path never waits on a download, and the `art` value
    /// only changes once the bytes are actually stored.
    fn refresh_art(&self, url: String) {
        if !self.wants_art.load(Ordering::Relaxed) {
            return;
        }
        {
            let mut fetched = self.fetched_art.lock().unwrap();
            if *fetched == url {
                return;
            }
            fetched.clone_from(&url);
        }
        if url.is_empty() {
            self.context
                .set_value(ART_VALUE, VariableValue::Text(String::new()));
            return;
        }

        let context = self.context.clone();
        let fetched = self.fetched_art.clone();
        tokio::spawn(async move {
            match load_art(&context, &url).await {
                Ok(bytes) => {
                    if let Err(error) = context.set_image(ART_VALUE, &bytes) {
                        context.log(
                            SurfaceLogLevel::Warning,
                            format!("could not store the cover art: {error}"),
                        );
                    }
                }
                Err(reason) => {
                    // A missing cover is ordinary, so this stays a log line rather than an error
                    // state, but the URL that failed is worth having. Clearing the record lets the
                    // next announcement of the same track try again.
                    context.log(
                        SurfaceLogLevel::Warning,
                        format!("could not load cover art from {url}: {reason}"),
                    );
                    fetched.lock().unwrap().clear();
                }
            }
        });
    }
}

/// Local players hand out `file://` URLs for cover art, which no HTTP client will fetch.
async fn load_art(context: &PluginContext, url: &str) -> Result<Vec<u8>, String> {
    {
        if let Some(path) = url.strip_prefix("file://") {
            let path = percent_decoded(path);
            return tokio::fs::read(&path)
                .await
                .map_err(|error| format!("{path}: {error}"));
        }
        let response = context
            .http
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("answered {}", response.status().as_u16()));
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| error.to_string())
    }
}

impl MprisPlugin {
    fn warn(&self, message: String) {
        self.context.log(SurfaceLogLevel::Warning, message);
    }
}

#[async_trait]
impl Plugin for MprisPlugin {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue) -> Result<(), PluginError> {
        let command = parse_command(action_name, parameters, |raw| self.context.interpolate(raw))?;
        let Some((bus_name, _)) = self.active_player() else {
            return Err(PluginError::Upstream(
                "no media player is running".to_string(),
            ));
        };

        if let PlayerCommand::Raise = command {
            return self
                .application_proxy(&bus_name)
                .await?
                .raise()
                .await
                .map_err(upstream);
        }

        let player = self.player_proxy(&bus_name).await?;
        match command {
            PlayerCommand::PlayPause => player.play_pause().await,
            PlayerCommand::Next => player.next().await,
            PlayerCommand::Previous => player.previous().await,
            PlayerCommand::Stop => player.stop().await,
            PlayerCommand::Seek {
                offset_microseconds,
            } => player.seek(offset_microseconds).await,
            PlayerCommand::SetVolume { volume } => player.set_volume(volume).await,
            PlayerCommand::Raise => unreachable!("raise is answered above"),
        }
        .map_err(upstream)
    }

    async fn subscribe(&self, subscriptions: &[Subscription]) -> Result<(), PluginError> {
        let wants_position = subscriptions
            .iter()
            .any(|subscription| subscription.name == POSITION_VALUE);
        self.wants_position.store(wants_position, Ordering::Relaxed);
        self.wants_art.store(
            subscriptions
                .iter()
                .any(|subscription| subscription.name == ART_VALUE),
            Ordering::Relaxed,
        );
        if wants_position {
            self.refresh_position(false).await;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerCommand {
    PlayPause,
    Next,
    Previous,
    Stop,
    Seek { offset_microseconds: i64 },
    SetVolume { volume: f64 },
    Raise,
}

/// Turns an action and its form parameters into something to send, before any player is involved,
/// so a mistyped action or a parameter that is not a number is reported the same whether or not
/// anything is playing.
fn parse_command(
    action_name: &str,
    parameters: &JsonValue,
    resolve: impl Fn(&str) -> String,
) -> Result<PlayerCommand, PluginError> {
    match action_name {
        "play_pause" => Ok(PlayerCommand::PlayPause),
        "next" => Ok(PlayerCommand::Next),
        "previous" => Ok(PlayerCommand::Previous),
        "stop" => Ok(PlayerCommand::Stop),
        "raise" => Ok(PlayerCommand::Raise),
        "seek" => {
            let offset = number_parameter(parameters, "offset_seconds", &resolve)?;
            Ok(PlayerCommand::Seek {
                offset_microseconds: (offset * MICROSECONDS_PER_SECOND) as i64,
            })
        }
        "set_volume" => {
            let volume = number_parameter(parameters, "volume", &resolve)?;
            Ok(PlayerCommand::SetVolume {
                volume: volume.clamp(0.0, 1.0),
            })
        }
        other => Err(PluginError::UnknownAction(other.to_string())),
    }
}

fn number_parameter(
    parameters: &JsonValue,
    key: &str,
    resolve: impl Fn(&str) -> String,
) -> Result<f64, PluginError> {
    let raw = optional_string(parameters, key)
        .ok_or_else(|| PluginError::Configuration(format!("{key} is required")))?;
    let resolved = resolve(&raw);
    resolved
        .trim()
        .parse::<f64>()
        .map_err(|_| PluginError::Configuration(format!("{key} is not a number: {resolved}")))
}

/// Accepts a number where a string is expected, because a generated form and a hand-written TOML
/// disagree about how `10` should be typed.
fn optional_string(parameters: &JsonValue, key: &str) -> Option<String> {
    match parameters.get(key)? {
        JsonValue::String(value) if value.is_empty() => None,
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Null => None,
        other => Some(other.to_string()),
    }
}

fn upstream(error: impl std::fmt::Display) -> PluginError {
    PluginError::Upstream(error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_file_url_path_is_percent_decoded() {
        assert_eq!(
            percent_decoded("/home/luc/Music/My%20Album/cover.jpg"),
            "/home/luc/Music/My Album/cover.jpg"
        );
        assert_eq!(percent_decoded("/plain/path.png"), "/plain/path.png");
        // A stray percent is left alone rather than eating the next two characters.
        assert_eq!(percent_decoded("/100%/x"), "/100%/x");
    }

    use super::*;
    use crate::{
        identifiers::IntegrationId,
        plugins::plugin::cancellation,
        variables::{VariableRef, VariableStore},
    };

    use crate::variables::VariableValue;
    use std::time::Duration;
    use zbus::{object_server::SignalContext, zvariant::Value};

    fn parse(action_name: &str, parameters: JsonValue) -> Result<PlayerCommand, PluginError> {
        parse_command(action_name, &parameters, |raw| raw.to_string())
    }

    #[test]
    fn the_transport_actions_need_no_parameters() {
        assert_eq!(
            parse("play_pause", serde_json::json!({})),
            Ok(PlayerCommand::PlayPause)
        );
        assert_eq!(
            parse("next", serde_json::json!({})),
            Ok(PlayerCommand::Next)
        );
        assert_eq!(
            parse("previous", serde_json::json!({})),
            Ok(PlayerCommand::Previous)
        );
        assert_eq!(
            parse("stop", serde_json::json!({})),
            Ok(PlayerCommand::Stop)
        );
        assert_eq!(
            parse("raise", serde_json::json!({})),
            Ok(PlayerCommand::Raise)
        );
    }

    #[test]
    fn an_unknown_action_is_reported_by_name() {
        assert_eq!(
            parse("shuffle", serde_json::json!({})),
            Err(PluginError::UnknownAction("shuffle".to_string()))
        );
    }

    #[test]
    fn seeking_converts_seconds_into_the_microseconds_mpris_wants() {
        assert_eq!(
            parse("seek", serde_json::json!({ "offset_seconds": 10 })),
            Ok(PlayerCommand::Seek {
                offset_microseconds: 10_000_000
            })
        );
        assert_eq!(
            parse("seek", serde_json::json!({ "offset_seconds": "-2.5" })),
            Ok(PlayerCommand::Seek {
                offset_microseconds: -2_500_000
            })
        );
    }

    #[test]
    fn a_missing_or_unparseable_number_is_a_configuration_error() {
        assert!(matches!(
            parse("seek", serde_json::json!({})),
            Err(PluginError::Configuration(_))
        ));
        assert!(matches!(
            parse("set_volume", serde_json::json!({ "volume": "loud" })),
            Err(PluginError::Configuration(_))
        ));
    }

    #[test]
    fn a_volume_outside_the_mpris_range_is_clamped_rather_than_rejected() {
        assert_eq!(
            parse("set_volume", serde_json::json!({ "volume": 1.4 })),
            Ok(PlayerCommand::SetVolume { volume: 1.0 })
        );
        assert_eq!(
            parse("set_volume", serde_json::json!({ "volume": -3 })),
            Ok(PlayerCommand::SetVolume { volume: 0.0 })
        );
    }

    #[test]
    fn a_parameter_resolves_variable_references_before_it_is_read_as_a_number() {
        let command = parse_command(
            "set_volume",
            &serde_json::json!({ "volume": "$(user:level)" }),
            |raw| raw.replace("$(user:level)", "0.25"),
        );
        assert_eq!(command, Ok(PlayerCommand::SetVolume { volume: 0.25 }));
    }

    /// Holds the pieces an instance's tasks depend on. Dropping the cancel handle stops them, and
    /// dropping the signal receiver closes the sink the plugin publishes through, so the test has
    /// to keep both alive for as long as the plugin.
    struct Started {
        plugin: Arc<dyn Plugin>,
        variables: Arc<VariableStore>,
        _cancel: crate::plugins::plugin::CancelHandle,
        _signals: tokio::sync::mpsc::Receiver<crate::plugins::engine::EngineSignal>,
    }

    async fn started(config: &str) -> Result<Started, PluginError> {
        let variables = Arc::new(VariableStore::default());
        let (signals, receiver) = tokio::sync::mpsc::channel(64);
        let (cancel, token) = cancellation();
        let integration_id = IntegrationId("mpris.local".to_string());
        let context = PluginContext::new(
            integration_id.clone(),
            variables.clone(),
            Arc::default(),
            signals,
            token,
            reqwest::Client::new(),
        );
        let plugin = start(
            InstanceConfig {
                integration_id,
                values: toml::from_str(config).expect("valid toml"),
            },
            context,
        )
        .await?;
        Ok(Started {
            plugin,
            variables,
            _cancel: cancel,
            _signals: receiver,
        })
    }

    async fn await_value(variables: &VariableStore, name: &str, expected: VariableValue) {
        let reference = VariableRef::new("mpris.local", name);
        for _ in 0..200 {
            if variables.get(&reference).as_ref() == Some(&expected) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!(
            "{name} was {:?}, expected {expected:?}",
            variables.get(&reference)
        );
    }

    /// The one thing that has to hold on a machine with no desktop session: starting reports the
    /// missing bus rather than hanging or panicking. Where there is a session bus this instead
    /// covers the ordinary path, since the instance has to come up there.
    #[tokio::test]
    async fn a_machine_without_a_session_bus_reports_a_configuration_error() {
        let has_session_bus = Connection::session().await.is_ok();
        let started = started("preferred_player = \"spotify\"").await;

        match (has_session_bus, started) {
            (true, started) => assert!(started.is_ok()),
            (false, Err(PluginError::Configuration(_))) => {}
            (false, other) => panic!("expected a configuration error, got {:?}", other.err()),
        }
    }

    /// A player good enough to answer everything this plugin reads and to record what it is told.
    /// Shared state rather than the object server's, so the test can look at both sides.
    #[derive(Clone)]
    struct FakePlayer {
        state: Arc<Mutex<FakeState>>,
    }

    #[derive(Default)]
    struct FakeState {
        status: String,
        title: String,
        volume: f64,
        position: i64,
        play_pause_calls: u32,
        seek_offsets: Vec<i64>,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl FakePlayer {
        async fn play_pause(&self) {
            self.state.lock().unwrap().play_pause_calls += 1;
        }

        async fn seek(&self, offset: i64) {
            self.state.lock().unwrap().seek_offsets.push(offset);
        }

        #[zbus(property)]
        async fn playback_status(&self) -> String {
            self.state.lock().unwrap().status.clone()
        }

        #[zbus(property)]
        async fn metadata(&self) -> HashMap<String, OwnedValue> {
            let title = self.state.lock().unwrap().title.clone();
            HashMap::from([
                (
                    "xesam:title".to_string(),
                    OwnedValue::try_from(Value::from(title)).expect("a title"),
                ),
                (
                    "mpris:length".to_string(),
                    OwnedValue::from(180_000_000_i64),
                ),
            ])
        }

        #[zbus(property)]
        async fn volume(&self) -> f64 {
            self.state.lock().unwrap().volume
        }

        #[zbus(property)]
        async fn set_volume(&self, volume: f64) {
            self.state.lock().unwrap().volume = volume;
        }

        #[zbus(property)]
        async fn position(&self) -> i64 {
            self.state.lock().unwrap().position
        }
    }

    /// Needs a session bus, which a headless machine does not have; there it proves nothing and
    /// stays out of the way.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_player_on_the_session_bus_is_followed_and_driven() {
        let fake = FakePlayer {
            state: Arc::new(Mutex::new(FakeState {
                status: "Paused".to_string(),
                title: "Xtal".to_string(),
                volume: 0.5,
                position: 12_500_000,
                ..FakeState::default()
            })),
        };
        let Ok(served) = zbus::ConnectionBuilder::session()
            .and_then(|builder| builder.name("org.mpris.MediaPlayer2.launchpitest"))
            .and_then(|builder| builder.serve_at(OBJECT_PATH, fake.clone()))
            .expect("a well-formed player")
            .build()
            .await
        else {
            return;
        };

        let started = started("preferred_player = \"launchpitest\"\nposition_interval_ms = 200\n")
            .await
            .expect("the instance starts");

        await_value(
            &started.variables,
            "title",
            VariableValue::Text("Xtal".to_string()),
        )
        .await;
        await_value(
            &started.variables,
            "status",
            VariableValue::Text("Paused".to_string()),
        )
        .await;
        await_value(&started.variables, "length", VariableValue::Number(180.0)).await;
        await_value(&started.variables, "volume", VariableValue::Number(0.5)).await;

        // Nothing watches the position yet, so nothing should have gone looking for it.
        assert_eq!(
            started
                .variables
                .get(&VariableRef::new("mpris.local", POSITION_VALUE)),
            Some(VariableValue::Number(0.0))
        );
        started
            .plugin
            .subscribe(&[Subscription {
                name: POSITION_VALUE.to_string(),
            }])
            .await
            .expect("subscriptions are accepted");
        await_value(
            &started.variables,
            POSITION_VALUE,
            VariableValue::Number(12.5),
        )
        .await;

        let interface = served
            .object_server()
            .interface::<_, FakePlayer>(OBJECT_PATH)
            .await
            .expect("the fake player is served");
        {
            let mut state = fake.state.lock().unwrap();
            state.status = "Playing".to_string();
            state.title = "Come to Daddy".to_string();
        }
        let context: &SignalContext<'_> = interface.signal_context();
        interface
            .get()
            .await
            .playback_status_changed(context)
            .await
            .expect("the status change is announced");
        interface
            .get()
            .await
            .metadata_changed(context)
            .await
            .expect("the metadata change is announced");

        await_value(
            &started.variables,
            "status",
            VariableValue::Text("Playing".to_string()),
        )
        .await;
        await_value(
            &started.variables,
            "title",
            VariableValue::Text("Come to Daddy".to_string()),
        )
        .await;

        assert_eq!(
            started
                .plugin
                .invoke("play_pause", &serde_json::json!({}))
                .await,
            Ok(())
        );
        assert_eq!(
            started
                .plugin
                .invoke("seek", &serde_json::json!({ "offset_seconds": -5 }))
                .await,
            Ok(())
        );
        assert_eq!(
            started
                .plugin
                .invoke("set_volume", &serde_json::json!({ "volume": 0.2 }))
                .await,
            Ok(())
        );

        let state = fake.state.lock().unwrap();
        assert_eq!(state.play_pause_calls, 1);
        assert_eq!(state.seek_offsets, vec![-5_000_000]);
        assert_eq!(state.volume, 0.2);
    }
}
