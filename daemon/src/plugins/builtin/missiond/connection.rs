use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{header::ACCEPT, RequestBuilder, Response, StatusCode, Url};
use serde::Deserialize;
use tokio::time::{interval, timeout, MissedTickBehavior};

use crate::{
    plugins::{
        builtin::missiond::{config::Target, discovery::Discovery, presets},
        plugin::{PluginContext, PluginError},
    },
    surfaces::logs::SurfaceLogLevel,
    variables::VariableValue,
};

const FIRST_RETRY: Duration = Duration::from_secs(1);
const LONGEST_RETRY: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// missiond writes a keep-alive every 30 seconds, so a stream silent for longer than this is one
/// the network dropped without closing.
const STREAM_SILENCE: Duration = Duration::from_secs(75);
/// The countdown to the next tab is kept by the local clock, not asked for.
const COUNTDOWN_TICK: Duration = Duration::from_secs(1);
const ACTIVE_COLOR: &str = "#22c55e";

/// The `state` frame of `/api/events`.
#[derive(Clone, Debug, Deserialize)]
pub struct State {
    pub device_name: String,
    pub current_playlist_id: Option<String>,
    pub current_tab_id: Option<String>,
    pub auto_rotate: bool,
    /// Unix seconds. Absent while rotation is paused.
    pub next_rotation_at: Option<u64>,
    pub screen_on: bool,
    pub brightness: u32,
    pub requires_auth: bool,
    pub access: Access,
}

/// What the key this instance connected with lets it do.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    Admin,
    Control,
    None,
}

/// The `catalogue` frame of `/api/events`.
#[derive(Debug, Deserialize)]
struct Catalogue {
    playlists: Vec<Playlist>,
}

#[derive(Debug, Deserialize)]
pub struct Playlist {
    pub playlist_id: String,
    pub name: String,
    pub tabs: Vec<Tab>,
}

#[derive(Debug, Deserialize)]
pub struct Tab {
    pub tab_id: String,
    pub name: String,
}

/// The plugin's copy of the display, answered from by `invoke` and `lookup` without a request.
#[derive(Debug, Default)]
pub struct View {
    /// Where commands go. Known before the stream opens for a configured address, and only once
    /// the display has been found for a discovered one.
    pub api: Option<Url>,
    pub is_connected: bool,
    pub state: Option<State>,
    pub playlists: Vec<Playlist>,
    /// Every value name last published, so a playlist that is deleted takes its values with it.
    published: HashSet<String>,
}

impl View {
    /// Every tab once, with the first playlist that holds it. A tab id is unique across missiond,
    /// so a tab in two playlists is still one tab on screen.
    pub fn tabs(&self) -> Vec<(&Playlist, &Tab)> {
        let mut seen = HashSet::new();
        self.playlists
            .iter()
            .flat_map(|playlist| playlist.tabs.iter().map(move |tab| (playlist, tab)))
            .filter(|(_, tab)| seen.insert(tab.tab_id.as_str()))
            .collect()
    }

    fn playlist_name(&self, playlist_id: &str) -> String {
        self.playlists
            .iter()
            .find(|playlist| playlist.playlist_id == playlist_id)
            .map_or_else(|| playlist_id.to_string(), |playlist| playlist.name.clone())
    }

    fn tab_name(&self, tab_id: &str) -> String {
        self.playlists
            .iter()
            .flat_map(|playlist| &playlist.tabs)
            .find(|tab| tab.tab_id == tab_id)
            .map_or_else(|| tab_id.to_string(), |tab| tab.name.clone())
    }
}

