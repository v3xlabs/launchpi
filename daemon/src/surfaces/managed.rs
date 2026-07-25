use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

use crate::{
    events::ServerEvent,
    identifiers::{PanelId, SurfaceId},
    surfaces::{
        layout::{SurfaceCapabilities, SurfaceLayout},
        logs::SurfaceLogLevel,
        registry::SurfaceRegistry,
    },
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkSurfaceStatus {
    Connecting,
    Connected,
    Unavailable,
    Disabled,
}

impl Default for NetworkSurfaceStatus {
    fn default() -> Self {
        Self::Connecting
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiscoveredNetworkSurface {
    pub discovery_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub serial_number: Option<String>,
    pub model: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManagedNetworkSurface {
    pub surface_id: SurfaceId,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub serial_number: Option<String>,
    pub model: String,
    #[serde(default = "default_stream_deck_layout")]
    pub layout: SurfaceLayout,
    #[serde(default = "default_stream_deck_capabilities")]
    pub capabilities: SurfaceCapabilities,
    pub active_panel_id: Option<PanelId>,
    pub is_enabled: bool,
    #[serde(default)]
    pub parent_surface_id: Option<SurfaceId>,
    #[serde(skip_deserializing, default)]
    pub status: NetworkSurfaceStatus,
    #[serde(skip_deserializing, default)]
    pub last_error: Option<String>,
}

fn default_stream_deck_layout() -> SurfaceLayout {
    SurfaceLayout::Grid {
        columns: 16,
        rows: 2,
    }
}

fn default_stream_deck_capabilities() -> SurfaceCapabilities {
    SurfaceCapabilities {
        supports_color: true,
        supports_images: true,
        supports_text: true,
        supports_brightness: true,
        supports_haptics: false,
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    #[default]
    Studio,
    NetworkDock,
}

impl SurfaceKind {
    pub fn model_name(self) -> &'static str {
        match self {
            Self::Studio => "Stream Deck Studio",
            Self::NetworkDock => "Stream Deck Network Dock",
        }
    }

    pub fn is_network_dock(self) -> bool {
        matches!(self, Self::NetworkDock)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AddNetworkSurface {
    pub name: String,
    pub host: String,
    pub port: Option<u16>,
    pub serial_number: Option<String>,
    #[serde(default)]
    pub kind: SurfaceKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UpdateNetworkSurface {
    pub is_enabled: bool,
}

impl SurfaceRegistry {
    /// mDNS re-announces a service periodically, and an announcement that says exactly what the
    /// last one said is not news. Emitting `Changed` for it made the web refetch on the discovery
    /// cadence for no reason.
    pub fn upsert_discovered(&self, surface: DiscoveredNetworkSurface) {
        let changed = self
            .discovered
            .write()
            .unwrap()
            .insert(surface.discovery_id.clone(), surface.clone())
            .is_none_or(|previous| previous != surface);
        if changed {
            self.emit(ServerEvent::Changed);
        }
    }
    pub fn remove_discovered(&self, discovery_id: &str) {
        if self
            .discovered
            .write()
            .unwrap()
            .remove(discovery_id)
            .is_some()
        {
            self.emit(ServerEvent::Changed);
        }
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
            self.emit(ServerEvent::DeviceStatus {
                surface_id: surface_id.clone(),
                status,
                last_error,
            });
        }
    }
}

fn status_label(status: &NetworkSurfaceStatus) -> &'static str {
    match status {
        NetworkSurfaceStatus::Connecting => "connecting",
        NetworkSurfaceStatus::Connected => "connected",
        NetworkSurfaceStatus::Unavailable => "unavailable",
        NetworkSurfaceStatus::Disabled => "disabled",
    }
}
