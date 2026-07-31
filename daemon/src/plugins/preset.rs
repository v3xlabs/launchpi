use std::{collections::HashMap, sync::RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    bindings::action::Action,
    identifiers::IntegrationId,
    panels::{
        control::ControlTemplate,
        rendered_state::{ColorBinding, Layer, RenderedState, ValueBinding},
    },
};

/// `self` in a preset means "whichever instance published this".
const SELF_INTEGRATION_ID: &str = "self";

/// A ready-made button an instance recommends. Runtime data rather than manifest data: what a Home
/// Assistant installation should offer is its own lights, and a plugin *type* does not know those.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Preset {
    /// Stable across republishes, so the picker does not reorder under the cursor.
    pub preset_id: String,
    /// What the picker groups by, in the plugin's own vocabulary: "Members", "Lights", "Scenes".
    pub category: String,
    pub name: String,
    pub description: Option<String>,
    /// Deliberately not wrapped in a tagged enum: a tag would inject a `kind` key into the table,
    /// and pasting this into `panels.toml` is the point. A future `panel` sibling is additive.
    pub control: ControlTemplate,
}

/// Replace, not merge, for the same reason `subscribe` replaces: an instance that has lost sight of
/// a light must be able to withdraw the button for it, and merging cannot express removal.
#[derive(Default)]
pub struct PresetStore {
    by_instance: RwLock<HashMap<IntegrationId, Vec<Preset>>>,
}

impl PresetStore {
    /// Answers whether anything actually changed, so an instance republishing the same
    /// recommendations on every poll tells the browser nothing.
    pub fn set(&self, integration_id: IntegrationId, presets: Vec<Preset>) -> bool {
        let mut by_instance = self.by_instance.write().unwrap();
        match by_instance.get(&integration_id) {
            Some(existing) if *existing == presets => false,
            _ if presets.is_empty() => by_instance.remove(&integration_id).is_some(),
            _ => {
                by_instance.insert(integration_id, presets);
                true
            }
        }
    }

    pub fn clear_instance(&self, integration_id: &IntegrationId) -> bool {
        self.by_instance
            .write()
            .unwrap()
            .remove(integration_id)
            .is_some()
    }

    pub fn snapshot(&self) -> Vec<(IntegrationId, Vec<Preset>)> {
        let mut entries: Vec<_> = self
            .by_instance
            .read()
            .unwrap()
            .iter()
            .map(|(integration_id, presets)| (integration_id.clone(), presets.clone()))
            .collect();
        entries.sort_by(|left, right| left.0 .0.cmp(&right.0 .0));
        entries
    }
}

/// Rewrites the `self` sigil to the publishing instance.
///
/// Done once on the way in rather than in the browser at drop time, so everything downstream — the
/// API, the picker, the panel file that ends up on disk — sees a preset that already names a real
/// instance. A `$(self:...)` that escaped into a saved panel would interpolate to nothing and look
/// like the button simply did not work.
pub fn substitute_self(preset: &mut Preset, integration_id: &IntegrationId) {
    let control = &mut preset.control;
    substitute_in_state(&mut control.default_state, integration_id);
    if let Some(pressed) = control.pressed_state.as_mut() {
        substitute_in_state(pressed, integration_id);
    }
    for binding in &mut control.action_bindings {
        for action in &mut binding.actions {
            substitute_in_action(action, integration_id);
        }
    }
}

/// Every layer is destructured down to its last field for the same reason `references_of_state` is:
/// a binding missed here keeps the `self` sigil, and a `$(self:...)` that reaches a saved panel
/// interpolates to nothing and reads as a button that simply does not work.
fn substitute_in_state(state: &mut RenderedState, integration_id: &IntegrationId) {
    let rewrite_color = |color: &mut ColorBinding| {
        if let ColorBinding::Reference(reference) = color {
            *reference = rewritten(reference, integration_id);
        }
    };

    for layer in &mut state.layers {
        match layer {
            Layer::Fill { color } => rewrite_color(color),
            Layer::Image {
                image,
                fit: _,
                anchor: _,
                scale_percent: _,
                tint,
            } => {
                image.0 = rewritten(&image.0, integration_id);
                if let Some(tint) = tint {
                    rewrite_color(tint);
                }
            }
            Layer::Text {
                text,
                color,
                anchor: _,
                font_family: _,
                font_size: _,
            } => {
                *text = rewritten(text, integration_id);
                rewrite_color(color);
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
                        *reference = rewritten(reference, integration_id);
                    }
                }
                rewrite_color(color);
            }
            Layer::Border { color, width: _ } => rewrite_color(color),
        }
    }
}

fn substitute_in_action(action: &mut Action, integration_id: &IntegrationId) {
    match action {
        Action::InvokeIntegration {
            integration_id: target,
            parameters,
            ..
        } => {
            if target.0 == SELF_INTEGRATION_ID {
                *target = integration_id.clone();
            }
            substitute_in_json(parameters, integration_id);
        }
        Action::SetVariable { value, .. } => substitute_in_json(value, integration_id),
        _ => {}
    }
}

fn substitute_in_json(value: &mut JsonValue, integration_id: &IntegrationId) {
    match value {
        JsonValue::String(text) => *text = rewritten(text, integration_id),
        JsonValue::Array(items) => {
            for item in items {
                substitute_in_json(item, integration_id);
            }
        }
        JsonValue::Object(fields) => {
            for field in fields.values_mut() {
                substitute_in_json(field, integration_id);
            }
        }
        _ => {}
    }
}

