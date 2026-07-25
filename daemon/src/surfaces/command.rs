use serde::{Deserialize, Serialize};

use crate::panels::rendered_state::{ResolvedLayer, RgbaColor};

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct KeyRendering {
    pub key_index: u8,
    #[serde(default)]
    pub layers: Vec<ResolvedLayer>,
    #[serde(default)]
    pub is_dimmed: bool,
}

impl KeyRendering {
    /// The one colour a palette-only surface can show. The bottom-most fill is the key's own
    /// background; anything above it is detail those surfaces cannot express.
    pub fn palette_color(&self) -> Option<RgbaColor> {
        self.layers.iter().find_map(|layer| match layer {
            ResolvedLayer::Fill { color } => Some(color.clone()),
            _ => None,
        })
    }
}

#[derive(Clone, Debug)]
pub enum SurfaceCommand {
    RenderKey(KeyRendering),
    RenderDialColor {
        dial_index: u8,
        color: RgbaColor,
        lit_segments: u8,
    },
}
