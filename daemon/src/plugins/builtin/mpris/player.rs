use std::collections::{BTreeMap, HashMap};

use zbus::zvariant::{OwnedValue, Value};

use crate::variables::VariableValue;

/// Every player puts all of its interfaces on this one object.
pub const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
pub const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
/// The namespace players own a well-known name under. The part after the trailing dot is the
/// application, which is what `preferred_player` is matched against.
pub const NAME_NAMESPACE: &str = "org.mpris.MediaPlayer2";
pub const NAME_PREFIX: &str = "org.mpris.MediaPlayer2.";

/// MPRIS reports both track lengths and positions in microseconds; a key shows seconds.
pub const MICROSECONDS_PER_SECOND: f64 = 1_000_000.0;

/// Ordered worst to best on purpose: when several players are running, the one that is actually
/// playing outranks one that was paused an hour ago, whatever the two did most recently.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum PlaybackStatus {
    #[default]
    Stopped,
    Paused,
    Playing,
}

impl PlaybackStatus {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "playing" => Self::Playing,
            "paused" => Self::Paused,
            _ => Self::Stopped,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Playing => "Playing",
            Self::Paused => "Paused",
            Self::Stopped => "Stopped",
        }
    }

    pub fn is_playing(self) -> bool {
        matches!(self, Self::Playing)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub length_seconds: f64,
}

/// The properties this plugin reads off a player. `GetAll` at startup and every `PropertiesChanged`
/// carry the same shape, so both arrive here, and an absent field means "unchanged" rather than
/// "empty".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerUpdate {
    pub status: Option<PlaybackStatus>,
    pub metadata: Option<TrackMetadata>,
    pub volume: Option<f64>,
}

pub fn read_update(properties: &HashMap<String, OwnedValue>) -> PlayerUpdate {
    PlayerUpdate {
        status: properties
            .get("PlaybackStatus")
            .map(|value| PlaybackStatus::parse(&text(value))),
        metadata: properties.get("Metadata").map(|value| read_metadata(value)),
        volume: properties.get("Volume").and_then(|value| number(value)),
    }
}

pub fn read_metadata(value: &Value<'_>) -> TrackMetadata {
    let mut metadata = TrackMetadata::default();
    for (key, value) in dict_entries(value) {
        match key {
            "xesam:title" => metadata.title = text(value),
            "xesam:artist" => metadata.artist = text(value),
            "xesam:album" => metadata.album = text(value),
            "mpris:artUrl" => metadata.art_url = text(value),
            "mpris:length" => {
                metadata.length_seconds =
                    number(value).unwrap_or_default() / MICROSECONDS_PER_SECOND
            }
            _ => {}
        }
    }
    metadata
}

#[derive(Clone, Debug, Default)]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub metadata: TrackMetadata,
    pub volume: f64,
    pub position_seconds: f64,
    /// Which update this player last spoke on, so the more recent of two equally ranked players
    /// wins.
    updates: u64,
}

/// Which players are on the bus, what each of them is doing, and which one the panel follows.
#[derive(Debug, Default)]
pub struct PlayerRoster {
    preferred: Option<String>,
    players: BTreeMap<String, PlayerState>,
    updates: u64,
}

impl PlayerRoster {
    pub fn new(preferred_player: Option<String>) -> Self {
        Self {
            preferred: preferred_player
                .map(|preferred| preferred.trim().to_ascii_lowercase())
                .filter(|preferred| !preferred.is_empty()),
            players: BTreeMap::new(),
            updates: 0,
        }
    }

    pub fn accepts(&self, bus_name: &str) -> bool {
        bus_name.starts_with(NAME_PREFIX)
            && match &self.preferred {
                Some(preferred) => bus_name.to_ascii_lowercase().contains(preferred),
                None => true,
            }
    }

    pub fn apply(&mut self, bus_name: &str, update: PlayerUpdate) {
        if !self.accepts(bus_name) {
            return;
        }
        self.updates += 1;
        let updates = self.updates;
        let state = self.players.entry(bus_name.to_string()).or_default();
        state.updates = updates;
        if let Some(status) = update.status {
            state.status = status;
        }
        if let Some(metadata) = update.metadata {
            // A different track starts at the beginning. Carrying the old elapsed time over shows a
            // wrong position until the next poll, and a track shorter than the last one shows one
            // past its own end.
            if metadata != state.metadata {
                state.position_seconds = 0.0;
            }
            state.metadata = metadata;
        }
        if let Some(volume) = update.volume {
            state.volume = volume;
        }
    }

    pub fn set_position(&mut self, bus_name: &str, seconds: f64) {
        if let Some(state) = self.players.get_mut(bus_name) {
            state.position_seconds = seconds;
        }
    }

