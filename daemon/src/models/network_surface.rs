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

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct KeyRendering {
    pub key_index: u8,
    pub text: Option<String>,
    pub icon: Option<KeyIcon>,
    pub foreground_color: Option<RgbaColor>,
    pub background_color: Option<RgbaColor>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyIcon {
    Circle,
    Diamond,
    Pause,
    Play,
    Square,
    Triangle,
}

/// Rotary dials on a Stream Deck Studio, and the LED segments making up one dial ring.
/// A single detent of the knob is one segment.
pub const DIAL_COUNT: u8 = 2;
pub const DIAL_RING_SEGMENTS: u8 = 24;

#[derive(Clone, Debug)]
pub enum SurfaceCommand {
    RenderKey(KeyRendering),
    RenderDialColor {
        dial_index: u8,
        color: RgbaColor,
        lit_segments: u8,
    },
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
    pub dial_states: Vec<SurfaceDialState>,
    pub dial_presses: Vec<SurfaceDialPress>,
    pub logs: Vec<SurfaceLogEntry>,
}

/// One line of a device's activity log: what the daemon saw the device do, and what it did back.
/// Memory only - a live diagnostic, not history worth persisting.
#[derive(Clone, Debug, Serialize)]
pub struct SurfaceLogEntry {
    pub surface_id: SurfaceId,
    /// Per-surface and monotonic, so the web can dedupe a snapshot against the live stream.
    pub sequence: u64,
    pub at_ms: u64,
    pub level: SurfaceLogLevel,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceLogLevel {
    Input,
    Info,
    Warning,
}

#[derive(Clone, Debug, Serialize)]
pub struct SurfaceKeyEvent {
    pub surface_id: SurfaceId,
    pub key_index: u8,
    pub is_pressed: bool,
}

/// Where a dial currently stands, as a percentage of its ring. Runtime only - the panel keeps the
/// level the dial starts from, and turning the knob never rewrites it.
#[derive(Clone, Debug, Serialize)]
pub struct SurfaceDialState {
    pub surface_id: SurfaceId,
    pub dial_index: u8,
    pub level: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct SurfaceDialPress {
    pub surface_id: SurfaceId,
    pub dial_index: u8,
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
    DialState {
        surface_id: SurfaceId,
        dial_index: u8,
        level: u8,
    },
    DialPress {
        surface_id: SurfaceId,
        dial_index: u8,
        is_pressed: bool,
    },
    Log(SurfaceLogEntry),
    Changed,
}
