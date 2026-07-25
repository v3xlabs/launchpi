use crate::{
    models::{control::Control, identifiers::AssetId, rendered_state::RenderedState},
    plugins::{
        feedback::{FeedbackCache, FeedbackKey},
        variables::{self, VariableStore},
    },
};

/// The live state a control resolves against. Cheap to build: it borrows the two stores and takes
/// a read lock per lookup, so a full panel repaint costs one uncontended lock per key.
#[derive(Clone, Copy)]
pub struct RenderContext<'a> {
    variables: &'a VariableStore,
    feedbacks: &'a FeedbackCache,
}

impl<'a> RenderContext<'a> {
    pub fn new(variables: &'a VariableStore, feedbacks: &'a FeedbackCache) -> Self {
        Self {
            variables,
            feedbacks,
        }
    }

    /// Applies every active feedback in order, then interpolates what is left.
    pub fn resolve(&self, control: &Control, is_pressed: bool) -> RenderedState {
        let mut state = if is_pressed {
            control
                .pressed_state
                .clone()
                .unwrap_or_else(|| control.default_state.clone())
        } else {
            control.default_state.clone()
        };

        for binding in &control.feedback_bindings {
            if self
                .feedbacks
                .is_active(&FeedbackKey::new(&binding.feedback))
            {
                state.overlay(&binding.state);
            }
        }

        state.text = state
            .text
            .map(|text| self.interpolate(&text))
            .filter(|text| !text.is_empty());
        state.image = state.image.and_then(|asset| self.resolve_asset(&asset));
        state
    }

    pub fn interpolate(&self, template: &str) -> String {
        variables::interpolate(template, |reference| self.variables.text(reference))
    }

    /// An image slot may hold a literal asset id or a reference that yields one. A reference that
    /// resolves to nothing leaves the button without an image rather than with a broken id.
    fn resolve_asset(&self, asset: &AssetId) -> Option<AssetId> {
        if !variables::has_reference(&asset.0) {
            return Some(asset.clone());
        }
        let resolved = self.interpolate(&asset.0);
        (!resolved.is_empty()).then_some(AssetId(resolved))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{
            feedback::{Feedback, FeedbackBinding},
            identifiers::{ControlId, IntegrationId},
            rendered_state::{RenderedStateOverride, RgbaColor},
            surface::SurfacePosition,
        },
        plugins::variables::{VariableRef, VariableValue},
    };

