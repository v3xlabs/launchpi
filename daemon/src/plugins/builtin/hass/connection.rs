use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        RwLock,
    },
    time::Duration,
};

use futures::{Sink, SinkExt, Stream, StreamExt};
use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, oneshot, Notify};
use tokio_tungstenite::tungstenite::Message;
use tracing::debug;

use crate::{
    plugins::{
        builtin::hass::{
            protocol::{self, ServerMessage, ServiceCall},
            values::{entity_id_of, parse_binding, value_for_field, ValueBinding},
        },
        plugin::{LookupOption, PluginContext, Subscription},
    },
    surfaces::logs::SurfaceLogLevel,
};

const FIRST_RETRY: Duration = Duration::from_secs(1);
const LONGEST_RETRY: Duration = Duration::from_secs(60);
/// Home Assistant closes an idle socket, and a socket the network dropped only reveals itself on a
/// write, so something has to be written even when nothing is happening.
const KEEPALIVE: Duration = Duration::from_secs(30);

/// What the plugin and its connection share: the bindings the connection publishes into, the queue
/// it takes commands from, and enough state for `invoke` to answer without a socket.
#[derive(Clone, Debug)]
pub struct CatalogueEntry {
    pub friendly_name: Option<String>,
    pub domain: String,
}

pub struct Shared {
    pub bindings: RwLock<Vec<ValueBinding>>,
    /// Every entity the seed reported, so the UI can offer real names instead of asking someone to
    /// remember `light.kitchen_ceiling_2`. Ordered, because it is read straight into a picker.
    pub catalogue: RwLock<BTreeMap<String, CatalogueEntry>>,
    pub commands: mpsc::Sender<PendingCommand>,
    pub reseed: Notify,
    pub is_connected: AtomicBool,
}

impl Shared {
    pub fn new(commands: mpsc::Sender<PendingCommand>) -> Self {
        Self {
            bindings: RwLock::new(Vec::new()),
            catalogue: RwLock::new(BTreeMap::new()),
            commands,
            reseed: Notify::new(),
            is_connected: AtomicBool::new(false),
        }
    }

    /// Remembers what an entity is called and what kind of thing it is. Every state that arrives
    /// updates it, so the picker reflects the installation as it is now.
    pub fn record(&self, entity_id: &str, state: &JsonValue) {
        let friendly_name = state
            .get("attributes")
            .and_then(|attributes| attributes.get("friendly_name"))
            .and_then(JsonValue::as_str)
            .map(str::to_string);
        let domain = entity_id
            .split_once('.')
            .map(|(domain, _)| domain.to_string())
            .unwrap_or_default();

        self.catalogue.write().unwrap().insert(
            entity_id.to_string(),
            CatalogueEntry {
                friendly_name,
                domain,
            },
        );
    }

    /// The entity list as lookup options: friendly name first because that is what a person knows,
    /// the id alongside it because that is what the binding actually stores.
    pub fn entity_options(&self) -> Vec<LookupOption> {
        self.catalogue
            .read()
            .unwrap()
            .iter()
            .map(|(entity_id, entry)| LookupOption {
                value: entity_id.clone(),
                label: match &entry.friendly_name {
                    Some(name) => format!("{name} ({entity_id})"),
                    None => entity_id.clone(),
                },
                group: Some(entry.domain.clone()),
            })
            .collect()
    }

    /// Replaces the watched set. Names that do not read like an entity are dropped here, so the
    /// connection never carries a binding it cannot answer.
    pub fn watch(&self, subscriptions: &[Subscription]) {
        let bindings = subscriptions
            .iter()
            .filter_map(|subscription| parse_binding(&subscription.name))
            .collect();
        *self.bindings.write().unwrap() = bindings;
        self.reseed.notify_one();
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::Relaxed)
    }
}

/// A command waiting for its `result` frame. The id is assigned by the connection because Home
/// Assistant requires ids to increase within one socket, and a reconnect starts them again.
pub struct PendingCommand {
    pub call: ServiceCall,
    pub respond: oneshot::Sender<Result<JsonValue, String>>,
}

enum Closed {
    Cancelled,
    Dropped,
}

enum Failure {
    /// The installation is not reachable right now. Worth retrying.
    Unreachable(String),
    /// The token will not become valid by trying again.
    Rejected(String),
}

