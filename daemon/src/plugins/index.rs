use std::collections::{HashMap, HashSet};

use crate::{
    models::{
        control::Control,
        identifiers::{IntegrationId, SurfaceId},
        panel::Panel,
        rendered_state::{RenderedState, RenderedStateOverride},
    },
    plugins::{
        feedback::FeedbackKey,
        plugin::Subscription,
        variables::{self, VariableRef},
    },
    state::key_index_for,
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RenderTarget {
    pub surface_id: SurfaceId,
    pub key_index: u8,
}

/// What every visible key depends on, inverted. A variable change looks up exactly the keys that
/// reference it rather than repainting a panel.
#[derive(Default)]
pub struct DependencyIndex {
    by_variable: HashMap<VariableRef, HashSet<RenderTarget>>,
    by_feedback: HashMap<FeedbackKey, HashSet<RenderTarget>>,
    subscriptions: HashMap<IntegrationId, Vec<Subscription>>,
}

impl DependencyIndex {
    pub fn build(active_panels: &[(SurfaceId, Panel)]) -> Self {
        let mut index = Self::default();
        for (surface_id, panel) in active_panels {
            for control in &panel.controls {
                let Some(key_index) = key_index_for(control, panel.layout.columns) else {
                    continue;
                };
                index.add_control(
                    RenderTarget {
                        surface_id: surface_id.clone(),
                        key_index,
                    },
                    control,
                );
            }
        }
        index.build_subscriptions();
        index
    }

    fn add_control(&mut self, target: RenderTarget, control: &Control) {
        for state in [Some(&control.default_state), control.pressed_state.as_ref()]
            .into_iter()
            .flatten()
        {
            for reference in references_of_state(state) {
                self.by_variable
                    .entry(reference)
                    .or_default()
                    .insert(target.clone());
            }
        }
        for binding in &control.feedback_bindings {
            self.by_feedback
                .entry(FeedbackKey::new(&binding.feedback))
                .or_default()
                .insert(target.clone());
            for reference in references_of_override(&binding.state) {
                self.by_variable
                    .entry(reference)
                    .or_default()
                    .insert(target.clone());
            }
        }
    }

    fn build_subscriptions(&mut self) {
        for reference in self.by_variable.keys() {
            self.subscriptions
                .entry(reference.integration_id.clone())
                .or_default()
                .push(Subscription::Variable {
                    name: reference.name.clone(),
                });
        }
        for key in self.by_feedback.keys() {
            self.subscriptions
                .entry(key.integration_id.clone())
                .or_default()
                .push(Subscription::Feedback {
                    feedback_name: key.feedback_name.clone(),
                    parameters: key.parameters(),
                });
        }
    }

    pub fn targets_for_variable(&self, reference: &VariableRef) -> Vec<RenderTarget> {
        self.by_variable
            .get(reference)
            .map(|targets| targets.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn targets_for_feedback(&self, key: &FeedbackKey) -> Vec<RenderTarget> {
        self.by_feedback
            .get(key)
            .map(|targets| targets.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// The feedbacks belonging to one instance that something on screen is actually watching.
    pub fn feedback_keys_for(&self, integration_id: &IntegrationId) -> Vec<FeedbackKey> {
        self.by_feedback
            .keys()
            .filter(|key| key.integration_id == *integration_id)
            .cloned()
            .collect()
    }

    pub fn subscriptions_for(&self, integration_id: &IntegrationId) -> &[Subscription] {
        self.subscriptions
            .get(integration_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn watched_integrations(&self) -> Vec<IntegrationId> {
        self.subscriptions.keys().cloned().collect()
    }
}

fn references_of_state(state: &RenderedState) -> Vec<VariableRef> {
    let mut found = state
        .text
        .as_deref()
        .map(variables::references)
        .unwrap_or_default();
    if let Some(image) = &state.image {
        found.extend(variables::references(&image.0));
    }
    found
}

fn references_of_override(state: &RenderedStateOverride) -> Vec<VariableRef> {
    let mut found = state
        .text
        .as_deref()
        .map(variables::references)
        .unwrap_or_default();
    if let Some(image) = &state.image {
        found.extend(variables::references(&image.0));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        feedback::{Feedback, FeedbackBinding},
        identifiers::{AssetId, ControlId, PanelId},
        panel::PanelLayout,
        surface::{SurfaceCapabilities, SurfacePosition},
    };

    fn control(column: u16, row: u16, text: Option<&str>) -> Control {
        Control {
            control_id: ControlId(format!("control-{column}-{row}")),
            name: "Key".to_string(),
            position: SurfacePosition { column, row },
            default_state: RenderedState {
                text: text.map(str::to_string),
                ..RenderedState::default()
            },
            pressed_state: None,
            action_bindings: Vec::new(),
            feedback_bindings: Vec::new(),
        }
    }

    fn panel(controls: Vec<Control>) -> Panel {
        Panel {
            panel_id: PanelId("panel".to_string()),
            name: "Panel".to_string(),
            layout: PanelLayout {
                columns: 4,
                rows: 2,
            },
            capabilities: SurfaceCapabilities::default(),
            controls,
            dial_colors: Vec::new(),
            dial_ring_levels: Vec::new(),
        }
    }

    fn surface() -> SurfaceId {
        SurfaceId("studio".to_string())
    }

    #[test]
    fn a_variable_maps_to_exactly_the_keys_that_reference_it() {
        let index = DependencyIndex::build(&[(
            surface(),
            panel(vec![
                control(0, 0, Some("$(http.local:value)")),
                control(1, 0, Some("static")),
                control(2, 1, Some("also $(http.local:value)")),
            ]),
        )]);

        let mut targets = index.targets_for_variable(&VariableRef::new("http.local", "value"));
        targets.sort_by_key(|target| target.key_index);
        assert_eq!(
            targets,
            vec![
                RenderTarget {
                    surface_id: surface(),
                    key_index: 0
                },
                RenderTarget {
                    surface_id: surface(),
                    key_index: 6
                },
            ]
        );
    }

    #[test]
    fn an_unreferenced_variable_maps_to_nothing() {
        let index =
            DependencyIndex::build(&[(surface(), panel(vec![control(0, 0, Some("plain"))]))]);
        assert!(index
            .targets_for_variable(&VariableRef::new("http.local", "value"))
            .is_empty());
    }

    #[test]
    fn an_image_reference_is_tracked_like_a_text_reference() {
        let mut keyed = control(0, 0, None);
        keyed.default_state.image = Some(AssetId("$(mpris.default:art)".to_string()));
        let index = DependencyIndex::build(&[(surface(), panel(vec![keyed]))]);
        assert_eq!(
            index
                .targets_for_variable(&VariableRef::new("mpris.default", "art"))
                .len(),
            1
        );
    }

    #[test]
    fn a_feedback_binding_registers_its_key_and_a_subscription() {
        let mut keyed = control(0, 0, None);
        let feedback = Feedback {
            integration_id: IntegrationId("hass.home".to_string()),
            feedback_name: "state_is".to_string(),
            parameters: serde_json::json!({ "entity_id": "light.kitchen" }),
        };
        keyed.feedback_bindings.push(FeedbackBinding {
            feedback: feedback.clone(),
            state: RenderedStateOverride::default(),
        });
        let index = DependencyIndex::build(&[(surface(), panel(vec![keyed]))]);

        assert_eq!(
            index
                .targets_for_feedback(&FeedbackKey::new(&feedback))
                .len(),
            1
        );
        let integration_id = IntegrationId("hass.home".to_string());
        assert_eq!(index.feedback_keys_for(&integration_id).len(), 1);
        assert_eq!(
            index.subscriptions_for(&integration_id),
            &[Subscription::Feedback {
                feedback_name: "state_is".to_string(),
                parameters: serde_json::json!({ "entity_id": "light.kitchen" }),
            }]
        );
    }

    #[test]
    fn two_keys_watching_the_same_feedback_share_one_key() {
        let feedback = Feedback {
            integration_id: IntegrationId("hass.home".to_string()),
            feedback_name: "state_is".to_string(),
            parameters: serde_json::json!({ "entity_id": "light.kitchen" }),
        };
        let binding = FeedbackBinding {
            feedback: feedback.clone(),
            state: RenderedStateOverride::default(),
        };
        let mut first = control(0, 0, None);
        first.feedback_bindings.push(binding.clone());
        let mut second = control(1, 0, None);
        second.feedback_bindings.push(binding);

        let index = DependencyIndex::build(&[(surface(), panel(vec![first, second]))]);
        assert_eq!(
            index
                .targets_for_feedback(&FeedbackKey::new(&feedback))
                .len(),
            2
        );
        assert_eq!(
            index
                .feedback_keys_for(&IntegrationId("hass.home".to_string()))
                .len(),
            1
        );
    }

    #[test]
    fn an_empty_workspace_watches_nothing() {
        let index = DependencyIndex::build(&[]);
        assert!(index.watched_integrations().is_empty());
    }
}