    fn color(red: u8, green: u8, blue: u8) -> RgbaColor {
        RgbaColor {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    fn control(default_state: RenderedState) -> Control {
        Control {
            control_id: ControlId("key".to_string()),
            name: "Key".to_string(),
            position: SurfacePosition { column: 0, row: 0 },
            default_state,
            pressed_state: None,
            action_bindings: Vec::new(),
            feedback_bindings: Vec::new(),
        }
    }

    fn feedback_binding(name: &str, state: RenderedStateOverride) -> FeedbackBinding {
        FeedbackBinding {
            feedback: Feedback {
                integration_id: IntegrationId("hass.home".to_string()),
                feedback_name: name.to_string(),
                parameters: serde_json::json!({}),
            },
            state,
        }
    }

    #[test]
    fn text_is_interpolated_from_the_variable_store() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("mpris.default", "title"),
            VariableValue::Text("Blue Monday".to_string()),
        );
        let feedbacks = FeedbackCache::default();
        let context = RenderContext::new(&variables, &feedbacks);

        let resolved = context.resolve(
            &control(RenderedState {
                text: Some("$(mpris.default:title)".to_string()),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.text, Some("Blue Monday".to_string()));
    }

    #[test]
    fn text_that_resolves_to_nothing_leaves_the_key_blank() {
        let variables = VariableStore::default();
        let feedbacks = FeedbackCache::default();
        let context = RenderContext::new(&variables, &feedbacks);

        let resolved = context.resolve(
            &control(RenderedState {
                text: Some("$(mpris.default:title)".to_string()),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.text, None);
    }

    #[test]
    fn an_inactive_feedback_contributes_nothing() {
        let variables = VariableStore::default();
        let feedbacks = FeedbackCache::default();
        let context = RenderContext::new(&variables, &feedbacks);

        let mut subject = control(RenderedState {
            background_color: Some(color(10, 10, 10)),
            ..RenderedState::default()
        });
        subject.feedback_bindings.push(feedback_binding(
            "is_on",
            RenderedStateOverride {
                background_color: Some(color(232, 185, 35)),
                ..RenderedStateOverride::default()
            },
        ));

        let resolved = context.resolve(&subject, false);
        assert_eq!(resolved.background_color, Some(color(10, 10, 10)));
    }

    #[test]
    fn an_active_feedback_overlays_only_the_fields_it_sets() {
        let variables = VariableStore::default();
        let feedbacks = FeedbackCache::default();
        let binding = feedback_binding(
            "is_on",
            RenderedStateOverride {
                background_color: Some(color(232, 185, 35)),
                ..RenderedStateOverride::default()
            },
        );
        feedbacks.set(FeedbackKey::new(&binding.feedback), true);
        let context = RenderContext::new(&variables, &feedbacks);

        let mut subject = control(RenderedState {
            text: Some("Kitchen".to_string()),
            background_color: Some(color(10, 10, 10)),
            ..RenderedState::default()
        });
        subject.feedback_bindings.push(binding);

        let resolved = context.resolve(&subject, false);
        assert_eq!(resolved.background_color, Some(color(232, 185, 35)));
        assert_eq!(resolved.text, Some("Kitchen".to_string()));
    }

    #[test]
    fn a_later_feedback_wins_the_fields_it_sets() {
        let variables = VariableStore::default();
        let feedbacks = FeedbackCache::default();
        let first = feedback_binding(
            "is_on",
            RenderedStateOverride {
                background_color: Some(color(1, 1, 1)),
                text: Some("first".to_string()),
                ..RenderedStateOverride::default()
            },
        );
        let second = feedback_binding(
            "is_bright",
            RenderedStateOverride {
                background_color: Some(color(2, 2, 2)),
                ..RenderedStateOverride::default()
            },
        );
        feedbacks.set(FeedbackKey::new(&first.feedback), true);
        feedbacks.set(FeedbackKey::new(&second.feedback), true);
        let context = RenderContext::new(&variables, &feedbacks);

        let mut subject = control(RenderedState::default());
        subject.feedback_bindings.push(first);
        subject.feedback_bindings.push(second);

        let resolved = context.resolve(&subject, false);
        assert_eq!(resolved.background_color, Some(color(2, 2, 2)));
        assert_eq!(resolved.text, Some("first".to_string()));
    }

    #[test]
    fn an_image_reference_resolves_to_the_published_asset() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("mpris.default", "art"),
            VariableValue::Image(AssetId("hash:abc123".to_string())),
        );
        let feedbacks = FeedbackCache::default();
        let context = RenderContext::new(&variables, &feedbacks);

        let resolved = context.resolve(
            &control(RenderedState {
                image: Some(AssetId("$(mpris.default:art)".to_string())),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.image, Some(AssetId("hash:abc123".to_string())));
    }

    #[test]
    fn a_literal_asset_id_passes_through_untouched() {
        let variables = VariableStore::default();
        let feedbacks = FeedbackCache::default();
        let context = RenderContext::new(&variables, &feedbacks);

        let resolved = context.resolve(
            &control(RenderedState {
                image: Some(AssetId("builtin:play".to_string())),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.image, Some(AssetId("builtin:play".to_string())));
    }

    #[test]
    fn the_pressed_state_falls_back_to_the_default_state() {
        let variables = VariableStore::default();
        let feedbacks = FeedbackCache::default();
        let context = RenderContext::new(&variables, &feedbacks);

        let subject = control(RenderedState {
            text: Some("Play".to_string()),
            ..RenderedState::default()
        });
        assert_eq!(
            context.resolve(&subject, true).text,
            Some("Play".to_string())
        );
    }
}
