use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, RwLock,
    },
};

use tokio::sync::{broadcast, mpsc};

use crate::{
    models::{
        control::Control,
        identifiers::{ControlId, PanelId, SurfaceId},
        network_surface::{
            DeviceInventory, DiscoveredNetworkSurface, KeyRendering, ManagedNetworkSurface,
            NetworkSurfaceStatus, ServerEvent, SurfaceCommand, SurfaceKeyEvent,
        },
        panel::{Panel, PanelLayout},
        rendered_state::{RenderedState, RgbaColor},
        surface::{SurfaceCapabilities, SurfaceLayout, SurfacePosition},
    },
    persistence::Persistence,
};

#[derive(Clone)]
pub struct AppState {
    pub surfaces: Arc<SurfaceRegistry>,
    persistence: Arc<Persistence>,
}

impl AppState {
    pub async fn load() -> anyhow::Result<Self> {
        let (persistence, devices, mut panels) = Persistence::open().await?;
        if panels.is_empty() {
            panels.push(default_panel());
        }
        Ok(Self {
            surfaces: Arc::new(SurfaceRegistry::from_configuration(devices, panels)),
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
    dock_children: RwLock<HashMap<String, Vec<String>>>,
    events: broadcast::Sender<ServerEvent>,
    next_surface_number: AtomicU64,
    next_panel_number: AtomicU64,
}

impl SurfaceRegistry {
    pub fn from_configuration(mut devices: Vec<ManagedNetworkSurface>, panels: Vec<Panel>) -> Self {
        let default_panel_id = panels.first().map(|panel| panel.panel_id.clone());
        if !devices.iter().any(|device| device.is_enabled) {
            devices.push(default_device(&devices, default_panel_id));
        }
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
            dock_children: RwLock::default(),
            events: broadcast::channel(256).0,
            next_surface_number: AtomicU64::new(next_surface_number),
            next_panel_number: AtomicU64::new(next_panel_number),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.events.subscribe()
    }

    fn emit(&self, event: ServerEvent) {
        let _ = self.events.send(event);
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
        discovered.sort_by(|left, right| left.name.cmp(&right.name));
        devices.sort_by(|left, right| left.name.cmp(&right.name));
        panels.sort_by(|left, right| left.name.cmp(&right.name));
        DeviceInventory {
            discovered,
            devices,
            panels,
            recent_key_events,
            key_states,
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
        self.render_active_panel(surface_id);
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
        let (command_sender, command_receiver) = mpsc::channel(64);
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
                    surface.status = status;
                    surface.last_error = last_error;
                    changed
                }
                None => false,
            }
        };
        if changed {
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
        panel
            .controls
            .iter()
            .filter_map(|control| rendering_for_control(control, false, panel.layout.columns))
            .collect()
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
        self.emit(ServerEvent::KeyState {
            surface_id: surface_id.clone(),
            key_index,
            is_pressed,
        });
        if let Some(rendering) = self.rendering_for_key(surface_id, key_index, is_pressed) {
            self.send_rendering(surface_id, rendering);
        }
        true
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
        panel
            .controls
            .iter()
            .find(|control| key_index_for(control, panel.layout.columns) == Some(key_index))
            .and_then(|control| rendering_for_control(control, is_pressed, panel.layout.columns))
    }
    fn render_active_panel(&self, surface_id: &str) {
        let surface_id = SurfaceId(surface_id.to_string());
        for rendering in self.active_key_renderings(&surface_id) {
            self.send_rendering(&surface_id, rendering);
        }
    }
    fn send_rendering(&self, surface_id: &SurfaceId, rendering: KeyRendering) {
        if let Some(sender) = self
            .active_connections
            .read()
            .unwrap()
            .get(&surface_id.0)
            .map(|connection| connection.command_sender.clone())
        {
            sender.try_send(SurfaceCommand::RenderKey(rendering)).ok();
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
fn key_index_for(control: &Control, columns: u16) -> Option<u8> {
    u8::try_from(
        u32::from(control.position.row) * u32::from(columns) + u32::from(control.position.column),
    )
    .ok()
}
fn rendering_for_control(
    control: &Control,
    is_pressed: bool,
    columns: u16,
) -> Option<KeyRendering> {
    let state = if is_pressed {
        control
            .pressed_state
            .as_ref()
            .unwrap_or(&control.default_state)
    } else {
        &control.default_state
    };
    Some(KeyRendering {
        key_index: key_index_for(control, columns)?,
        text: state.text.clone(),
        icon: None,
        foreground_color: state.foreground_color.clone(),
        background_color: state.background_color.clone(),
    })
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
    }
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
    use super::{default_panel, SurfaceRegistry};
    use crate::models::{
        identifiers::SurfaceId, network_surface::ServerEvent, surface::SurfaceLayout,
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
    fn auto_provisions_a_panel_for_an_unseen_child_layout() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let layout = SurfaceLayout::Grid { columns: 5, rows: 3 };

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