/// Everything a panel can read, derived from the view alone so that both frames publish through
/// one path.
fn values(view: &View, now: u64) -> Vec<(String, VariableValue)> {
    let mut values = vec![(
        "connected".to_string(),
        VariableValue::Boolean(view.is_connected),
    )];
    let state = view.state.as_ref();
    let playlist_id = state.and_then(|state| state.current_playlist_id.as_deref());
    let tab_id = state.and_then(|state| state.current_tab_id.as_deref());

    if let Some(state) = state {
        values.extend([
            ("device_name".to_string(), text(&state.device_name)),
            (
                "brightness".to_string(),
                VariableValue::Number(f64::from(state.brightness)),
            ),
            (
                "playlist_id".to_string(),
                text(playlist_id.unwrap_or_default()),
            ),
            (
                "playlist_name".to_string(),
                VariableValue::Text(
                    playlist_id
                        .map(|id| view.playlist_name(id))
                        .unwrap_or_default(),
                ),
            ),
            ("tab_id".to_string(), text(tab_id.unwrap_or_default())),
            (
                "tab_name".to_string(),
                VariableValue::Text(tab_id.map(|id| view.tab_name(id)).unwrap_or_default()),
            ),
            ("awake".to_string(), VariableValue::Boolean(state.screen_on)),
            (
                "screen_state".to_string(),
                text(if state.screen_on { "Awake" } else { "Asleep" }),
            ),
            ("screen_color".to_string(), color(state.screen_on)),
            (
                "paused".to_string(),
                VariableValue::Boolean(!state.auto_rotate),
            ),
            (
                "playback_state".to_string(),
                text(if state.auto_rotate {
                    "Rotating"
                } else {
                    "Paused"
                }),
            ),
            ("playback_color".to_string(), color(state.auto_rotate)),
        ]);
        values.extend(countdown(state, now));
    }

    for playlist in &view.playlists {
        let is_active = playlist_id == Some(playlist.playlist_id.as_str());
        let prefix = format!("playlists.{}", playlist.playlist_id);
        values.push((format!("{prefix}.name"), text(&playlist.name)));
        values.push((
            format!("{prefix}.active"),
            VariableValue::Boolean(is_active),
        ));
        values.push((format!("{prefix}.color"), color(is_active)));
    }
    for (_, tab) in view.tabs() {
        let is_active = tab_id == Some(tab.tab_id.as_str());
        let prefix = format!("tabs.{}", tab.tab_id);
        values.push((format!("{prefix}.name"), text(&tab.name)));
        values.push((
            format!("{prefix}.active"),
            VariableValue::Boolean(is_active),
        ));
        values.push((format!("{prefix}.color"), color(is_active)));
    }

    values
}

fn countdown(state: &State, now: u64) -> [(String, VariableValue); 2] {
    let remaining = state.next_rotation_at.map(|next| next.saturating_sub(now));
    let label = match remaining {
        None => String::new(),
        Some(seconds) if seconds < 60 => format!("{seconds}s"),
        Some(seconds) => format!("{}:{:02}", seconds / 60, seconds % 60),
    };
    [
        (
            "seconds_to_next_tab".to_string(),
            VariableValue::Number(remaining.unwrap_or_default() as f64),
        ),
        ("next_tab_in".to_string(), VariableValue::Text(label)),
    ]
}

fn text(value: &str) -> VariableValue {
    VariableValue::Text(value.to_string())
}

/// Empty rather than a neutral colour: an unresolvable colour leaves a key unbordered, which is how
/// "not this one" is spelled in the render path.
fn color(is_active: bool) -> VariableValue {
    text(if is_active { ACTIVE_COLOR } else { "" })
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

pub struct Shared {
    key: Option<String>,
    http: reqwest::Client,
    pub view: Mutex<View>,
}

impl Shared {
    pub fn new(key: Option<String>, http: reqwest::Client) -> Self {
        Self {
            key,
            http,
            view: Mutex::default(),
        }
    }

    fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        match &self.key {
            Some(key) => request.bearer_auth(key),
            None => request,
        }
    }

    pub async fn post(&self, segments: &[&str]) -> Result<(), PluginError> {
        let api = self.view.lock().unwrap().api.clone().ok_or_else(|| {
            PluginError::Upstream("the display has not been found on the network".to_string())
        })?;
        let response = self
            .authorized(self.http.post(endpoint(&api, segments)))
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| PluginError::Upstream(format!("missiond did not answer: {error}")))?;

        match response.status() {
            status if status.is_success() => Ok(()),
            StatusCode::UNAUTHORIZED => Err(PluginError::Upstream(
                "missiond refused this instance's key".to_string(),
            )),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(PluginError::Upstream(format!(
                    "missiond answered {status}: {}",
                    body.trim()
                )))
            }
        }
    }

    /// Without a request timeout, which would end the stream; only opening it is bounded.
    async fn open_events(&self, api: &Url) -> Result<Response, String> {
        let request = self
            .authorized(self.http.get(endpoint(api, &["events"])))
            .header(ACCEPT, "text/event-stream")
            .send();
        timeout(REQUEST_TIMEOUT, request)
            .await
            .map_err(|_| "the event stream did not open in time".to_string())?
            .and_then(Response::error_for_status)
            .map_err(|error| error.to_string())
    }
}

