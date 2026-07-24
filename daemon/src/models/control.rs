use serde::{Deserialize, Serialize};

use crate::models::{
    action::ActionBinding,
    feedback::FeedbackBinding,
    identifiers::ControlId,
    rendered_state::RenderedState,
    surface::SurfacePosition,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Control {
    pub control_id: ControlId,
    pub name: String,
    pub position: SurfacePosition,
    pub default_state: RenderedState,
    pub action_bindings: Vec<ActionBinding>,
    pub feedback_bindings: Vec<FeedbackBinding>,
}
