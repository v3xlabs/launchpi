use crate::{
    identifiers::AssetId,
    panels::{
        control::Control,
        rendered_state::{
            ColorBinding, Layer, RenderedState, ResolvedLayer, RgbaColor, ValueBinding,
        },
    },
    variables::{template, VariableStore},
};

/// What a control resolved to: every binding replaced by the value it names. This is what the
/// renderer draws, and what the de-duplication ledger hashes.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ResolvedState {
    pub layers: Vec<ResolvedLayer>,
}

/// What a layer whose only content is a colour falls back to when nothing has published one yet.
/// A [`Layer::Fill`] or [`Layer::Border`] in that state is dropped instead: an unanswered plugin
/// should leave a key unstyled rather than paint it black, and that is how a plugin spells
/// "nothing to report". A layer that carries content keeps the content and loses only the colour.
const UNRESOLVED_CONTENT_COLOR: RgbaColor = RgbaColor {
    red: 255,
    green: 255,
    blue: 255,
    alpha: 255,
};

/// The live state a control resolves against. Cheap to build: it borrows the store and takes a
/// read lock per lookup, so a full panel repaint costs one uncontended lock per key.
#[derive(Clone, Copy)]
pub struct RenderContext<'a> {
    variables: &'a VariableStore,
}

impl<'a> RenderContext<'a> {
    pub fn new(variables: &'a VariableStore) -> Self {
        Self { variables }
    }

    /// Every field goes through the same resolution: a literal passes through, a reference is
    /// looked up. There is no overlay pass, because there are no boolean feedbacks to overlay.
    pub fn resolve(&self, control: &Control, is_pressed: bool) -> ResolvedState {
        self.resolve_states(
            &control.default_state,
            control.pressed_state.as_ref(),
            is_pressed,
        )
    }

    /// Resolution without a `Control`, so the web can have an unsaved draft drawn by the same code
    /// that draws the device. Sharing this is what stops the preview from drifting: there is one
    /// implementation of what a binding means, not one per consumer.
    pub fn resolve_states(
        &self,
        default_state: &RenderedState,
        pressed_state: Option<&RenderedState>,
        is_pressed: bool,
    ) -> ResolvedState {
        let state = if is_pressed {
            pressed_state.unwrap_or(default_state)
        } else {
            default_state
        };

        ResolvedState {
            layers: state
                .layers
                .iter()
                .filter_map(|layer| self.resolve_layer(layer))
                .collect(),
        }
    }

    /// A layer that resolves to nothing drawable is dropped rather than drawn as a default, so a
    /// plugin that has not answered yet leaves the key as if the layer were not there.
    fn resolve_layer(&self, layer: &Layer) -> Option<ResolvedLayer> {
        match layer {
            Layer::Fill { color } => Some(ResolvedLayer::Fill {
                color: self.resolve_color(Some(color))?,
            }),
            Layer::Image {
                image,
                fit,
                anchor,
                scale_percent,
                tint,
            } => (*scale_percent > 0).then_some(()).and_then(|()| {
                Some(ResolvedLayer::Image {
                    image: self.resolve_asset(image)?,
                    fit: *fit,
                    anchor: *anchor,
                    scale_percent: *scale_percent,
                    tint: self.resolve_color(tint.as_ref()),
                })
            }),
            Layer::Text {
                text,
                color,
                anchor,
            } => {
                let text = self.interpolate(text);

                (!text.is_empty()).then(|| ResolvedLayer::Text {
                    text,
                    color: self
                        .resolve_color(Some(color))
                        .unwrap_or(UNRESOLVED_CONTENT_COLOR),
                    anchor: *anchor,
                })
            }
            Layer::Bar {
                value,
                maximum,
                color,
                edge,
                thickness,
            } => {
                let maximum = self.resolve_value(maximum)?;

                (maximum > 0 && *thickness > 0).then(|| ResolvedLayer::Bar {
                    value: self.resolve_value(value).unwrap_or(0),
                    maximum,
                    color: self
                        .resolve_color(Some(color))
                        .unwrap_or(UNRESOLVED_CONTENT_COLOR),
                    edge: *edge,
                    thickness: *thickness,
                })
            }
            Layer::Border { color, width } => (*width > 0).then_some(()).and_then(|()| {
                Some(ResolvedLayer::Border {
                    color: self.resolve_color(Some(color))?,
                    width: *width,
                })
            }),
        }
    }

    pub fn interpolate(&self, template: &str) -> String {
        template::interpolate(template, |reference| self.variables.text(reference))
    }

