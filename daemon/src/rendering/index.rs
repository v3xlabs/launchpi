use std::collections::{HashMap, HashSet};

use crate::{
    identifiers::{IntegrationId, SurfaceId},
    panels::{
        control::Control,
        rendered_state::{ColorBinding, Layer, RenderedState, ValueBinding},
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

/// Every field a layer can bind, not just the textual ones. A reference that is missed here is
/// invisible twice over: its plugin is never told to watch the value, and a change to it never
/// marks the key dirty.
///
/// Every layer is destructured down to its last field, with the ones that cannot bind named and
/// discarded, so that adding a field to a layer fails to compile here until someone has decided
/// whether it binds.
fn references_of_state(state: &RenderedState) -> Vec<VariableRef> {
    let mut found = Vec::new();
    for layer in &state.layers {
        match layer {
            Layer::Fill { color } => extend_from_color(&mut found, color),
            Layer::Image {
                image,
                fit: _,
                anchor: _,
                scale_percent: _,
                tint,
            } => {
                found.extend(template::references(&image.0));
                if let Some(tint) = tint {
                    extend_from_color(&mut found, tint);
                }
            }
            Layer::Text {
                text,
                color,
                anchor: _,
            } => {
                found.extend(template::references(text));
                extend_from_color(&mut found, color);
            }
            Layer::Bar {
                value,
                maximum,
                color,
                edge: _,
                thickness: _,
            } => {
                for binding in [value, maximum] {
                    if let ValueBinding::Reference(reference) = binding {
                        found.extend(template::references(reference));
                    }
                }
                extend_from_color(&mut found, color);
            }
            Layer::Border { color, width: _ } => extend_from_color(&mut found, color),
        }
    }
    found
}

fn extend_from_color(found: &mut Vec<VariableRef>, binding: &ColorBinding) {
    if let ColorBinding::Reference(reference) = binding {
        found.extend(template::references(reference));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        identifiers::{AssetId, ControlId, PanelId},
        panels::{
            rendered_state::{Anchor9, Fit, RgbaColor},
            PanelLayout,
        },
        surfaces::layout::{SurfaceCapabilities, SurfacePosition},
    };

    fn control(column: u16, row: u16, text: Option<&str>) -> Control {
        Control {
            control_id: ControlId(format!("control-{column}-{row}")),
            name: "Key".to_string(),
            position: SurfacePosition { column, row },
            default_state: RenderedState {
                layers: text
                    .map(|text| {
                        vec![Layer::Text {
                            text: text.to_string(),
                            color: RgbaColor::opaque(255, 255, 255).into(),
                            anchor: Anchor9::Center,
                        }]
                    })
                    .unwrap_or_default(),
                is_pressed: false,
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
            dials: Vec::new(),
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
    fn a_border_and_an_overlay_register_targets_and_subscriptions() {
        let mut keyed = control(0, 0, None);
        keyed.default_state.layers = vec![
            Layer::Border {
                color: ColorBinding::Reference("$(discord.home:status_color)".to_string()),
                width: 5,
            },
            Layer::Image {
                image: AssetId("$(discord.home:status_icon)".to_string()),
                fit: Fit::Contain,
                anchor: Anchor9::BottomEnd,
                scale_percent: 32,
                tint: Some(ColorBinding::Reference("$(discord.home:tint)".to_string())),
            },
        ];
        let index = DependencyIndex::build(&[(surface(), panel(vec![keyed]))]);

        assert_eq!(
            index
                .targets_for_variable(&VariableRef::new("discord.home", "status_color"))
                .len(),
            1,
            "a border colour must mark its key dirty when the colour changes"
        );
        assert_eq!(
            index
                .targets_for_variable(&VariableRef::new("discord.home", "status_icon"))
                .len(),
            1,
            "a badge must mark its key dirty when the badge changes"
        );

        let mut names: Vec<_> = index
            .subscriptions_for(&IntegrationId("discord.home".to_string()))
            .iter()
            .map(|subscription| subscription.name.clone())
            .collect();
        names.sort();
        assert_eq!(names, vec!["status_color", "status_icon", "tint"]);
    }

    #[test]
    fn a_colour_binding_registers_a_target_and_a_subscription() {
        let mut keyed = control(0, 0, None);
        keyed.default_state.layers = vec![
            Layer::Fill {
                color: ColorBinding::Reference("$(hass.home:light.kitchen.color)".to_string()),
            },
            Layer::Text {
                text: "Kitchen".to_string(),
                color: ColorBinding::Reference("$(hass.home:light.kitchen.text_color)".to_string()),
                anchor: Anchor9::Center,
            },
        ];
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
        keyed.default_state.layers = vec![Layer::Fill {
            color: RgbaColor::opaque(1, 2, 3).into(),
        }];
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
        keyed.default_state.layers = vec![Layer::Image {
            image: AssetId("$(mpris.default:art)".to_string()),
            fit: Fit::Cover,
            anchor: Anchor9::Center,
            scale_percent: 100,
            tint: None,
        }];
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
