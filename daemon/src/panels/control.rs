use serde::{Deserialize, Serialize};

use crate::{
    bindings::{action::ActionBinding, feedback::FeedbackBinding},
    identifiers::ControlId,
    panels::rendered_state::RenderedState,
    surfaces::layout::SurfacePosition,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Control {
    pub control_id: ControlId,
    pub name: String,
    pub position: SurfacePosition,
    pub default_state: RenderedState,
    pub pressed_state: Option<RenderedState>,
    pub action_bindings: Vec<ActionBinding>,
    pub feedback_bindings: Vec<FeedbackBinding>,
}