/// Segments are pushed rather than formatted into a string, so an id is percent-encoded instead of
/// reshaping the path.
fn endpoint(api: &Url, segments: &[&str]) -> Url {
    let mut url = api.clone();
    url.path_segments_mut()
        .expect("an api address is always http or https")
        .extend(segments);
    url
}

enum Closed {
    Cancelled,
    /// The connection was up and then ended, so the next attempt starts from the shortest delay.
    Dropped(String),
}

pub async fn run(
    context: PluginContext,
    shared: Arc<Shared>,
    target: Target,
    discovery: Option<Arc<Discovery>>,
) {
    let mut retry_in = FIRST_RETRY;
    loop {
        let api = match (&target, &discovery) {
            (Target::Address(api), _) => api.clone(),
            (Target::Discovered(device_id), Some(discovery)) => {
                match locate(&context, discovery, device_id).await {
                    Some(api) => api,
                    None => return,
                }
            }
            _ => return,
        };
        shared.view.lock().unwrap().api = Some(api.clone());

        let outcome = tokio::select! {
            _ = context.cancel.cancelled() => Ok(Closed::Cancelled),
            outcome = serve(&context, &shared, &api) => outcome,
        };
        update(&context, &shared, |view| view.is_connected = false);

        match outcome {
            Ok(Closed::Cancelled) => return,
            Ok(Closed::Dropped(reason)) => {
                retry_in = FIRST_RETRY;
                context.log(
                    SurfaceLogLevel::Warning,
                    format!("lost {api} ({reason}), reconnecting"),
                );
            }
            Err(reason) => context.log(
                SurfaceLogLevel::Warning,
                format!(
                    "{api} is not answering ({reason}), retrying in {}s",
                    retry_in.as_secs()
                ),
            ),
        }

        tokio::select! {
            _ = context.cancel.cancelled() => return,
            _ = tokio::time::sleep(retry_in) => {}
        }
        retry_in = (retry_in * 2).min(LONGEST_RETRY);
    }
}

/// Waits for the display to announce itself. Answered afresh on every attempt, so a display that
/// came back on another address is followed there.
async fn locate(context: &PluginContext, discovery: &Discovery, device_id: &str) -> Option<Url> {
    let mut displays = discovery.subscribe();
    let mut is_reported = false;
    loop {
        let found = displays
            .borrow_and_update()
            .values()
            .find(|display| display.device_id == device_id)
            .map(|display| display.api.clone());
        if found.is_some() {
            return found;
        }
        if !is_reported {
            is_reported = true;
            context.log(
                SurfaceLogLevel::Info,
                format!("waiting for {device_id} to announce itself on the network"),
            );
        }
        tokio::select! {
            _ = context.cancel.cancelled() => return None,
            changed = displays.changed() => changed.ok()?,
        }
    }
}

async fn serve(context: &PluginContext, shared: &Shared, api: &Url) -> Result<Closed, String> {
    let mut events = shared.open_events(api).await?;
    let mut decoder = EventStream::default();
    let mut is_announced = false;
    let mut ticker = interval(COUNTDOWN_TICK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticker.tick() => publish_countdown(context, shared),
            chunk = timeout(STREAM_SILENCE, events.chunk()) => {
                let chunk = match chunk {
                    Ok(Ok(Some(chunk))) => chunk,
                    Ok(Ok(None)) => return Ok(Closed::Dropped("the event stream ended".to_string())),
                    Ok(Err(error)) => return Ok(Closed::Dropped(error.to_string())),
                    Err(_) => return Ok(Closed::Dropped("the event stream went silent".to_string())),
                };
                for frame in decoder.push(&chunk) {
                    receive(context, shared, &frame, &mut is_announced);
                }
            }
        }
    }
}

