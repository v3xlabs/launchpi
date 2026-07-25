use serde::{Deserialize, Serialize};

use crate::{
    identifiers::AssetId,
    panels::rendered_state::{ContentLayout, Progress, RgbaColor},
};

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct KeyRendering {
    pub key_index: u8,
    pub text: Option<String>,
    pub icon: Option<KeyIcon>,
    #[serde(default)]
    pub image: Option<AssetId>,
    #[serde(default)]
    pub progress: Option<Progress>,
    pub foreground_color: Option<RgbaColor>,
    pub background_color: Option<RgbaColor>,
    #[serde(default)]
    pub content_layout: ContentLayout,
    #[serde(default)]
    pub is_dimmed: bool,
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

#[derive(Clone, Debug)]
pub enum SurfaceCommand {
    RenderKey(KeyRendering),
    RenderDialColor {
        dial_index: u8,
        color: RgbaColor,
        lit_segments: u8,
    },
}
