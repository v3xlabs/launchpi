use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{atomic::AtomicU64, Arc, Mutex, RwLock},
};

use tokio::sync::{broadcast, mpsc};
use tracing::warn;

use crate::{
    events::ServerEvent,
    panels::{control::Control, Panel},
    plugins::engine::{InputEvent, INPUT_QUEUE_SIZE},
    rendering::ledger::RenderLedger,
    surfaces::{
        connection::ActiveConnection,
        defaults::{default_device, repair_panel_assignments},
        keys::SurfaceKeyEvent,
        logs::SurfaceLogEntry,
        managed::{DiscoveredNetworkSurface, ManagedNetworkSurface},
    },
    variables::VariableStore,
};

pub struct SurfaceRegistry {
    pub(super) discovered: RwLock<HashMap<String, DiscoveredNetworkSurface>>,
    pub(super) managed: RwLock<HashMap<String, ManagedNetworkSurface>>,
    pub(super) panels: RwLock<HashMap<String, Panel>>,
    pub(super) active_connections: RwLock<HashMap<String, ActiveConnection>>,
    pub(super) key_states: RwLock<HashMap<(String, u8), bool>>,
    pub(super) pressed_controls: RwLock<HashMap<(String, u8), Control>>,
    pub(super) dismissed_overlay_keys: RwLock<HashSet<(String, u8)>>,
    pub(super) recent_key_events: RwLock<VecDeque<SurfaceKeyEvent>>,
    /// Lit ring segments per dial while a surface is connected, keyed by (surface, dial index).
    /// Absent means "wherever the active panel says the dial starts".
    pub(super) dial_positions: RwLock<HashMap<(String, u8), u8>>,
    pub(super) dial_presses: RwLock<HashMap<(String, u8), bool>>,
    pub(super) dock_children: RwLock<HashMap<String, Vec<String>>>,
    /// The newest `SURFACE_LOG_CAPACITY` lines per surface, oldest first.
    pub(super) logs: RwLock<HashMap<String, VecDeque<SurfaceLogEntry>>>,
    pub(super) rendered: RenderLedger,
    /// Live plugin state the render path resolves against.
    pub(super) variables: Arc<VariableStore>,
    /// Gestures on their way to the action engine. Separate from `events` because that broadcast
    /// drops on lag, and a dropped action is not the same kind of loss as a dropped repaint.
    input: mpsc::Sender<InputEvent>,
    input_receiver: Mutex<Option<mpsc::Receiver<InputEvent>>>,
    events: broadcast::Sender<ServerEvent>,
    pub(super) next_surface_number: AtomicU64,
    pub(super) next_panel_number: AtomicU64,
    pub(super) next_log_sequence: AtomicU64,
}

impl SurfaceRegistry {
    pub fn from_configuration(
        mut devices: Vec<ManagedNetworkSurface>,
        mut panels: Vec<Panel>,
    ) -> Self {
        let default_panel_id = panels.first().map(|panel| panel.panel_id.clone());
        let mut seen_endpoints = std::collections::HashSet::new();
        devices.retain(|device| seen_endpoints.insert((device.host.clone(), device.port)));
        if !devices.iter().any(|device| device.is_enabled) {
            devices.push(default_device(&devices, default_panel_id));
        }
        repair_panel_assignments(&mut devices, &mut panels);
        let next_surface_number = devices
            .iter()
            .filter_map(|device| {
                device
                    .surface_id
                    .0
                    .strip_prefix("stream-deck-studio-")?
                    .parse()
                    .ok()
            })
            .max()
            .unwrap_or(0);
        let next_panel_number = panels
            .iter()
            .filter_map(|panel| panel.panel_id.0.strip_prefix("studio-panel-")?.parse().ok())
            .max()
            .unwrap_or(0);
        let (input, input_receiver) = mpsc::channel(INPUT_QUEUE_SIZE);
        Self {
            discovered: RwLock::default(),
            managed: RwLock::new(
                devices
                    .into_iter()
                    .map(|device| (device.surface_id.0.clone(), device))
                    .collect(),
            ),
            panels: RwLock::new(
                panels
                    .into_iter()
                    .map(|panel| (panel.panel_id.0.clone(), panel))
                    .collect(),
            ),
            active_connections: RwLock::default(),
            key_states: RwLock::default(),
            pressed_controls: RwLock::default(),
            dismissed_overlay_keys: RwLock::default(),
            recent_key_events: RwLock::default(),
            dial_positions: RwLock::default(),
            dial_presses: RwLock::default(),
            dock_children: RwLock::default(),
            logs: RwLock::default(),
            rendered: RenderLedger::default(),
            variables: Arc::default(),
            input,
            input_receiver: Mutex::new(Some(input_receiver)),
            events: broadcast::channel(256).0,
            next_surface_number: AtomicU64::new(next_surface_number),
            next_panel_number: AtomicU64::new(next_panel_number),
            next_log_sequence: AtomicU64::new(0),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.events.subscribe()
    }

    pub fn variables(&self) -> Arc<VariableStore> {
        self.variables.clone()
    }


    /// Handed to the action engine at startup. Until something takes it the receiver stays here,
    /// so input queues rather than failing to send.
    pub fn take_input_receiver(&self) -> Option<mpsc::Receiver<InputEvent>> {
        self.input_receiver.lock().unwrap().take()
    }

    pub(super) fn emit(&self, event: ServerEvent) {
        let _ = self.events.send(event);
    }

    /// Lets the plugin engine put its own events on the same stream the web already watches.
    pub fn emit_event(&self, event: ServerEvent) {
        self.emit(event);
    }

    pub(super) fn dispatch_input(&self, event: InputEvent) {
        if let Err(mpsc::error::TrySendError::Full(_)) = self.input.try_send(event) {
            warn!(
                capacity = INPUT_QUEUE_SIZE,
                "input queue is full, dropped a gesture; bound actions did not run"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{identifiers::SurfaceId, surfaces::defaults::default_panel};

    #[test]
    fn creates_a_studio_device_with_the_hello_panel_active() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let device = registry
            .managed(&SurfaceId("stream-deck-studio-1".to_string()))
            .expect("default device should exist");

        assert_eq!(
            device.active_panel_id.as_ref().map(|id| id.0.as_str()),
            Some("studio-panel-1")
        );
        assert_eq!(
            registry.active_key_renderings(&device.surface_id)[0]
                .layers
                .iter()
                .find_map(|layer| match layer {
                    crate::panels::rendered_state::ResolvedLayer::Text { text, .. } =>
                        Some(text.as_str()),
                    _ => None,
                }),
            Some("Hello")
        );
    }
}
