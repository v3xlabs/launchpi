use serde::{Deserialize, Serialize};

use crate::panels::rendered_state::RgbaColor;

/// One rotary dial a panel declares. `index` is the dial's position on the surface, counted from
/// the left, and `level` the percentage of the ring lit when the panel loads.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PanelDial {
    pub index: u8,
    #[serde(default = "full_ring")]
    pub level: u8,
    pub color: RgbaColor,
}

pub fn full_ring() -> u8 {
    100
}
