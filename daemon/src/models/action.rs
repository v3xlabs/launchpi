use serde::{Deserialize, Serialize};

use crate::models::identifiers::{IntegrationId, PanelId};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionBinding {
    pub gesture: ActionTrigger,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionTrigger {
    Press,
    Release,
    Hold { duration_ms: u64 },
    RotateClockwise,
    RotateCounterClockwise,
    ValueChanged,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Serialize)]
    struct Bindings {
        binding: Vec<ActionBinding>,
    }

    #[test]
    fn a_binding_reads_from_the_tagged_toml_form() {
        let document: Bindings = toml::from_str(
            r#"
            [[binding]]
            gesture = "press"

            [[binding.actions]]
            type = "invoke_integration"
            integration_id = "hass.home"
            action_name = "light.toggle"
            parameters = { entity_id = "light.kitchen" }

            [[binding.actions]]
            type = "wait"
            duration_ms = 200
            "#,
        )
        .expect("the documented form parses");

        assert_eq!(document.binding[0].gesture, ActionTrigger::Press);
        assert_eq!(
            document.binding[0].actions,
            vec![
                Action::InvokeIntegration {
                    integration_id: IntegrationId("hass.home".to_string()),
                    action_name: "light.toggle".to_string(),
                    parameters: serde_json::json!({ "entity_id": "light.kitchen" }),
                },
                Action::Wait { duration_ms: 200 },
            ]
        );
    }

    #[test]
    fn a_hold_gesture_carries_its_duration() {
        let document: Bindings = toml::from_str(
            r#"
            [[binding]]
            gesture = { hold = { duration_ms = 800 } }
            actions = []
            "#,
        )
        .expect("the documented form parses");
        assert_eq!(
            document.binding[0].gesture,
            ActionTrigger::Hold { duration_ms: 800 }
        );
    }

    #[test]
    fn bindings_survive_a_toml_round_trip() {
        let original = Bindings {
            binding: vec![ActionBinding {
                gesture: ActionTrigger::RotateClockwise,
                actions: vec![
                    Action::SetVariable {
                        variable_name: "brightness".to_string(),
                        value: serde_json::json!(50),
                    },
                    Action::ChangePanel {
                        panel_id: PanelId("studio-panel-1".to_string()),
                    },
                ],
            }],
        };
        let rendered = toml::to_string_pretty(&original).expect("serializes");
        let parsed: Bindings = toml::from_str(&rendered).expect("round-trips");
        assert_eq!(parsed.binding, original.binding);
    }
}