fn receive(context: &PluginContext, shared: &Shared, frame: &Frame, is_announced: &mut bool) {
    let understood = match frame.event.as_str() {
        "state" => serde_json::from_str::<State>(&frame.data).map(|state| {
            if !*is_announced {
                *is_announced = true;
                announce(context, &state);
            }
            update(context, shared, |view| {
                view.is_connected = true;
                view.state = Some(state);
            });
        }),
        "catalogue" => serde_json::from_str::<Catalogue>(&frame.data).map(|catalogue| {
            update(context, shared, |view| view.playlists = catalogue.playlists);
            let offered = presets::from_view(&shared.view.lock().unwrap());
            context.set_presets(offered);
        }),
        _ => Ok(()),
    };
    if let Err(error) = understood {
        context.log(
            SurfaceLogLevel::Warning,
            format!(
                "missiond sent a {} frame this plugin does not understand: {error}",
                frame.event
            ),
        );
    }
}

fn announce(context: &PluginContext, state: &State) {
    context.log(
        SurfaceLogLevel::Info,
        format!("connected to {}", state.device_name),
    );
    if state.requires_auth && state.access == Access::None {
        context.log(
            SurfaceLogLevel::Warning,
            "missiond did not accept this instance's key, so its state is shown but every action \
             will be refused",
        );
    }
}

fn update(context: &PluginContext, shared: &Shared, change: impl FnOnce(&mut View)) {
    let (published, withdrawn) = {
        let mut view = shared.view.lock().unwrap();
        change(&mut view);
        let published = values(&view, unix_now());
        let names: HashSet<String> = published.iter().map(|(name, _)| name.clone()).collect();
        let withdrawn: Vec<String> = view.published.difference(&names).cloned().collect();
        view.published = names;
        (published, withdrawn)
    };
    for name in withdrawn {
        context.clear_value(name);
    }
    for (name, value) in published {
        context.set_value(name, value);
    }
}

fn publish_countdown(context: &PluginContext, shared: &Shared) {
    let Some(values) = shared
        .view
        .lock()
        .unwrap()
        .state
        .as_ref()
        .map(|state| countdown(state, unix_now()))
    else {
        return;
    };
    for (name, value) in values {
        context.set_value(name, value);
    }
}

#[derive(Debug, PartialEq)]
struct Frame {
    event: String,
    data: String,
}

/// The frames of a server-sent event stream. Comments, which is how missiond keeps the stream
/// alive, and the `id` and `retry` fields are skipped.
#[derive(Default)]
struct EventStream {
    pending: Vec<u8>,
    event: String,
    data: String,
}

impl EventStream {
    /// A line split across chunks waits in `pending` for the rest of it.
    fn push(&mut self, chunk: &[u8]) -> Vec<Frame> {
        self.pending.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);

            if line.is_empty() {
                let event = std::mem::take(&mut self.event);
                if !self.data.is_empty() {
                    frames.push(Frame {
                        event: if event.is_empty() {
                            "message".to_string()
                        } else {
                            event
                        },
                        data: std::mem::take(&mut self.data),
                    });
                }
                continue;
            }
            let (field, value) = line.split_once(':').unwrap_or((line, ""));
            let value = value.strip_prefix(' ').unwrap_or(value);
            match field {
                "event" => value.clone_into(&mut self.event),
                "data" => {
                    if !self.data.is_empty() {
                        self.data.push('\n');
                    }
                    self.data.push_str(value);
                }
                _ => {}
            }
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_split_across_chunks_is_read_once_it_is_complete() {
        let mut stream = EventStream::default();

        assert!(stream
            .push(b": keep-alive\n\nevent: state\ndata: {\"screen")
            .is_empty());
        assert_eq!(
            stream.push(b"_on\":true}\r\n\r\ndata: 2\n\n"),
            vec![
                Frame {
                    event: "state".to_string(),
                    data: "{\"screen_on\":true}".to_string()
                },
                Frame {
                    event: "message".to_string(),
                    data: "2".to_string()
                },
            ]
        );
    }
}
