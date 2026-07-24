use serde::{Deserialize, Serialize};

use crate::models::{control::Control, identifiers::PanelId};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Panel {
    pub panel_id: PanelId,
    pub name: String,
    pub layout: PanelLayout,
    pub controls: Vec<Control>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PanelLayout {
    pub columns: u16,
    pub rows: u16,
}
