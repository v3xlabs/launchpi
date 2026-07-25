use std::collections::{HashMap, HashSet};

use crate::{
    identifiers::{IntegrationId, SurfaceId},
    panels::{
        control::Control,
        rendered_state::{ColorBinding, RenderedState},
        Panel,
    },
    plugins::plugin::Subscription,
    surfaces::keys::key_index_for,
    variables::{template, VariableRef},
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
    }

    fn build_subscriptions(&mut self) {
        for reference in self.by_variable.keys() {
            self.subscriptions
                .entry(reference.integration_id.clone())
                .or_default()
                .push(Subscription {
                    name: reference.name.clone(),
                });
        }
    }

    pub fn targets_for_variable(&self, reference: &VariableRef) -> Vec<RenderTarget> {
        self.by_variable
            .get(reference)
            .map(|targets| targets.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn subscriptions_for(&self, integration_id: &IntegrationId) -> &[Subscription] {
        self.subscriptions
            .get(integration_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Every key the index knows about, for the rare case where what changed is not a value.
    pub fn every_target(&self) -> Vec<RenderTarget> {
        let mut targets: HashSet<RenderTarget> = HashSet::new();
        for known in self.by_variable.values() {
            targets.extend(known.iter().cloned());
        }
        targets.into_iter().collect()
    }

    pub fn watched_integrations(&self) -> Vec<IntegrationId> {
        self.subscriptions.keys().cloned().collect()
    }
}

/// Every field a control can bind, not just the textual ones. A colour reference that is missed
/// here is invisible twice over: its plugin is never told to watch the value, and a change to it
/// never marks the key dirty.
fn references_of_state(state: &RenderedState) -> Vec<VariableRef> {
    let mut found = state
        .text
        .as_deref()
        .map(template::references)
        .unwrap_or_default();
    if let Some(image) = &state.image {
        found.extend(template::references(&image.0));
    }
    for binding in [
        state.foreground_color.as_ref(),
        state.background_color.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if let ColorBinding::Reference(reference) = binding {
            found.extend(template::references(reference));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        identifiers::{AssetId, ControlId, PanelId},
        panels::PanelLayout,
        surfaces::layout::{SurfaceCapabilities, SurfacePosition},
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
    fn a_colour_binding_registers_a_target_and_a_subscription() {
        let mut keyed = control(0, 0, None);
        keyed.default_state.background_color = Some(ColorBinding::Reference(
            "$(hass.home:light.kitchen.color)".to_string(),
        ));
        keyed.default_state.foreground_color = Some(ColorBinding::Reference(
            "$(hass.home:light.kitchen.text_color)".to_string(),
        ));
        let index = DependencyIndex::build(&[(surface(), panel(vec![keyed]))]);

        assert_eq!(
            index
                .targets_for_variable(&VariableRef::new("hass.home", "light.kitchen.color"))
                .len(),
            1,
            "a colour binding must mark its key dirty when the colour changes"
        );
        assert_eq!(
            index
                .targets_for_variable(&VariableRef::new("hass.home", "light.kitchen.text_color"))
                .len(),
            1
        );

        let mut names: Vec<_> = index
            .subscriptions_for(&IntegrationId("hass.home".to_string()))
            .iter()
            .map(|subscription| subscription.name.clone())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec!["light.kitchen.color", "light.kitchen.text_color"],
            "the plugin must be told to watch the colours something is showing"
        );
    }

    #[test]
    fn a_literal_colour_subscribes_to_nothing() {
        let mut keyed = control(0, 0, None);
        keyed.default_state.background_color =
            Some(crate::panels::rendered_state::RgbaColor::opaque(1, 2, 3).into());
        let index = DependencyIndex::build(&[(surface(), panel(vec![keyed]))]);

        assert!(index.watched_integrations().is_empty());
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
    fn an_empty_workspace_watches_nothing() {
        let index = DependencyIndex::build(&[]);
        assert!(index.watched_integrations().is_empty());
    }
}
