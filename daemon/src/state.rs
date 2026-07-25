use std::{
    collections::{
        hash_map::DefaultHasher,
        {HashMap, VecDeque},
    },
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
};

use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

use crate::{
    models::{
        control::Control,
        identifiers::{ControlId, PanelId, SurfaceId},
        network_surface::{
            DeviceInventory, DiscoveredNetworkSurface, KeyRendering, ManagedNetworkSurface,
            NetworkSurfaceStatus, ServerEvent, SurfaceCommand, SurfaceDialPress, SurfaceDialState,
            SurfaceKeyEvent, SurfaceLogEntry, SurfaceLogLevel, DIAL_COUNT, DIAL_RING_SEGMENTS,
        },
        panel::{Panel, PanelLayout},
        rendered_state::{RenderedState, RgbaColor},
        surface::{SurfaceCapabilities, SurfaceLayout, SurfacePosition},
    },
    persistence::Persistence,
    plugins::{
        config::PluginDirectory,
        engine::{InputEvent, PluginEngine, INPUT_QUEUE_SIZE},
        feedback::FeedbackCache,
        render::RenderContext,
        variables::VariableStore,
    },
};

/// How many pending renders a surface can queue before the daemon starts dropping them.
const SURFACE_COMMAND_QUEUE_SIZE: usize = 64;
/// How many log lines a surface keeps. Enough to cover a burst of dial turns and still show what
/// came before it.
const SURFACE_LOG_CAPACITY: usize = 400;

fn status_label(status: &NetworkSurfaceStatus) -> &'static str {
    match status {
        NetworkSurfaceStatus::Connecting => "connecting",
        NetworkSurfaceStatus::Connected => "connected",
        NetworkSurfaceStatus::Unavailable => "unavailable",
        NetworkSurfaceStatus::Disabled => "disabled",
    }
}

fn unix_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[derive(Clone)]
pub struct AppState {
    pub surfaces: Arc<SurfaceRegistry>,
    pub plugins: Arc<PluginEngine>,
    persistence: Arc<Persistence>,
}

impl AppState {
    pub async fn load() -> anyhow::Result<Self> {
        let (persistence, devices, mut panels) = Persistence::open().await?;
        if panels.is_empty() {
            panels.push(default_panel());
        }
        let surfaces = Arc::new(SurfaceRegistry::from_configuration(devices, panels));
        let directory = PluginDirectory::open(&crate::persistence::config_directory()?)?;
        let input = surfaces
            .take_input_receiver()
            .expect("the input receiver has not been taken yet");
        let plugins = PluginEngine::start(
            surfaces.clone(),
            surfaces.variables(),
            surfaces.feedbacks(),
            directory,
            input,
        )
        .await;
        Ok(Self {
            surfaces,
            plugins,
            persistence: Arc::new(persistence),
        })
    }

    pub fn persist_configuration(&self) -> anyhow::Result<()> {
        let devices = self
            .surfaces
            .managed_surfaces()
            .into_iter()
            .filter(|device| device.parent_surface_id.is_none())
            .collect();
        self.persistence
            .save_configuration(devices, self.surfaces.panels())
    }

    pub fn export_panel_configuration(&self, panel_id: &str) -> anyhow::Result<Option<String>> {
        let Some(panel) = self.surfaces.panel(panel_id) else {
            return Ok(None);
        };
        self.persistence.render_panel(panel).map(Some)
    }

    pub fn update_status(
        &self,
        surface_id: &SurfaceId,
        status: NetworkSurfaceStatus,
        last_error: Option<String>,
    ) {
        self.surfaces
            .update_status(surface_id, status.clone(), last_error.clone());
        let persistence = self.persistence.clone();
        let surface_id = surface_id.0.clone();
        tokio::spawn(async move {
            let _ = persistence
                .record_status(surface_id, status, last_error)
                .await;
        });
    }
}

pub struct SurfaceRegistry {
    discovered: RwLock<HashMap<String, DiscoveredNetworkSurface>>,
    managed: RwLock<HashMap<String, ManagedNetworkSurface>>,
    panels: RwLock<HashMap<String, Panel>>,
    active_connections: RwLock<HashMap<String, ActiveConnection>>,
    key_states: RwLock<HashMap<(String, u8), bool>>,
    recent_key_events: RwLock<VecDeque<SurfaceKeyEvent>>,
    /// Lit ring segments per dial while a surface is connected, keyed by (surface, dial index).
    /// Absent means "wherever the active panel says the dial starts".
    dial_positions: RwLock<HashMap<(String, u8), u8>>,
    dial_presses: RwLock<HashMap<(String, u8), bool>>,
    dock_children: RwLock<HashMap<String, Vec<String>>>,
    /// The newest `SURFACE_LOG_CAPACITY` lines per surface, oldest first.
    logs: RwLock<HashMap<String, VecDeque<SurfaceLogEntry>>>,
    /// What each key was last told to show. A repaint that resolves to the same thing costs a hash
    /// lookup instead of a JPEG encode, which is what makes a live variable affordable.
    last_rendered: RwLock<HashMap<(String, u8), u64>>,
    /// Live plugin state the render path resolves against.
    variables: Arc<VariableStore>,
    feedbacks: Arc<FeedbackCache>,
    /// Gestures on their way to the action engine. Separate from `events` because that broadcast
    /// drops on lag, and a dropped action is not the same kind of loss as a dropped repaint.
    input: mpsc::Sender<InputEvent>,
    input_receiver: Mutex<Option<mpsc::Receiver<InputEvent>>>,
    events: broadcast::Sender<ServerEvent>,
    next_surface_number: AtomicU64,
    next_panel_number: AtomicU64,
    next_log_sequence: AtomicU64,
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
            recent_key_events: RwLock::default(),
            dial_positions: RwLock::default(),
            dial_presses: RwLock::default(),
            dock_children: RwLock::default(),
            logs: RwLock::default(),
            last_rendered: RwLock::default(),
            variables: Arc::default(),
            feedbacks: Arc::default(),
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

