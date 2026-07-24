use serde::{Deserialize, Serialize};

use crate::models::identifiers::{IntegrationId, PanelId};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionBinding {
    pub gesture: ActionTrigger,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ActionTrigger {
    Press,
    Release,
    Hold { duration_ms: u64 },
    RotateClockwise,
    RotateCounterClockwise,
    ValueChanged,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Action {
    InvokeIntegration {
        integration_id: IntegrationId,
        action_name: String,
        parameters: serde_json::Value,
    },
    SetVariable {
        variable_name: String,
        value: serde_json::Value,
    },
    ChangePanel {
        panel_id: PanelId,
    },
    Wait {
        duration_ms: u64,
    },
}
