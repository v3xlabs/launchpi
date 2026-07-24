use serde::{Deserialize, Serialize};

use crate::models::{
    identifiers::IntegrationId,
    rendered_state::RenderedStateOverride,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FeedbackBinding {
    pub feedback: Feedback,
    pub state: RenderedStateOverride,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Feedback {
    pub integration_id: IntegrationId,
    pub feedback_name: String,
    pub parameters: serde_json::Value,
}
