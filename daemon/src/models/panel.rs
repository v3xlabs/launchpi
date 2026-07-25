use serde::{Deserialize, Serialize};

use crate::models::{
    control::Control, identifiers::PanelId, rendered_state::RgbaColor, surface::SurfaceCapabilities,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Panel {
    pub panel_id: PanelId,
    pub name: String,
    pub layout: PanelLayout,
    #[serde(default)]
    pub capabilities: SurfaceCapabilities,
    pub controls: Vec<Control>,
    #[serde(default)]
    pub dial_colors: Vec<RgbaColor>,
    #[serde(default)]
    pub dial_ring_levels: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PanelLayout {
    pub columns: u16,
    pub rows: u16,
}
