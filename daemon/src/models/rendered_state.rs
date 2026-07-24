use serde::{Deserialize, Serialize};

use crate::models::identifiers::AssetId;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedState {
    pub text: Option<String>,
    pub image: Option<AssetId>,
    pub foreground_color: Option<RgbaColor>,
    pub background_color: Option<RgbaColor>,
    pub progress: Option<Progress>,
    pub is_pressed: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedStateOverride {
    pub text: Option<String>,
    pub image: Option<AssetId>,
    pub foreground_color: Option<RgbaColor>,
    pub background_color: Option<RgbaColor>,
    pub progress: Option<Progress>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RgbaColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Progress {
    pub value: u16,
    pub maximum_value: u16,
}