    pub fn forget(&mut self, bus_name: &str) {
        self.players.remove(bus_name);
    }

    pub fn active(&self) -> Option<(&str, &PlayerState)> {
        self.players
            .iter()
            .max_by_key(|(_, state)| (state.status, state.updates))
            .map(|(bus_name, state)| (bus_name.as_str(), state))
    }

    /// Everything this plugin publishes, in one pass. Republishing values that did not move is
    /// free, so every change recomputes the whole set rather than tracking which name it touched.
    pub fn published_values(&self) -> Vec<(&'static str, VariableValue)> {
        let nothing = PlayerState::default();
        let (bus_name, state) = self.active().unwrap_or(("", &nothing));
        vec![
            ("player", VariableValue::Text(bus_name.to_string())),
            ("title", VariableValue::Text(state.metadata.title.clone())),
            ("artist", VariableValue::Text(state.metadata.artist.clone())),
            ("album", VariableValue::Text(state.metadata.album.clone())),
            (
                "art_url",
                VariableValue::Text(state.metadata.art_url.clone()),
            ),
            (
                "status",
                VariableValue::Text(state.status.as_str().to_string()),
            ),
            ("position", VariableValue::Number(state.position_seconds)),
            (
                "length",
                VariableValue::Number(state.metadata.length_seconds),
            ),
            ("volume", VariableValue::Number(state.volume)),
        ]
    }
}

/// A variant wraps every value in an `a{sv}`, and some players wrap twice.
fn unwrap_variant<'a>(value: &'a Value<'a>) -> &'a Value<'a> {
    match value {
        Value::Value(inner) => unwrap_variant(inner),
        other => other,
    }
}

fn dict_entries<'a>(value: &'a Value<'a>) -> impl Iterator<Item = (&'a str, &'a Value<'a>)> {
    let entries = match unwrap_variant(value) {
        Value::Dict(dict) => Some(dict.iter()),
        _ => None,
    };
    entries
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| match unwrap_variant(key) {
            Value::Str(key) => Some((key.as_str(), value)),
            _ => None,
        })
}

fn text(value: &Value<'_>) -> String {
    match unwrap_variant(value) {
        Value::Str(text) => text.to_string(),
        Value::ObjectPath(path) => path.to_string(),
        // `xesam:artist` is a list, and a key has one line to show it on.
        Value::Array(items) => items
            .iter()
            .map(text)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        other => number(other)
            .map(|number| VariableValue::Number(number).to_string())
            .unwrap_or_default(),
    }
}