pub async fn run(
    context: PluginContext,
    url: String,
    token: String,
    shared: std::sync::Arc<Shared>,
    mut commands: mpsc::Receiver<PendingCommand>,
) {
    let mut retry_in = FIRST_RETRY;
    loop {
        let outcome = serve(&context, &url, &token, &shared, &mut commands).await;
        shared.is_connected.store(false, Ordering::Relaxed);

        match outcome {
            Ok(Closed::Cancelled) => return,
            Ok(Closed::Dropped) => {
                retry_in = FIRST_RETRY;
                context.log(
                    SurfaceLogLevel::Warning,
                    format!("{url} closed the connection, reconnecting"),
                );
            }
            Err(Failure::Rejected(reason)) => {
                context.log(
                    SurfaceLogLevel::Warning,
                    format!("{url} rejected the access token: {reason}"),
                );
                return;
            }
            Err(Failure::Unreachable(reason)) => context.log(
                SurfaceLogLevel::Warning,
                format!(
                    "{url} is not answering ({reason}), retrying in {}s",
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

async fn serve(
    context: &PluginContext,
    url: &str,
    token: &str,
    shared: &Shared,
    commands: &mut mpsc::Receiver<PendingCommand>,
) -> Result<Closed, Failure> {
    let (socket, _) = tokio::select! {
        _ = context.cancel.cancelled() => return Ok(Closed::Cancelled),
        connected = tokio_tungstenite::connect_async(url) => {
            connected.map_err(|error| Failure::Unreachable(error.to_string()))?
        }
    };
    let (mut writer, mut reader) = socket.split();

    tokio::select! {
        _ = context.cancel.cancelled() => return Ok(Closed::Cancelled),
        authenticated = authenticate(&mut reader, &mut writer, token) => authenticated?,
    }

    let mut next_id = 1_u64;
    let mut pending: HashMap<u64, oneshot::Sender<Result<JsonValue, String>>> = HashMap::new();
    let mut seeds: HashSet<u64> = HashSet::new();

    send(
        &mut writer,
        protocol::subscribe_state_changed(take(&mut next_id)),
    )
    .await?;
    let seed = take(&mut next_id);
    seeds.insert(seed);
    send(&mut writer, protocol::get_states(seed)).await?;

    shared.is_connected.store(true, Ordering::Relaxed);
    context.log(SurfaceLogLevel::Info, format!("connected to {url}"));

    let mut keepalive = tokio::time::interval(KEEPALIVE);
    keepalive.tick().await;

    loop {
        tokio::select! {
            _ = context.cancel.cancelled() => {
                let _ = writer.close().await;
                return Ok(Closed::Cancelled);
            }
            _ = shared.reseed.notified() => {
                let seed = take(&mut next_id);
                seeds.insert(seed);
                send(&mut writer, protocol::get_states(seed)).await?;
            }
            command = commands.recv() => {
                let Some(command) = command else { return Ok(Closed::Cancelled) };
                let id = take(&mut next_id);
                pending.insert(id, command.respond);
                send(&mut writer, command.call.message(id)).await?;
            }
            _ = keepalive.tick() => {
                writer
                    .send(Message::Ping(Vec::new()))
                    .await
                    .map_err(|error| Failure::Unreachable(error.to_string()))?;
            }
            frame = reader.next() => {
                let Some(frame) = frame else { return Ok(Closed::Dropped) };
                match frame.map_err(|error| Failure::Unreachable(error.to_string()))? {
                    Message::Text(payload) => {
                        receive(context, shared, &payload, &mut pending, &mut seeds);
                    }
                    Message::Close(_) => return Ok(Closed::Dropped),
                    _ => {}
                }
            }
        }
    }
}

async fn authenticate<R, W>(reader: &mut R, writer: &mut W, token: &str) -> Result<(), Failure>
where
    R: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    W: Sink<Message> + Unpin,
    W::Error: std::fmt::Display,
{
    loop {
        match read(reader).await? {
            ServerMessage::AuthRequired => send(writer, protocol::auth(token)).await?,
            ServerMessage::AuthOk => return Ok(()),
            ServerMessage::AuthInvalid { message } => return Err(Failure::Rejected(message)),
            _ => {}
        }
    }
}

fn receive(
    context: &PluginContext,
    shared: &Shared,
    payload: &str,
    pending: &mut HashMap<u64, oneshot::Sender<Result<JsonValue, String>>>,
    seeds: &mut HashSet<u64>,
) {
    match protocol::parse_server_message(payload) {
        Ok(ServerMessage::State(state)) => publish(context, shared, &state),
        Ok(ServerMessage::Result { id, outcome }) => {
            if let Some(responder) = pending.remove(&id) {
                let _ = responder.send(outcome);
                return;
            }
            match (seeds.remove(&id), outcome) {
                (true, Ok(result)) => {
                    for state in protocol::states_of(&result) {
                        publish(context, shared, &state);
                    }
                }
                (_, Err(reason)) => context.log(SurfaceLogLevel::Warning, reason),
                (false, Ok(_)) => {}
            }
        }
        Ok(_) => {}
        Err(reason) => debug!(
            integration_id = context.integration_id.0,
            reason, "dropped an unreadable frame"
        ),
    }
}

/// Publishes every subscribed value that reads from this entity. Nothing else in the installation
/// is looked at, so a panel watching four lights costs four lookups per event.
fn publish(context: &PluginContext, shared: &Shared, state: &JsonValue) {
    let Some(entity_id) = entity_id_of(state) else {
        return;
    };
    shared.record(&entity_id, state);
    let bindings = shared.bindings.read().unwrap();
    for binding in bindings
        .iter()
        .filter(|binding| binding.entity_id == entity_id)
    {
        if let Some(value) = value_for_field(state, &binding.field) {
            context.set_value(binding.name.clone(), value);
        }
    }
}

fn take(next_id: &mut u64) -> u64 {
    let id = *next_id;
    *next_id += 1;
    id
}

async fn send<W>(writer: &mut W, message: JsonValue) -> Result<(), Failure>
where
    W: Sink<Message> + Unpin,
    W::Error: std::fmt::Display,
{
    writer
        .send(Message::Text(message.to_string()))
        .await
        .map_err(|error| Failure::Unreachable(error.to_string()))
}

async fn read<R>(reader: &mut R) -> Result<ServerMessage, Failure>
where
    R: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let Some(frame) = reader.next().await else {
            return Err(Failure::Unreachable(
                "the connection closed during the handshake".to_string(),
            ));
        };
        match frame.map_err(|error| Failure::Unreachable(error.to_string()))? {
            Message::Text(payload) => {
                return protocol::parse_server_message(&payload).map_err(Failure::Unreachable)
            }
            Message::Close(_) => {
                return Err(Failure::Unreachable(
                    "the connection closed during the handshake".to_string(),
                ))
            }
            _ => {}
        }
    }
}
