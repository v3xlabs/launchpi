use serde::{Deserialize, Serialize};

use crate::models::{
    identifiers::{PanelId, SurfaceId},
    rendered_state::RgbaColor,
    surface::{SurfaceCapabilities, SurfaceLayout},
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyRendering {
    pub key_index: u8,
    pub text: Option<String>,
    pub icon: Option<KeyIcon>,
    pub foreground_color: Option<RgbaColor>,
    pub background_color: Option<RgbaColor>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyIcon {
    Circle,
    Diamond,
    Pause,
    Play,
    Square,
    Triangle,
}

#[derive(Clone, Debug)]
pub enum SurfaceCommand {
    RenderKey(KeyRendering),
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

#[derive(Clone, Debug, Serialize)]
pub struct DeviceInventory {
    pub discovered: Vec<DiscoveredNetworkSurface>,
    pub devices: Vec<ManagedNetworkSurface>,
    pub panels: Vec<crate::models::panel::Panel>,
    pub recent_key_events: Vec<SurfaceKeyEvent>,
    pub key_states: Vec<SurfaceKeyEvent>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SurfaceKeyEvent {
    pub surface_id: SurfaceId,
    pub key_index: u8,
    pub is_pressed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    KeyState {
        surface_id: SurfaceId,
        key_index: u8,
        is_pressed: bool,
    },
    Changed,
}