    pub fn feedbacks(&self) -> Arc<FeedbackCache> {
        self.feedbacks.clone()
    }

    /// Handed to the action engine at startup. Until something takes it the receiver stays here,
    /// so input queues rather than failing to send.
    pub fn take_input_receiver(&self) -> Option<mpsc::Receiver<InputEvent>> {
        self.input_receiver.lock().unwrap().take()
    }

    fn render_context(&self) -> RenderContext<'_> {
        RenderContext::new(&self.variables, &self.feedbacks)
    }

    /// The control a key belongs to on whatever panel the surface is showing.
    pub fn control_at(&self, surface_id: &SurfaceId, key_index: u8) -> Option<Control> {
        let device = self.managed(surface_id)?;
        let panel = self.panel(&device.active_panel_id?.0)?;
        panel
            .controls
            .into_iter()
            .find(|control| key_index_for(control, panel.layout.columns) == Some(key_index))
    }

    /// Re-resolves one key against current plugin state and pushes it if anything changed.
    pub fn refresh_key(&self, surface_id: &SurfaceId, key_index: u8) {
        let is_pressed = self
            .key_states
            .read()
            .unwrap()
            .get(&(surface_id.0.clone(), key_index))
            .copied()
            .unwrap_or(false);
        if let Some(rendering) = self.rendering_for_key(surface_id, key_index, is_pressed) {
            self.send_rendering(surface_id, rendering);
        }
    }

    fn emit(&self, event: ServerEvent) {
        let _ = self.events.send(event);
    }

    /// Appends one line to a surface's log and streams it to anyone watching the device page.
    pub fn log(&self, surface_id: &SurfaceId, level: SurfaceLogLevel, message: String) {
        let entry = SurfaceLogEntry {
            surface_id: surface_id.clone(),
            sequence: self.next_log_sequence.fetch_add(1, Ordering::Relaxed),
            at_ms: unix_epoch_ms(),
            level,
            message,
        };
        {
            let mut logs = self.logs.write().unwrap();
            let entries = logs.entry(surface_id.0.clone()).or_default();
            if entries.len() == SURFACE_LOG_CAPACITY {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }
        self.emit(ServerEvent::Log(entry));
    }

    pub fn inventory(&self) -> DeviceInventory {
        let mut discovered: Vec<_> = self.discovered.read().unwrap().values().cloned().collect();
        let mut devices: Vec<_> = self.managed.read().unwrap().values().cloned().collect();
        let mut panels: Vec<_> = self.panels.read().unwrap().values().cloned().collect();
        let recent_key_events = self
            .recent_key_events
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        let key_states = self
            .key_states
            .read()
            .unwrap()
            .iter()
            .filter(|(_, is_pressed)| **is_pressed)
            .map(|((surface_id, key_index), is_pressed)| SurfaceKeyEvent {
                surface_id: SurfaceId(surface_id.clone()),
                key_index: *key_index,
                is_pressed: *is_pressed,
            })
            .collect();
        let dial_states = self
            .dial_positions
            .read()
            .unwrap()
            .iter()
            .map(
                |((surface_id, dial_index), lit_segments)| SurfaceDialState {
                    surface_id: SurfaceId(surface_id.clone()),
                    dial_index: *dial_index,
                    level: percent_from_segments(*lit_segments),
                },
            )
            .collect();
        let dial_presses = self
            .dial_presses
            .read()
            .unwrap()
            .iter()
            .filter(|(_, is_pressed)| **is_pressed)
            .map(|((surface_id, dial_index), is_pressed)| SurfaceDialPress {
                surface_id: SurfaceId(surface_id.clone()),
                dial_index: *dial_index,
                is_pressed: *is_pressed,
            })
            .collect();
        let mut logs: Vec<_> = self
            .logs
            .read()
            .unwrap()
            .values()
            .flatten()
            .cloned()
            .collect();
        logs.sort_by_key(|entry| entry.sequence);
        discovered.sort_by(|left, right| left.name.cmp(&right.name));
        devices.sort_by(|left, right| left.name.cmp(&right.name));
        panels.sort_by(|left, right| left.name.cmp(&right.name));
        DeviceInventory {
            discovered,
            devices,
            panels,
            recent_key_events,
            key_states,
            dial_states,
            dial_presses,
            logs,
        }
    }

    pub fn upsert_discovered(&self, surface: DiscoveredNetworkSurface) {
        self.discovered
            .write()
            .unwrap()
            .insert(surface.discovery_id.clone(), surface);
        self.emit(ServerEvent::Changed);
    }
    pub fn remove_discovered(&self, discovery_id: &str) {
        self.discovered.write().unwrap().remove(discovery_id);
        self.emit(ServerEvent::Changed);
    }
    pub fn discovered(&self, discovery_id: &str) -> Option<DiscoveredNetworkSurface> {
        self.discovered.read().unwrap().get(discovery_id).cloned()
    }
    pub fn add_managed(&self, surface: ManagedNetworkSurface) -> ManagedNetworkSurface {
        self.managed
            .write()
            .unwrap()
            .insert(surface.surface_id.0.clone(), surface.clone());
        self.emit(ServerEvent::Changed);
        surface
    }
    pub fn add_managed_child(
        &self,
        parent_id: &SurfaceId,
        surface: ManagedNetworkSurface,
    ) -> ManagedNetworkSurface {
        self.dock_children
            .write()
            .unwrap()
            .entry(parent_id.0.clone())
            .or_default()
            .push(surface.surface_id.0.clone());
        self.add_managed(surface)
    }
    pub fn deactivate_children_of(&self, parent_id: &SurfaceId) {
        let children = self
            .dock_children
            .write()
            .unwrap()
            .remove(&parent_id.0)
            .unwrap_or_default();
        let had_children = !children.is_empty();
        for child in children {
            self.deactivate(&child);
            self.managed.write().unwrap().remove(&child);
        }
        if had_children {
            self.emit(ServerEvent::Changed);
        }
    }
    pub fn set_identity(&self, surface_id: &SurfaceId, model: String, layout: SurfaceLayout) {
        let changed = {
            let mut managed = self.managed.write().unwrap();
            match managed.get_mut(&surface_id.0) {
                Some(surface) => {
                    let changed = surface.model != model || surface.layout != layout;
                    surface.model = model;
                    surface.layout = layout;
                    changed
                }
                None => false,
            }
        };
        if changed {
            self.emit(ServerEvent::Changed);
        }
    }
    pub fn ensure_default_panel_for_layout(&self, layout: &SurfaceLayout) -> Option<PanelId> {
        let SurfaceLayout::Grid { columns, rows } = layout else {
            return None;
        };
        let (columns, rows) = (*columns, *rows);
        if let Some(existing) = self
            .panels()
            .into_iter()
            .find(|panel| panel.layout.columns == columns && panel.layout.rows == rows)
        {
            return Some(existing.panel_id);
        }
        let controls = (0..rows)
            .flat_map(|row| {
                (0..columns).map(move |column| {
                    let index = row * columns + column;
                    Control {
                        control_id: ControlId(format!("auto-{row}-{column}")),
                        name: format!("Key {index}"),
                        position: SurfacePosition { column, row },
                        default_state: RenderedState {
                            text: Some(format!("{index}")),
                            foreground_color: Some(white()),
                            background_color: Some(color(30, 41, 59)),
                            image: None,
                            progress: None,
                            is_pressed: false,
                        },
                        pressed_state: None,
                        action_bindings: Vec::new(),
                        feedback_bindings: Vec::new(),
                    }
                })
            })
            .collect();
        let panel = Panel {
            panel_id: self.create_panel_id(),
            name: format!("Auto {columns}x{rows}"),
            layout: PanelLayout { columns, rows },
            capabilities: studio_capabilities(),
            controls,
            dial_colors: Vec::new(),
            dial_ring_levels: Vec::new(),
        };
        self.upsert_panel(panel).ok().map(|panel| panel.panel_id)
    }
    pub fn managed(&self, surface_id: &SurfaceId) -> Option<ManagedNetworkSurface> {
        self.managed.read().unwrap().get(&surface_id.0).cloned()
    }
    pub fn managed_surfaces(&self) -> Vec<ManagedNetworkSurface> {
        self.managed.read().unwrap().values().cloned().collect()
    }
    pub fn has_managed_endpoint(&self, host: &str, port: u16) -> bool {
        self.managed
            .read()
            .unwrap()
            .values()
            .any(|surface| surface.host == host && surface.port == port)
    }
    pub fn managed_by_endpoint(&self, host: &str, port: u16) -> Option<ManagedNetworkSurface> {
        self.managed
            .read()
            .unwrap()
            .values()
            .find(|surface| surface.host == host && surface.port == port)
            .cloned()
    }
    pub fn create_surface_id(&self) -> SurfaceId {
        SurfaceId(format!(
            "stream-deck-studio-{}",
            self.next_surface_number.fetch_add(1, Ordering::Relaxed) + 1
        ))
    }
    pub fn create_panel_id(&self) -> PanelId {
        PanelId(format!(
            "studio-panel-{}",
            self.next_panel_number.fetch_add(1, Ordering::Relaxed) + 1
        ))
    }
    pub fn panels(&self) -> Vec<Panel> {
        self.panels.read().unwrap().values().cloned().collect()
    }
    pub fn panel(&self, panel_id: &str) -> Option<Panel> {
        self.panels.read().unwrap().get(panel_id).cloned()
    }

    pub fn upsert_panel(&self, panel: Panel) -> Result<Panel, String> {
        validate_panel(&panel)?;
        if self.managed.read().unwrap().values().any(|device| {
            device.active_panel_id.as_ref() == Some(&panel.panel_id)
                && !is_compatible(device, &panel)
        }) {
            return Err("panel is incompatible with a device it is assigned to".to_string());
        }
        self.panels
            .write()
            .unwrap()
            .insert(panel.panel_id.0.clone(), panel.clone());
        let assigned_surface_ids: Vec<_> = self
            .managed
            .read()
            .unwrap()
            .values()
            .filter(|device| device.active_panel_id.as_ref() == Some(&panel.panel_id))
            .map(|device| device.surface_id.0.clone())
            .collect();

        for surface_id in assigned_surface_ids {
            self.render_active_panel(&surface_id);
        }
        self.emit(ServerEvent::Changed);
        Ok(panel)
    }

    pub fn remove_panel(&self, panel_id: &str) -> Option<Panel> {
        let removed = self.panels.write().unwrap().remove(panel_id)?;
        let remaining = self.panels();
        let reassigned: Vec<_> = {
            let mut devices = self.managed.write().unwrap();
            devices
                .values_mut()
                .filter(|device| device.active_panel_id.as_ref() == Some(&removed.panel_id))
                .map(|device| {
                    device.active_panel_id = remaining
                        .iter()
                        .find(|panel| is_compatible(device, panel))
                        .map(|panel| panel.panel_id.clone());
                    device.surface_id.0.clone()
                })
                .collect()
        };
        for surface_id in reassigned {
            self.reset_dial_positions(&surface_id);
            self.render_active_panel(&surface_id);
        }
        self.emit(ServerEvent::Changed);
        Some(removed)
    }

    pub fn assign_active_panel(
        &self,
        surface_id: &str,
        panel_id: &str,
    ) -> Result<ManagedNetworkSurface, String> {
        let panel = self
            .panel(panel_id)
            .ok_or_else(|| "panel was not found".to_string())?;
        let device = {
            let mut devices = self.managed.write().unwrap();
            let device = devices
                .get_mut(surface_id)
                .ok_or_else(|| "managed device was not found".to_string())?;
            if !is_compatible(device, &panel) {
                return Err("panel layout or capabilities are incompatible with device".to_string());
            }
            device.active_panel_id = Some(panel.panel_id);
            device.clone()
        };
        self.reset_dial_positions(surface_id);
        self.render_active_panel(surface_id);
        self.log(
            &device.surface_id,
            SurfaceLogLevel::Info,
            format!("active panel set to {}", panel.name),
        );
        self.emit(ServerEvent::Changed);
        Ok(device)
    }

    pub fn remove_managed(&self, surface_id: &str) -> Option<ManagedNetworkSurface> {
        self.deactivate(surface_id);
        let removed = self.managed.write().unwrap().remove(surface_id);
        if removed.is_some() {
            self.emit(ServerEvent::Changed);
        }
        removed
    }
    pub fn set_enabled(&self, surface_id: &str, is_enabled: bool) -> Option<ManagedNetworkSurface> {
        if !is_enabled {
            self.deactivate(surface_id);
        }
        let surface = {
            let mut managed = self.managed.write().unwrap();
            let surface = managed.get_mut(surface_id)?;
            surface.is_enabled = is_enabled;
            surface.status = if is_enabled {
                NetworkSurfaceStatus::Connecting
            } else {
                NetworkSurfaceStatus::Disabled
            };
            surface.last_error = None;
            surface.clone()
        };
        self.emit(ServerEvent::Changed);
        Some(surface)
    }
    pub fn activate(
        &self,
        surface_id: &SurfaceId,
    ) -> (Arc<AtomicBool>, mpsc::Receiver<SurfaceCommand>) {
        self.deactivate(&surface_id.0);
        let is_active = Arc::new(AtomicBool::new(true));
        let (command_sender, command_receiver) = mpsc::channel(SURFACE_COMMAND_QUEUE_SIZE);
        self.active_connections.write().unwrap().insert(
            surface_id.0.clone(),
            ActiveConnection {
                is_active: is_active.clone(),
                command_sender,
            },
        );
        (is_active, command_receiver)
    }
    pub fn deactivate(&self, surface_id: &str) {
        self.reset_dial_positions(surface_id);
        self.forget_renderings(surface_id);
        if let Some(connection) = self.active_connections.write().unwrap().remove(surface_id) {
            connection.is_active.store(false, Ordering::Release);
        }
    }
    pub fn update_status(
        &self,
        surface_id: &SurfaceId,
        status: NetworkSurfaceStatus,
        last_error: Option<String>,
    ) {
        let changed = {
            let mut managed = self.managed.write().unwrap();
            match managed.get_mut(&surface_id.0) {
                Some(surface) => {
                    let changed = surface.status != status || surface.last_error != last_error;
                    surface.status = status.clone();
                    surface.last_error = last_error.clone();
                    changed
                }
                None => false,
            }
        };
        if changed {
            let label = status_label(&status);
            let (level, message) = match &last_error {
                Some(error) => (SurfaceLogLevel::Warning, format!("{label}: {error}")),
                None => (SurfaceLogLevel::Info, label.to_string()),
            };
            self.log(surface_id, level, message);
            self.emit(ServerEvent::Changed);
        }
    }

    pub fn active_key_renderings(&self, surface_id: &SurfaceId) -> Vec<KeyRendering> {
        let Some(device) = self.managed(surface_id) else {
            return Vec::new();
        };
        let Some(panel_id) = device.active_panel_id else {
            return Vec::new();
        };
        let Some(panel) = self.panel(&panel_id.0) else {
            return Vec::new();
        };
        let context = self.render_context();
        panel
            .controls
            .iter()
            .filter_map(|control| {
                rendering_for_control(control, false, panel.layout.columns, &context)
            })
            .collect()
    }

    /// Every dial the active panel configures, as (index, colour, lit segments).
    pub fn active_dial_rings(&self, surface_id: &SurfaceId) -> Vec<(u8, RgbaColor, u8)> {
        let Some(device) = self.managed(surface_id) else {
            return Vec::new();
        };
        let Some(panel_id) = device.active_panel_id else {
            return Vec::new();
        };
        let Some(panel) = self.panel(&panel_id.0) else {
            return Vec::new();
        };
        panel
            .dial_colors
            .iter()
            .take(usize::from(DIAL_COUNT))
            .enumerate()
            .filter_map(|(index, color)| {
                let dial_index = u8::try_from(index).ok()?;
                Some((
                    dial_index,
                    color.clone(),
                    self.lit_segments(surface_id, dial_index, &panel, index),
                ))
            })
            .collect()
    }

    /// Turning the knob moves the ring one segment per detent, clamped at both ends. The new
    /// position is runtime state: it is pushed back to the device and broadcast, never persisted.
    pub fn record_dial_turn(&self, surface_id: &SurfaceId, dial_index: u8, detents: i8) {
        if dial_index >= DIAL_COUNT || detents == 0 {
            return;
        }
        let Some((color, current)) = self.dial_ring(surface_id, dial_index) else {
            debug!(
                surface_id = surface_id.0,
                dial_index, detents, "ignored a dial turn: the surface has no active panel"
            );
            return;
        };
        let moved = i16::from(current) + i16::from(detents);
        let next = u8::try_from(moved.clamp(0, i16::from(DIAL_RING_SEGMENTS))).unwrap_or(0);
        if next == current {
            debug!(
                surface_id = surface_id.0,
                dial_index,
                detents,
                lit_segments = current,
                "dial turn hit the end of its ring"
            );
            return;
        }
        debug!(
            surface_id = surface_id.0,
            dial_index,
            detents,
            from_segments = current,
            to_segments = next,
            "dial moved"
        );
        self.dial_positions
            .write()
            .unwrap()
            .insert((surface_id.0.clone(), dial_index), next);
        self.send_dial_color(surface_id, dial_index, color, next);
        let level = percent_from_segments(next);
        self.log(
            surface_id,
            SurfaceLogLevel::Input,
            format!(
                "dial {dial_index} turned {detents:+} to {level}% ({next}/{DIAL_RING_SEGMENTS})"
            ),
        );
        self.emit(ServerEvent::DialState {
            surface_id: surface_id.clone(),
            dial_index,
            level,
        });
    }

    /// Records a dial being pushed in or released. Returns whether that changed anything, so the
    /// caller can log edges rather than every report. Nothing is bound to a dial press yet.
    pub fn record_dial_press(
        &self,
        surface_id: &SurfaceId,
        dial_index: u8,
        is_pressed: bool,
    ) -> bool {
        if dial_index >= DIAL_COUNT {
            return false;
        }
        let previous = self
            .dial_presses
            .write()
            .unwrap()
            .insert((surface_id.0.clone(), dial_index), is_pressed);
        if previous == Some(is_pressed) {
            return false;
        }
        self.log(
            surface_id,
            SurfaceLogLevel::Input,
            format!(
                "dial {dial_index} {}",
                if is_pressed { "pressed" } else { "released" }
            ),
        );
        self.emit(ServerEvent::DialPress {
            surface_id: surface_id.clone(),
            dial_index,
            is_pressed,
        });
        true
    }

    /// Colour and current segment count for one dial. Dials the panel leaves unconfigured still
    /// respond to a turn, lit white, so the knob is never dead.
    fn dial_ring(&self, surface_id: &SurfaceId, dial_index: u8) -> Option<(RgbaColor, u8)> {
        let device = self.managed(surface_id)?;
        let panel = self.panel(&device.active_panel_id?.0)?;
        let index = usize::from(dial_index);
        let color = panel.dial_colors.get(index).cloned().unwrap_or_else(white);
        Some((
            color,
            self.lit_segments(surface_id, dial_index, &panel, index),
        ))
    }

    fn lit_segments(
        &self,
        surface_id: &SurfaceId,
        dial_index: u8,
        panel: &Panel,
        index: usize,
    ) -> u8 {
        self.dial_positions
            .read()
            .unwrap()
            .get(&(surface_id.0.clone(), dial_index))
            .copied()
            .unwrap_or_else(|| {
                segments_from_percent(panel.dial_ring_levels.get(index).copied().unwrap_or(100))
            })
    }

    /// Drops runtime dial state so the dials fall back to the panel's configured levels.
    fn reset_dial_positions(&self, surface_id: &str) {
        self.dial_positions
            .write()
            .unwrap()
            .retain(|(id, _), _| id != surface_id);
        self.dial_presses
            .write()
            .unwrap()
            .retain(|(id, _), _| id != surface_id);
    }

    pub fn record_key_state(
        &self,
        surface_id: &SurfaceId,
        key_index: u8,
        is_pressed: bool,
    ) -> bool {
        let mut key_states = self.key_states.write().unwrap();
        let previous_state = key_states.insert((surface_id.0.clone(), key_index), is_pressed);
        if previous_state == Some(is_pressed) {
            return false;
        }
        let mut events = self.recent_key_events.write().unwrap();
        events.push_front(SurfaceKeyEvent {
            surface_id: surface_id.clone(),
            key_index,
            is_pressed,
        });
        events.truncate(50);
        drop(events);
        self.log(
            surface_id,
            SurfaceLogLevel::Input,
            format!(
                "key {key_index} {}",
                if is_pressed { "pressed" } else { "released" }
            ),
        );
        self.emit(ServerEvent::KeyState {
            surface_id: surface_id.clone(),
            key_index,
            is_pressed,
        });
        if let Some(rendering) = self.rendering_for_key(surface_id, key_index, is_pressed) {
            self.send_rendering(surface_id, rendering);
        }
        self.dispatch_input(InputEvent::Key {
            surface_id: surface_id.clone(),
            key_index,
            is_pressed,
        });
        true
    }

    fn dispatch_input(&self, event: InputEvent) {
        if let Err(mpsc::error::TrySendError::Full(_)) = self.input.try_send(event) {
            warn!(
                capacity = INPUT_QUEUE_SIZE,
                "input queue is full, dropped a gesture; bound actions did not run"
            );
        }
    }

    fn rendering_for_key(
        &self,
        surface_id: &SurfaceId,
        key_index: u8,
        is_pressed: bool,
    ) -> Option<KeyRendering> {
        let device = self.managed(surface_id)?;
        let panel_id = device.active_panel_id?;
        let panel = self.panel(&panel_id.0)?;
        panel.controls.iter().find_map(|control| {
            (key_index_for(control, panel.layout.columns) == Some(key_index)).then(|| {
                rendering_for_control(
                    control,
                    is_pressed,
                    panel.layout.columns,
                    &self.render_context(),
                )
            })?
        })
    }
    fn render_active_panel(&self, surface_id: &str) {
        let surface_id = SurfaceId(surface_id.to_string());
        for rendering in self.active_key_renderings(&surface_id) {
            self.send_rendering(&surface_id, rendering);
        }
        for (dial_index, color, lit_segments) in self.active_dial_rings(&surface_id) {
            self.send_dial_color(&surface_id, dial_index, color, lit_segments);
        }
    }
    /// Drops a repaint that would produce the identical image. Without this a single one-hertz
    /// variable on a 16x2 Studio would re-encode thirty-two JPEGs a second to change one of them.
    fn send_rendering(&self, surface_id: &SurfaceId, rendering: KeyRendering) {
        let key_index = rendering.key_index;
        let fingerprint = fingerprint(&rendering);
        if self
            .last_rendered
            .write()
            .unwrap()
            .insert((surface_id.0.clone(), key_index), fingerprint)
            == Some(fingerprint)
        {
            return;
        }
        self.dispatch(
            surface_id,
            SurfaceCommand::RenderKey(rendering),
            &format!("key {key_index} rendering"),
        );
    }

    /// Forgets what a surface was showing, so the next repaint is sent rather than deduplicated
    /// against a device that has since been reset.
    fn forget_renderings(&self, surface_id: &str) {
        self.last_rendered
            .write()
            .unwrap()
            .retain(|(id, _), _| id != surface_id);
    }
    fn send_dial_color(
        &self,
        surface_id: &SurfaceId,
        dial_index: u8,
        color: RgbaColor,
        lit_segments: u8,
    ) {
        self.dispatch(
            surface_id,
            SurfaceCommand::RenderDialColor {
                dial_index,
                color,
                lit_segments,
            },
            &format!("dial {dial_index} ring"),
        );
    }

    /// Hands a command to the surface's connection task. Both failures used to be swallowed: a full
    /// queue means the device is not keeping up and the surface is now showing something stale,
    /// which is worth a warning rather than silence.
    fn dispatch(&self, surface_id: &SurfaceId, command: SurfaceCommand, what: &str) {
        let Some(sender) = self
            .active_connections
            .read()
            .unwrap()
            .get(&surface_id.0)
            .map(|connection| connection.command_sender.clone())
        else {
            debug!(
                surface_id = surface_id.0,
                what, "dropped a command: no active connection for the surface"
            );
            return;
        };
        match sender.try_send(command) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!(
                    surface_id = surface_id.0,
                    what,
                    capacity = SURFACE_COMMAND_QUEUE_SIZE,
                    "surface command queue is full, dropped a command; the device is behind and \
                     its keys or dials will be stale"
                );
                self.log(
                    surface_id,
                    SurfaceLogLevel::Warning,
                    format!("dropped {what}: the device is behind"),
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => debug!(
                surface_id = surface_id.0,
                what, "dropped a command: the connection task has gone"
            ),
        }
    }
}

