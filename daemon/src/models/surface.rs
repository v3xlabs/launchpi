use serde::{Deserialize, Serialize};

use crate::models::identifiers::SurfaceId;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Surface {
    pub surface_id: SurfaceId,
    pub name: String,
    pub kind: SurfaceKind,
    pub layout: SurfaceLayout,
    pub controls: Vec<SurfaceControl>,
    pub capabilities: SurfaceCapabilities,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SurfaceKind {
    StreamDeck,
    Midi,
    Keyboard,
    Launchpad,
    Custom(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SurfaceLayout {
    Grid { columns: u16, rows: u16 },
    Freeform,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceControl {
    pub surface_control_id: String,
    pub kind: SurfaceControlKind,
    pub position: SurfacePosition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SurfaceControlKind {
    Key,
    Encoder,
    Fader,
    Display,
    InputOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfacePosition {
    pub column: u16,
    pub row: u16,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceCapabilities {
    pub supports_color: bool,
    pub supports_images: bool,
    pub supports_text: bool,
    pub supports_brightness: bool,
    pub supports_haptics: bool,
}
