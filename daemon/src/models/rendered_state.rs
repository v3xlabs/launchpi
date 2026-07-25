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

impl RenderedState {
    /// Applies an active feedback's style, field by field. A field the override leaves as `None`
    /// keeps whatever the state already had, so several feedbacks can each contribute one aspect
    /// and the last one to set a field wins it.
    pub fn overlay(&mut self, override_state: &RenderedStateOverride) {
        if let Some(text) = &override_state.text {
            self.text = Some(text.clone());
        }
        if let Some(image) = &override_state.image {
            self.image = Some(image.clone());
        }
        if let Some(color) = &override_state.foreground_color {
            self.foreground_color = Some(color.clone());
        }
        if let Some(color) = &override_state.background_color {
            self.background_color = Some(color.clone());
        }
        if let Some(progress) = &override_state.progress {
            self.progress = Some(progress.clone());
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderedStateOverride {
    pub text: Option<String>,
    pub image: Option<AssetId>,
    pub foreground_color: Option<RgbaColor>,
    pub background_color: Option<RgbaColor>,
    pub progress: Option<Progress>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
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