struct ActiveConnection {
    is_active: Arc<AtomicBool>,
    command_sender: mpsc::Sender<SurfaceCommand>,
}

fn is_compatible(device: &ManagedNetworkSurface, panel: &Panel) -> bool {
    matches!((&device.layout, &panel.layout), (SurfaceLayout::Grid { columns: dc, rows: dr }, PanelLayout { columns: pc, rows: pr }) if dc == pc && dr == pr)
        && supports(&device.capabilities, &panel.capabilities)
}
fn supports(device: &SurfaceCapabilities, panel: &SurfaceCapabilities) -> bool {
    (!panel.supports_color || device.supports_color)
        && (!panel.supports_images || device.supports_images)
        && (!panel.supports_text || device.supports_text)
        && (!panel.supports_brightness || device.supports_brightness)
        && (!panel.supports_haptics || device.supports_haptics)
}
fn validate_panel(panel: &Panel) -> Result<(), String> {
    if panel.name.trim().is_empty() || panel.layout.columns == 0 || panel.layout.rows == 0 {
        return Err("panel name and layout dimensions are required".to_string());
    }
    let mut positions = HashMap::new();
    for control in &panel.controls {
        if control.position.column >= panel.layout.columns
            || control.position.row >= panel.layout.rows
        {
            return Err("control position is outside panel layout".to_string());
        }
        if positions
            .insert((control.position.column, control.position.row), ())
            .is_some()
        {
            return Err("panel controls cannot share a position".to_string());
        }
    }
    Ok(())
}
pub fn key_index_for(control: &Control, columns: u16) -> Option<u8> {
    u8::try_from(
        u32::from(control.position.row) * u32::from(columns) + u32::from(control.position.column),
    )
    .ok()
}
fn rendering_for_control(
    control: &Control,
    is_pressed: bool,
    columns: u16,
    context: &RenderContext<'_>,
) -> Option<KeyRendering> {
    let state = context.resolve(control, is_pressed);
    Some(KeyRendering {
        key_index: key_index_for(control, columns)?,
        text: state.text,
        icon: None,
        foreground_color: state.foreground_color,
        background_color: state.background_color,
    })
}