/// An exact substring replace is safe because an integration id cannot contain `(` or `:`, so
/// `$(self:` is the only spelling the sigil can appear in.
fn rewritten(text: &str, integration_id: &IntegrationId) -> String {
    text.replace(
        &format!("$({SELF_INTEGRATION_ID}:"),
        &format!("$({}:", integration_id.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bindings::action::{ActionBinding, ActionTrigger},
        identifiers::AssetId,
        panels::rendered_state::{Anchor9, Fit, RgbaColor},
    };
    use serde_json::json;

    fn preset() -> Preset {
        Preset {
            preset_id: "member-0".to_string(),
            category: "Members".to_string(),
            name: "Member 1".to_string(),
            description: None,
            control: ControlTemplate {
                name: "Member 1".to_string(),
                default_state: RenderedState {
                    layers: vec![
                        Layer::Fill {
                            color: ColorBinding::Reference("$(self:fill)".to_string()),
                        },
                        Layer::Image {
                            image: AssetId("$(self:channel_members_0_image)".to_string()),
                            fit: Fit::Cover,
                            anchor: Anchor9::Center,
                            scale_percent: 100,
                            tint: Some(ColorBinding::Reference("$(self:tint)".to_string())),
                        },
                        Layer::Text {
                            text: "$(self:channel_members_0)".to_string(),
                            color: ColorBinding::Reference("$(self:text)".to_string()),
                            anchor: Anchor9::Center,
                            font_family: None,
                            font_size: None,
                        },
                        Layer::Bar {
                            value: ValueBinding::Reference("$(self:position)".to_string()),
                            maximum: 100.into(),
                            color: RgbaColor::opaque(255, 255, 255).into(),
                            edge: Default::default(),
                            thickness: 6,
                        },
                        Layer::Border {
                            color: ColorBinding::Reference(
                                "$(self:channel_members_0_status_color)".to_string(),
                            ),
                            width: 5,
                        },
                    ],
                    is_pressed: false,
                },
                pressed_state: None,
                action_bindings: vec![ActionBinding {
                    gesture: ActionTrigger::Press,
                    actions: vec![Action::InvokeIntegration {
                        integration_id: IntegrationId(SELF_INTEGRATION_ID.to_string()),
                        action_name: "mute_member".to_string(),
                        parameters: json!({"user_id": "$(self:channel_members_0_id)", "mute": true}),
                    }],
                }],
            },
        }
    }

    #[test]
    fn self_becomes_the_publishing_instance_in_every_bindable_field() {
        let mut subject = preset();
        substitute_self(&mut subject, &IntegrationId("discord.home".to_string()));

        // Every layer, and every bindable field of every layer, or the sigil escapes into a panel.
        assert_eq!(
            subject.control.default_state.layers,
            vec![
                Layer::Fill {
                    color: ColorBinding::Reference("$(discord.home:fill)".to_string()),
                },
                Layer::Image {
                    image: AssetId("$(discord.home:channel_members_0_image)".to_string()),
                    fit: Fit::Cover,
                    anchor: Anchor9::Center,
                    scale_percent: 100,
                    tint: Some(ColorBinding::Reference("$(discord.home:tint)".to_string())),
                },
                Layer::Text {
                    text: "$(discord.home:channel_members_0)".to_string(),
                    color: ColorBinding::Reference("$(discord.home:text)".to_string()),
                    anchor: Anchor9::Center,
                    font_family: None,
                    font_size: None,
                },
                Layer::Bar {
                    value: ValueBinding::Reference("$(discord.home:position)".to_string()),
                    maximum: 100.into(),
                    color: RgbaColor::opaque(255, 255, 255).into(),
                    edge: Default::default(),
                    thickness: 6,
                },
                Layer::Border {
                    color: ColorBinding::Reference(
                        "$(discord.home:channel_members_0_status_color)".to_string()
                    ),
                    width: 5,
                },
            ]
        );

        let Action::InvokeIntegration {
            integration_id,
            parameters,
            ..
        } = &subject.control.action_bindings[0].actions[0]
        else {
            panic!("the binding is an integration call");
        };
        assert_eq!(integration_id.0, "discord.home");
        assert_eq!(
            parameters.get("user_id").and_then(JsonValue::as_str),
            Some("$(discord.home:channel_members_0_id)"),
            "a sigil nested in action parameters must be rewritten too"
        );
    }

    #[test]
    fn a_preset_that_already_names_an_instance_is_left_alone() {
        let mut subject = preset();
        subject.control.default_state.layers = vec![Layer::Text {
            text: "$(hass.home:light.kitchen.state)".to_string(),
            color: RgbaColor::opaque(255, 255, 255).into(),
            anchor: Anchor9::Center,
            font_family: None,
            font_size: None,
        }];
        substitute_self(&mut subject, &IntegrationId("discord.home".to_string()));

        assert_eq!(
            subject.control.default_state.layers,
            vec![Layer::Text {
                text: "$(hass.home:light.kitchen.state)".to_string(),
                color: RgbaColor::opaque(255, 255, 255).into(),
                anchor: Anchor9::Center,
                font_family: None,
                font_size: None,
            }]
        );
    }

    #[test]
    fn republishing_the_same_recommendations_reports_no_change() {
        let store = PresetStore::default();
        let instance = IntegrationId("discord.home".to_string());

        assert!(store.set(instance.clone(), vec![preset()]));
        assert!(!store.set(instance.clone(), vec![preset()]));
        assert!(
            store.set(instance, Vec::new()),
            "and withdrawal is a change"
        );
    }

    #[test]
    fn clearing_an_instance_withdraws_only_its_own() {
        let store = PresetStore::default();
        let discord = IntegrationId("discord.home".to_string());
        let hass = IntegrationId("hass.home".to_string());
        store.set(discord.clone(), vec![preset()]);
        store.set(hass.clone(), vec![preset()]);

        assert!(store.clear_instance(&discord));
        assert_eq!(
            store
                .snapshot()
                .into_iter()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec![hass]
        );
    }
}