    /// A reference that resolves to something unparseable leaves the colour unset rather than
    /// painting it black, so a plugin that has not answered yet looks like an unstyled key.
    pub fn resolve_color(&self, binding: Option<&ColorBinding>) -> Option<RgbaColor> {
        match binding? {
            ColorBinding::Literal(color) => Some(color.clone()),
            ColorBinding::Reference(reference) => RgbaColor::from_hex(&self.interpolate(reference)),
        }
    }

    /// A reference that resolves to something unparseable leaves the number unset, for the same
    /// reason an unresolved colour does: a plugin that has not answered yet should not be read as
    /// having answered zero.
    fn resolve_value(&self, binding: &ValueBinding) -> Option<u16> {
        match binding {
            ValueBinding::Literal(value) => Some(*value),
            ValueBinding::Reference(reference) => self
                .interpolate(reference)
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .map(|number| number.round().clamp(0.0, f64::from(u16::MAX)) as u16),
        }
    }

    /// An image slot may hold a literal asset id or a reference that yields one. A reference that
    /// resolves to nothing leaves the button without an image rather than with a broken id.
    fn resolve_asset(&self, asset: &AssetId) -> Option<AssetId> {
        if !template::has_reference(&asset.0) {
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
        identifiers::ControlId,
        panels::rendered_state::{Anchor9, Edge, Fit},
        surfaces::layout::SurfacePosition,
        variables::{VariableRef, VariableValue},
    };

    fn control(layers: Vec<Layer>) -> Control {
        Control {
            control_id: ControlId("key".to_string()),
            name: "Key".to_string(),
            position: SurfacePosition { column: 0, row: 0 },
            default_state: RenderedState {
                layers,
                is_pressed: false,
            },
            pressed_state: None,
            action_bindings: Vec::new(),
        }
    }

    fn text(text: &str, color: ColorBinding) -> Layer {
        Layer::Text {
            text: text.to_string(),
            color,
            anchor: Anchor9::Center,
        }
    }

    fn white() -> ColorBinding {
        RgbaColor::opaque(255, 255, 255).into()
    }

    fn only(resolved: ResolvedState) -> Option<ResolvedLayer> {
        resolved.layers.into_iter().next()
    }

    #[test]
    fn text_is_interpolated_from_the_value_store() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("mpris.default", "title"),
            VariableValue::Text("Blue Monday".to_string()),
        );
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![text("$(mpris.default:title)", white())]),
            false,
        );
        assert_eq!(
            only(resolved),
            Some(ResolvedLayer::Text {
                text: "Blue Monday".to_string(),
                color: RgbaColor::opaque(255, 255, 255),
                anchor: Anchor9::Center,
            })
        );
    }

    #[test]
    fn a_literal_colour_passes_through() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);
        let color = RgbaColor::opaque(10, 20, 30);

        let resolved = context.resolve(
            &control(vec![Layer::Fill {
                color: color.clone().into(),
            }]),
            false,
        );
        assert_eq!(only(resolved), Some(ResolvedLayer::Fill { color }));
    }

    #[test]
    fn a_colour_reference_resolves_to_what_the_plugin_published() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("hass.home", "light.kitchen.color"),
            VariableValue::Color(RgbaColor::opaque(232, 185, 35)),
        );
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Fill {
                color: ColorBinding::Reference("$(hass.home:light.kitchen.color)".to_string()),
            }]),
            false,
        );
        assert_eq!(
            only(resolved),
            Some(ResolvedLayer::Fill {
                color: RgbaColor::opaque(232, 185, 35)
            })
        );
    }

    #[test]
    fn a_plugin_publishing_a_hex_string_works_just_as_well() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("hass.home", "light.kitchen.color"),
            VariableValue::Text("#00ff7f".to_string()),
        );
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Fill {
                color: ColorBinding::Reference("$(hass.home:light.kitchen.color)".to_string()),
            }]),
            false,
        );
        assert_eq!(
            only(resolved),
            Some(ResolvedLayer::Fill {
                color: RgbaColor::opaque(0, 255, 127)
            })
        );
    }

    #[test]
    fn an_unresolved_fill_leaves_the_key_unstyled() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Fill {
                color: ColorBinding::Reference("$(hass.home:missing)".to_string()),
            }]),
            false,
        );
        assert_eq!(only(resolved), None);
    }

    /// A label is the content of its layer, so losing the colour must not lose the label.
    #[test]
    fn an_unresolved_text_colour_still_draws_the_text() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![text(
                "Play",
                ColorBinding::Reference("$(hass.home:missing)".to_string()),
            )]),
            false,
        );
        assert_eq!(
            only(resolved),
            Some(ResolvedLayer::Text {
                text: "Play".to_string(),
                color: UNRESOLVED_CONTENT_COLOR,
                anchor: Anchor9::Center,
            })
        );
    }

    #[test]
    fn an_image_reference_resolves_to_the_published_asset() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("mpris.default", "art"),
            VariableValue::Image(AssetId("hash:abc123".to_string())),
        );
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Image {
                image: AssetId("$(mpris.default:art)".to_string()),
                fit: Fit::Cover,
                anchor: Anchor9::Center,
                scale_percent: 100,
                tint: None,
            }]),
            false,
        );
        assert_eq!(
            only(resolved),
            Some(ResolvedLayer::Image {
                image: AssetId("hash:abc123".to_string()),
                fit: Fit::Cover,
                anchor: Anchor9::Center,
                scale_percent: 100,
                tint: None,
            })
        );
    }

    #[test]
    fn an_image_that_resolves_to_nothing_is_not_drawn() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Image {
                image: AssetId("$(discord.home:missing)".to_string()),
                fit: Fit::Contain,
                anchor: Anchor9::BottomEnd,
                scale_percent: 32,
                tint: None,
            }]),
            false,
        );
        assert_eq!(only(resolved), None);
    }

    #[test]
    fn a_border_colour_resolves_from_what_the_plugin_published() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("discord.home", "channel_members_0_status_color"),
            VariableValue::Text("#ed4245".to_string()),
        );
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Border {
                color: ColorBinding::Reference(
                    "$(discord.home:channel_members_0_status_color)".to_string(),
                ),
                width: 6,
            }]),
            false,
        );
        assert_eq!(
            only(resolved),
            Some(ResolvedLayer::Border {
                color: RgbaColor::opaque(237, 66, 69),
                width: 6,
            })
        );
    }

    /// How a plugin says "nothing to report": publish an empty colour and the outline disappears.
    #[test]
    fn an_empty_border_colour_leaves_the_key_unbordered() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("discord.home", "channel_members_0_status_color"),
            VariableValue::Text(String::new()),
        );
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Border {
                color: ColorBinding::Reference(
                    "$(discord.home:channel_members_0_status_color)".to_string(),
                ),
                width: 6,
            }]),
            false,
        );
        assert_eq!(only(resolved), None);
    }

    #[test]
    fn a_zero_width_border_is_not_drawn() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Border {
                color: RgbaColor::opaque(1, 2, 3).into(),
                width: 0,
            }]),
            false,
        );
        assert_eq!(only(resolved), None);
    }

    /// The improvement layers buy for free: a progress bar could only ever be written out, and now
    /// follows whatever a plugin publishes.
    #[test]
    fn a_bar_follows_the_values_it_binds() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("mpris.default", "position"),
            VariableValue::Number(37.4),
        );
        variables.set(
            VariableRef::new("mpris.default", "length"),
            VariableValue::Number(180.0),
        );
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Bar {
                value: ValueBinding::Reference("$(mpris.default:position)".to_string()),
                maximum: ValueBinding::Reference("$(mpris.default:length)".to_string()),
                color: white(),
                edge: Edge::Bottom,
                thickness: 6,
            }]),
            false,
        );
        assert_eq!(
            only(resolved),
            Some(ResolvedLayer::Bar {
                value: 37,
                maximum: 180,
                color: RgbaColor::opaque(255, 255, 255),
                edge: Edge::Bottom,
                thickness: 6,
            })
        );
    }

    #[test]
    fn a_bar_with_no_maximum_yet_is_not_drawn() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![Layer::Bar {
                value: 1.into(),
                maximum: ValueBinding::Reference("$(mpris.default:length)".to_string()),
                color: white(),
                edge: Edge::Bottom,
                thickness: 6,
            }]),
            false,
        );
        assert_eq!(only(resolved), None);
    }

    #[test]
    fn the_stack_keeps_the_order_it_was_written_in() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(vec![
                Layer::Fill {
                    color: RgbaColor::opaque(1, 1, 1).into(),
                },
                text("over", white()),
                Layer::Border {
                    color: RgbaColor::opaque(2, 2, 2).into(),
                    width: 5,
                },
            ]),
            false,
        );
        assert!(matches!(
            resolved.layers.as_slice(),
            [
                ResolvedLayer::Fill { .. },
                ResolvedLayer::Text { .. },
                ResolvedLayer::Border { .. }
            ]
        ));
    }

    #[test]
    fn the_pressed_state_falls_back_to_the_default_state() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);
        let subject = control(vec![text("Play", white())]);

        assert_eq!(
            only(context.resolve(&subject, true)),
            Some(ResolvedLayer::Text {
                text: "Play".to_string(),
                color: RgbaColor::opaque(255, 255, 255),
                anchor: Anchor9::Center,
            })
        );
    }
}