fn fingerprint(rendering: &KeyRendering) -> u64 {
    let mut hasher = DefaultHasher::new();
    rendering.hash(&mut hasher);
    hasher.finish()
}
fn default_device(
    devices: &[ManagedNetworkSurface],
    active_panel_id: Option<PanelId>,
) -> ManagedNetworkSurface {
    ManagedNetworkSurface {
        surface_id: SurfaceId(format!("stream-deck-studio-{}", devices.len() + 1)),
        name: "Stream Deck Studio".to_string(),
        host: "127.0.0.1".to_string(),
        port: crate::streamdeck::studio::default_port(),
        serial_number: None,
        model: "Stream Deck Studio".to_string(),
        layout: SurfaceLayout::Grid {
            columns: 16,
            rows: 2,
        },
        capabilities: studio_capabilities(),
        active_panel_id,
        is_enabled: true,
        parent_surface_id: None,
        status: NetworkSurfaceStatus::Connecting,
        last_error: None,
    }
}

fn repair_panel_assignments(devices: &mut [ManagedNetworkSurface], panels: &mut Vec<Panel>) {
    let needs_studio_panel = devices.iter().any(|device| {
        device.active_panel_id.as_ref().is_none_or(|panel_id| {
            panels
                .iter()
                .find(|panel| panel.panel_id == *panel_id)
                .is_none_or(|panel| !is_compatible(device, panel))
        }) && matches!(
            device.layout,
            SurfaceLayout::Grid {
                columns: 16,
                rows: 2
            }
        )
    });
    if needs_studio_panel
        && !panels
            .iter()
            .any(|panel| panel.layout.columns == 16 && panel.layout.rows == 2)
    {
        panels.push(default_panel());
    }

    for device in devices {
        let assignment_is_valid = device.active_panel_id.as_ref().and_then(|panel_id| {
            panels
                .iter()
                .find(|panel| panel.panel_id == *panel_id)
                .filter(|panel| is_compatible(device, panel))
        });
        if assignment_is_valid.is_some() {
            continue;
        }
        device.active_panel_id = panels
            .iter()
            .find(|panel| is_compatible(device, panel))
            .map(|panel| panel.panel_id.clone());
    }
}
pub fn studio_capabilities() -> SurfaceCapabilities {
    SurfaceCapabilities {
        supports_color: true,
        supports_images: true,
        supports_text: true,
        supports_brightness: true,
        supports_haptics: false,
    }
}
fn default_panel() -> Panel {
    Panel {
        panel_id: PanelId("studio-panel-1".to_string()),
        name: "Hello".to_string(),
        layout: PanelLayout {
            columns: 16,
            rows: 2,
        },
        capabilities: studio_capabilities(),
        controls: vec![Control {
            control_id: ControlId("hello".to_string()),
            name: "Hello".to_string(),
            position: SurfacePosition { column: 0, row: 0 },
            default_state: RenderedState {
                text: Some("Hello".to_string()),
                foreground_color: Some(white()),
                background_color: Some(color(35, 88, 165)),
                image: None,
                progress: None,
                is_pressed: false,
            },
            pressed_state: Some(RenderedState {
                text: Some("Hello".to_string()),
                foreground_color: Some(white()),
                background_color: Some(color(18, 44, 83)),
                image: None,
                progress: None,
                is_pressed: true,
            }),
            action_bindings: Vec::new(),
            feedback_bindings: Vec::new(),
        }],
        dial_colors: vec![color(35, 88, 165), color(35, 88, 165)],
        dial_ring_levels: vec![100, 100],
    }
}
fn segments_from_percent(percent: u8) -> u8 {
    let segments = u16::from(percent.min(100)) * u16::from(DIAL_RING_SEGMENTS) / 100;
    u8::try_from(segments).unwrap_or(DIAL_RING_SEGMENTS)
}

