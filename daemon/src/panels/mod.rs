use serde::{Deserialize, Serialize};

pub mod control;
pub mod dial;
pub mod rendered_state;

use crate::{
    identifiers::PanelId,
    panels::{control::Control, dial::PanelDial},
    surfaces::layout::SurfaceCapabilities,
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
    pub dials: Vec<PanelDial>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PanelLayout {
    pub columns: u16,
    pub rows: u16,
}