fn number(value: &Value<'_>) -> Option<f64> {
    match unwrap_variant(value) {
        Value::U8(number) => Some(f64::from(*number)),
        Value::I16(number) => Some(f64::from(*number)),
        Value::U16(number) => Some(f64::from(*number)),
        Value::I32(number) => Some(f64::from(*number)),
        Value::U32(number) => Some(f64::from(*number)),
        Value::I64(number) => Some(*number as f64),
        Value::U64(number) => Some(*number as f64),
        Value::F64(number) => Some(*number),
        // Players exist that publish `mpris:length` as a string.
        Value::Str(text) => text.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Dict, Signature, Type};

    fn variant(value: impl Into<Value<'static>>) -> Value<'static> {
        Value::new(value.into())
    }

    fn metadata_of(entries: Vec<(&str, Value<'static>)>) -> Value<'static> {
        let mut dict = Dict::new(
            <&str as Type>::signature(),
            Signature::from_static_str_unchecked("v"),
        );
        for (key, value) in entries {
            dict.add(key.to_string(), variant(value)).expect("a{sv}");
        }
        Value::Dict(dict)
    }

    fn owned(value: Value<'static>) -> OwnedValue {
        OwnedValue::try_from(value).expect("no file descriptors")
    }

    fn playing(bus_name: &str, roster: &mut PlayerRoster) {
        roster.apply(
            bus_name,
            PlayerUpdate {
                status: Some(PlaybackStatus::Playing),
                ..PlayerUpdate::default()
            },
        );
    }

    fn paused(bus_name: &str, roster: &mut PlayerRoster) {
        roster.apply(
            bus_name,
            PlayerUpdate {
                status: Some(PlaybackStatus::Paused),
                ..PlayerUpdate::default()
            },
        );
    }

    #[test]
    fn the_three_mpris_statuses_parse_and_anything_else_is_stopped() {
        assert_eq!(PlaybackStatus::parse("Playing"), PlaybackStatus::Playing);
        assert_eq!(PlaybackStatus::parse(" paused "), PlaybackStatus::Paused);
        assert_eq!(PlaybackStatus::parse("Stopped"), PlaybackStatus::Stopped);
        assert_eq!(PlaybackStatus::parse(""), PlaybackStatus::Stopped);
        assert_eq!(PlaybackStatus::parse("buffering"), PlaybackStatus::Stopped);
    }

    #[test]
    fn a_metadata_dictionary_maps_onto_the_published_track() {
        let metadata = read_metadata(&metadata_of(vec![
            ("xesam:title", Value::from("Windowlicker")),
            (
                "xesam:artist",
                Value::from(vec!["Aphex Twin".to_string(), "AFX".to_string()]),
            ),
            ("xesam:album", Value::from("Windowlicker")),
            ("mpris:artUrl", Value::from("https://example.test/art.png")),
            ("mpris:length", Value::from(363_000_000_i64)),
            ("xesam:autoRating", Value::from(0.8_f64)),
        ]));

        assert_eq!(
            metadata,
            TrackMetadata {
                title: "Windowlicker".to_string(),
                artist: "Aphex Twin, AFX".to_string(),
                album: "Windowlicker".to_string(),
                art_url: "https://example.test/art.png".to_string(),
                length_seconds: 363.0,
            }
        );
    }

    #[test]
    fn a_track_length_survives_the_types_players_actually_send() {
        let from_unsigned = read_metadata(&metadata_of(vec![(
            "mpris:length",
            Value::from(90_000_000_u64),
        )]));
        let from_string = read_metadata(&metadata_of(vec![(
            "mpris:length",
            Value::from("90000000"),
        )]));

        assert_eq!(from_unsigned.length_seconds, 90.0);
        assert_eq!(from_string.length_seconds, 90.0);
    }

    #[test]
    fn metadata_that_is_missing_everything_reads_as_blank_rather_than_failing() {
        assert_eq!(
            read_metadata(&metadata_of(Vec::new())),
            TrackMetadata::default()
        );
        assert_eq!(
            read_metadata(&Value::from("not a dictionary")),
            TrackMetadata::default()
        );
    }

    #[test]
    fn a_property_change_only_moves_the_properties_it_carries() {
        let mut properties = HashMap::new();
        properties.insert("Volume".to_string(), owned(Value::from(0.4_f64)));

        assert_eq!(
            read_update(&properties),
            PlayerUpdate {
                status: None,
                metadata: None,
                volume: Some(0.4),
            }
        );
    }

    #[test]
    fn get_all_reads_every_property_at_once() {
        let mut properties = HashMap::new();
        properties.insert("PlaybackStatus".to_string(), owned(Value::from("Playing")));
        properties.insert("Volume".to_string(), owned(Value::from(1.0_f64)));
        properties.insert(
            "Metadata".to_string(),
            owned(metadata_of(vec![(
                "xesam:title",
                Value::from("Come to Daddy"),
            )])),
        );

        let update = read_update(&properties);
        assert_eq!(update.status, Some(PlaybackStatus::Playing));
        assert_eq!(update.volume, Some(1.0));
        assert_eq!(
            update.metadata.expect("metadata was present").title,
            "Come to Daddy"
        );
    }

    #[test]
    fn a_playing_player_outranks_one_that_was_paused_more_recently() {
        let mut roster = PlayerRoster::new(None);
        playing("org.mpris.MediaPlayer2.spotify", &mut roster);
        paused("org.mpris.MediaPlayer2.firefox", &mut roster);

        assert_eq!(
            roster.active().map(|(bus_name, _)| bus_name),
            Some("org.mpris.MediaPlayer2.spotify")
        );
    }

    #[test]
    fn the_most_recent_player_wins_when_neither_is_playing() {
        let mut roster = PlayerRoster::new(None);
        paused("org.mpris.MediaPlayer2.spotify", &mut roster);
        paused("org.mpris.MediaPlayer2.firefox", &mut roster);

        assert_eq!(
            roster.active().map(|(bus_name, _)| bus_name),
            Some("org.mpris.MediaPlayer2.firefox")
        );
    }

    #[test]
    fn a_player_that_leaves_hands_the_panel_back_to_the_one_still_running() {
        let mut roster = PlayerRoster::new(None);
        paused("org.mpris.MediaPlayer2.spotify", &mut roster);
        playing("org.mpris.MediaPlayer2.firefox", &mut roster);
        roster.forget("org.mpris.MediaPlayer2.firefox");

        assert_eq!(
            roster.active().map(|(bus_name, _)| bus_name),
            Some("org.mpris.MediaPlayer2.spotify")
        );
    }

    #[test]
    fn a_preferred_player_matches_a_case_insensitive_substring_of_the_bus_name() {
        let roster = PlayerRoster::new(Some("  SPOTIFY ".to_string()));

        assert!(roster.accepts("org.mpris.MediaPlayer2.spotify"));
        assert!(roster.accepts("org.mpris.MediaPlayer2.spotifyd.instance7"));
        assert!(!roster.accepts("org.mpris.MediaPlayer2.firefox.instance2"));
        assert!(!roster.accepts("org.gnome.Shell"));
    }

    #[test]
    fn an_empty_preferred_player_tracks_everything() {
        let roster = PlayerRoster::new(Some("   ".to_string()));

        assert!(roster.accepts("org.mpris.MediaPlayer2.firefox"));
        assert!(!roster.accepts("org.freedesktop.DBus"));
    }

    #[test]
    fn a_player_the_instance_does_not_want_is_never_tracked() {
        let mut roster = PlayerRoster::new(Some("spotify".to_string()));
        playing("org.mpris.MediaPlayer2.firefox", &mut roster);

        assert!(roster.active().is_none());
    }

    #[test]
    fn a_new_track_restarts_the_position() {
        let mut roster = PlayerRoster::new(None);
        playing("org.mpris.MediaPlayer2.spotify", &mut roster);
        roster.set_position("org.mpris.MediaPlayer2.spotify", 42.0);
        roster.apply(
            "org.mpris.MediaPlayer2.spotify",
            PlayerUpdate {
                metadata: Some(TrackMetadata {
                    title: "Next".to_string(),
                    ..TrackMetadata::default()
                }),
                ..PlayerUpdate::default()
            },
        );

        assert_eq!(
            roster
                .active()
                .map(|(_, state)| state.position_seconds)
                .expect("a player is active"),
            0.0
        );
    }

    #[test]
    fn the_same_metadata_arriving_again_leaves_the_position_alone() {
        let mut roster = PlayerRoster::new(None);
        let track = TrackMetadata {
            title: "Same".to_string(),
            ..TrackMetadata::default()
        };
        roster.apply(
            "org.mpris.MediaPlayer2.spotify",
            PlayerUpdate {
                metadata: Some(track.clone()),
                ..PlayerUpdate::default()
            },
        );
        roster.set_position("org.mpris.MediaPlayer2.spotify", 42.0);
        roster.apply(
            "org.mpris.MediaPlayer2.spotify",
            PlayerUpdate {
                metadata: Some(track),
                ..PlayerUpdate::default()
            },
        );

        assert_eq!(
            roster
                .active()
                .map(|(_, state)| state.position_seconds)
                .expect("a player is active"),
            42.0
        );
    }

    #[test]
    fn nothing_running_publishes_a_blank_set_rather_than_leaving_stale_values() {
        let values = PlayerRoster::new(None).published_values();

        assert_eq!(
            values,
            vec![
                ("player", VariableValue::Text(String::new())),
                ("title", VariableValue::Text(String::new())),
                ("artist", VariableValue::Text(String::new())),
                ("album", VariableValue::Text(String::new())),
                ("art_url", VariableValue::Text(String::new())),
                ("status", VariableValue::Text("Stopped".to_string())),
                ("position", VariableValue::Number(0.0)),
                ("length", VariableValue::Number(0.0)),
                ("volume", VariableValue::Number(0.0)),
            ]
        );
    }

    #[test]
    fn the_active_player_is_what_gets_published() {
        let mut roster = PlayerRoster::new(None);
        roster.apply(
            "org.mpris.MediaPlayer2.spotify",
            PlayerUpdate {
                status: Some(PlaybackStatus::Playing),
                metadata: Some(TrackMetadata {
                    title: "Xtal".to_string(),
                    artist: "Aphex Twin".to_string(),
                    length_seconds: 293.0,
                    ..TrackMetadata::default()
                }),
                volume: Some(0.75),
            },
        );
        roster.set_position("org.mpris.MediaPlayer2.spotify", 12.5);
        let values: HashMap<_, _> = roster.published_values().into_iter().collect();

        assert_eq!(
            values["player"],
            VariableValue::Text("org.mpris.MediaPlayer2.spotify".to_string())
        );
        assert_eq!(values["title"], VariableValue::Text("Xtal".to_string()));
        assert_eq!(
            values["artist"],
            VariableValue::Text("Aphex Twin".to_string())
        );
        assert_eq!(values["status"], VariableValue::Text("Playing".to_string()));
        assert_eq!(values["position"], VariableValue::Number(12.5));
        assert_eq!(values["length"], VariableValue::Number(293.0));
        assert_eq!(values["volume"], VariableValue::Number(0.75));
    }
}