/// Rounds up, so a level reported to the API converts back to the same segment count the device is
/// lighting - one lit segment reads as 5%, not 4% (which would floor back to zero).
fn percent_from_segments(lit_segments: u8) -> u8 {
    let segments = u16::from(lit_segments.min(DIAL_RING_SEGMENTS));
    u8::try_from((segments * 100).div_ceil(u16::from(DIAL_RING_SEGMENTS))).unwrap_or(100)
}

fn white() -> RgbaColor {
    color(u8::MAX, u8::MAX, u8::MAX)
}
fn color(red: u8, green: u8, blue: u8) -> RgbaColor {
    RgbaColor {
        red,
        green,
        blue,
        alpha: u8::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::{default_device, default_panel, SurfaceRegistry};
    use crate::models::{
        identifiers::{PanelId, SurfaceId},
        network_surface::ServerEvent,
        surface::SurfaceLayout,
    };

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
                .text
                .as_deref(),
            Some("Hello")
        );
    }

    #[test]
    fn rejects_assigning_a_panel_with_an_incompatible_layout() {
        let panel = default_panel();
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![panel.clone()]);
        let mut incompatible = panel;
        incompatible.panel_id.0 = "incompatible".to_string();
        incompatible.layout.columns = 3;
        registry
            .upsert_panel(incompatible)
            .expect("panel should be valid");

        assert!(registry
            .assign_active_panel("stream-deck-studio-1", "incompatible")
            .is_err());
    }

    #[test]
    fn deleting_the_active_panel_falls_back_to_a_compatible_one() {
        let mut spare = default_panel();
        spare.panel_id = PanelId("studio-panel-2".to_string());
        let registry =
            SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel(), spare]);

        registry
            .remove_panel("studio-panel-1")
            .expect("the active panel should be removable");

        let device = registry
            .managed(&SurfaceId("stream-deck-studio-1".to_string()))
            .expect("default device should exist");
        assert_eq!(
            device.active_panel_id.as_ref().map(|id| id.0.as_str()),
            Some("studio-panel-2")
        );
        assert!(registry.panel("studio-panel-1").is_none());
    }

    #[test]
    fn deleting_the_last_compatible_panel_leaves_the_device_unassigned() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);

        registry
            .remove_panel("studio-panel-1")
            .expect("the active panel should be removable");

        let device = registry
            .managed(&SurfaceId("stream-deck-studio-1".to_string()))
            .expect("default device should exist");
        assert_eq!(device.active_panel_id, None);
        assert!(registry.panels().is_empty());
        assert!(registry.remove_panel("studio-panel-1").is_none());
    }

    #[test]
    fn repairs_a_missing_studio_panel_and_keeps_dial_settings_available() {
        let mut incompatible = default_panel();
        incompatible.panel_id = PanelId("studio-panel-2".to_string());
        incompatible.layout = crate::models::panel::PanelLayout {
            columns: 8,
            rows: 4,
        };
        let device = default_device(&[], Some(incompatible.panel_id.clone()));
        let registry = SurfaceRegistry::from_configuration(vec![device], vec![incompatible]);
        let device = registry
            .managed(&SurfaceId("stream-deck-studio-1".to_string()))
            .expect("configured device should exist");

        assert_eq!(
            device.active_panel_id.as_ref().map(|id| id.0.as_str()),
            Some("studio-panel-1")
        );
        assert_eq!(registry.active_dial_rings(&device.surface_id).len(), 2);
    }

    #[test]
    fn a_dial_turn_moves_one_ring_segment_per_detent_and_clamps() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());
        // The default panel starts both dials at 100%, so a full ring.
        assert_eq!(registry.active_dial_rings(&surface_id)[0].2, 24);

        registry.record_dial_turn(&surface_id, 0, -4);
        assert_eq!(registry.active_dial_rings(&surface_id)[0].2, 20);
        assert_eq!(dial_level(&registry, &surface_id, 0), Some(84));

        registry.record_dial_turn(&surface_id, 0, -60);
        assert_eq!(registry.active_dial_rings(&surface_id)[0].2, 0);
        assert_eq!(dial_level(&registry, &surface_id, 0), Some(0));

        registry.record_dial_turn(&surface_id, 0, 1);
        assert_eq!(dial_level(&registry, &surface_id, 0), Some(5));

        registry.record_dial_turn(&surface_id, 0, 99);
        assert_eq!(dial_level(&registry, &surface_id, 0), Some(100));
        // The other dial keeps the panel's configured level.
        assert_eq!(dial_level(&registry, &surface_id, 1), None);
    }

    #[test]
    fn reports_dial_presses_and_releases_once_per_edge() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());

        assert!(registry.record_dial_press(&surface_id, 0, true));
        assert!(!registry.record_dial_press(&surface_id, 0, true));
        assert_eq!(registry.inventory().dial_presses.len(), 1);

        assert!(registry.record_dial_press(&surface_id, 0, false));
        assert!(registry.inventory().dial_presses.is_empty());
    }

    #[test]
    fn reported_dial_levels_survive_a_round_trip_to_segments() {
        for lit_segments in 0..=24 {
            let level = super::percent_from_segments(lit_segments);
            assert_eq!(super::segments_from_percent(level), lit_segments);
        }
    }

    fn dial_level(
        registry: &SurfaceRegistry,
        surface_id: &SurfaceId,
        dial_index: u8,
    ) -> Option<u8> {
        registry
            .inventory()
            .dial_states
            .into_iter()
            .find(|state| &state.surface_id == surface_id && state.dial_index == dial_index)
            .map(|state| state.level)
    }

    #[test]
    fn auto_provisions_a_panel_for_an_unseen_child_layout() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let layout = SurfaceLayout::Grid {
            columns: 5,
            rows: 3,
        };

        let panel_id = registry
            .ensure_default_panel_for_layout(&layout)
            .expect("a panel should be provisioned");
        let panel = registry.panel(&panel_id.0).expect("panel should exist");
        assert_eq!(panel.controls.len(), 15);
        assert_eq!((panel.layout.columns, panel.layout.rows), (5, 3));

        let reused = registry
            .ensure_default_panel_for_layout(&layout)
            .expect("existing panel should be reused");
        assert_eq!(reused, panel_id);
    }

    #[test]
    fn broadcasts_a_key_state_event_when_a_key_is_pressed() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let mut receiver = registry.subscribe();
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());

        registry.record_key_state(&surface_id, 0, true);

        let mut key_state = None;
        while let Ok(event) = receiver.try_recv() {
            if let ServerEvent::KeyState {
                key_index,
                is_pressed,
                ..
            } = event
            {
                key_state = Some((key_index, is_pressed));
            }
        }
        assert_eq!(key_state, Some((0, true)));
    }

    #[test]
    fn key_state_event_serializes_with_a_string_surface_id() {
        let event = ServerEvent::KeyState {
            surface_id: SurfaceId("stream-deck-studio-1".to_string()),
            key_index: 3,
            is_pressed: true,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"key_state","surface_id":"stream-deck-studio-1","key_index":3,"is_pressed":true}"#
        );
    }
}
