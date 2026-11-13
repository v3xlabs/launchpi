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

impl ResolvedState {
    /// The scrim goes over the label and icon, not under them, so they dim with the key.
    fn push_held_layers(&mut self) {
        self.layers.push(ResolvedLayer::Fill { color: HELD_SCRIM });
        self.layers.push(ResolvedLayer::Border {
            color: HELD_EDGE,
            width: HELD_EDGE_WIDTH,
        });
    }
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

/// Over the label and icon, so it must stay translucent for them to be readable.
const HELD_SCRIM: RgbaColor = RgbaColor {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 115,
};

/// A `Border` layer is drawn opaque with its alpha folded toward black, so this grey is what lands
/// on the key rather than a blend with the face. Light, because a dark frame vanishes on black.
const HELD_EDGE: RgbaColor = RgbaColor {
    red: 255,
    green: 255,
    blue: 255,
    alpha: 110,
};

/// Narrower than a status border, so that border's colour still shows inside the frame.
const HELD_EDGE_WIDTH: u8 = 3;

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
    /// looked up. The one overlay is a held key with no `pressed_state`, which gets a darkened,
    /// framed copy of its default look.
    pub fn resolve(&self, control: &Control, is_pressed: bool) -> ResolvedState {
        self.resolve_with_font(control, is_pressed, None)
    }

    pub fn resolve_with_font(
        &self,
        control: &Control,
        is_pressed: bool,
        panel_font_family: Option<&str>,
    ) -> ResolvedState {
        self.resolve_states(
            &control.default_state,
            control.pressed_state.as_ref(),
            is_pressed,
            panel_font_family,
        )
    }

    /// Resolution without a `Control`, so the web can have an unsaved draft drawn by the same code
    /// that draws the device. Sharing this is what stops the preview from drifting: there is one
    /// implementation of what a binding means, not one per consumer.
    ///
    /// Only a missing `pressed_state` gets the derived held look. An empty one is still the author's.
    pub fn resolve_states(
        &self,
        default_state: &RenderedState,
        pressed_state: Option<&RenderedState>,
        is_pressed: bool,
        panel_font_family: Option<&str>,
    ) -> ResolvedState {
        let is_derived_press = is_pressed && pressed_state.is_none();
        let state = if is_pressed {
            pressed_state.unwrap_or(default_state)
        } else {
            default_state
        };

        let mut layers = Vec::with_capacity(state.layers.len() + usize::from(is_derived_press) * 2);
        layers.extend(
            state
                .layers
                .iter()
                .filter_map(|layer| self.resolve_layer(layer, panel_font_family)),
        );
        let mut resolved = ResolvedState { layers };
        if is_derived_press {
            resolved.push_held_layers();
        }
        resolved
    }

    /// A layer that resolves to nothing drawable is dropped rather than drawn as a default, so a
    /// plugin that has not answered yet leaves the key as if the layer were not there.
    fn resolve_layer(
        &self,
        layer: &Layer,
        panel_font_family: Option<&str>,
    ) -> Option<ResolvedLayer> {
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
                font_family,
                font_size,
            } => {
                let text = self.interpolate(text);

                (!text.is_empty()).then(|| ResolvedLayer::Text {
                    text,
                    color: self
                        .resolve_color(Some(color))
                        .unwrap_or(UNRESOLVED_CONTENT_COLOR),
                    anchor: *anchor,
                    font_family: font_family
                        .as_deref()
                        .or(panel_font_family)
                        .unwrap_or("sans-serif")
                        .to_string(),
                    font_size: *font_size,
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
            font_family: None,
            font_size: None,
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
                font_family: "sans-serif".to_string(),
                font_size: None,
            })
        );
    }

    #[test]
    fn a_text_layer_overrides_the_panel_font() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);
        let mut layer = text("Play", white());
        let Layer::Text {
            font_family,
            font_size,
            ..
        } = &mut layer
        else {
            unreachable!();
        };
        *font_family = Some("Layer Font".to_string());
        *font_size = Some(24);

        assert_eq!(
            only(context.resolve_with_font(&control(vec![layer]), false, Some("Panel Font"),)),
            Some(ResolvedLayer::Text {
                text: "Play".to_string(),
                color: RgbaColor::opaque(255, 255, 255),
                anchor: Anchor9::Center,
                font_family: "Layer Font".to_string(),
                font_size: Some(24),
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
                font_family: "sans-serif".to_string(),
                font_size: None,
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
    fn a_held_key_without_a_pressed_state_is_its_default_look_darkened_and_framed() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("mpris.default", "title"),
            VariableValue::Text("Blue Monday".to_string()),
        );
        let context = RenderContext::new(&variables);
        let subject = control(vec![
            Layer::Fill {
                color: RgbaColor::opaque(30, 41, 59).into(),
            },
            Layer::Image {
                image: AssetId("mdi:music".to_string()),
                fit: Fit::Contain,
                anchor: Anchor9::TopCenter,
                scale_percent: 40,
                tint: None,
            },
            text("$(mpris.default:title)", white()),
        ]);

        let released = context.resolve(&subject, false);
        let held = context.resolve(&subject, true);

        assert_eq!(
            held.layers.get(..released.layers.len()),
            Some(released.layers.as_slice()),
            "the icon and the label are drawn exactly as they are when released"
        );
        match &held.layers[released.layers.len()..] {
            [ResolvedLayer::Fill { color: scrim }, ResolvedLayer::Border { width, .. }] => {
                assert_eq!((scrim.red, scrim.green, scrim.blue), (0, 0, 0), "dark");
                assert!(
                    scrim.alpha > 0 && scrim.alpha < 255,
                    "translucent, so the face still shows through"
                );
                assert!(*width > 0, "the frame has to be visible");
            }
            other => panic!("expected a scrim and a frame over the face, got {other:?}"),
        }
    }

    #[test]
    fn a_pressed_state_of_its_own_is_drawn_as_written() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);
        let own_look = RenderedState {
            layers: vec![Layer::Fill {
                color: RgbaColor::opaque(200, 0, 0).into(),
            }],
            is_pressed: true,
        };
        let mut subject = control(vec![text("Play", white())]);

        subject.pressed_state = Some(own_look);
        assert_eq!(
            context.resolve(&subject, true).layers,
            vec![ResolvedLayer::Fill {
                color: RgbaColor::opaque(200, 0, 0)
            }],
            "no scrim or frame is added to a look the author wrote"
        );

        subject.pressed_state = Some(RenderedState::default());
        assert_eq!(
            context.resolve(&subject, true).layers,
            Vec::new(),
            "an empty pressed state is still a pressed state: the key goes blank"
        );
    }

    #[test]
    fn releasing_a_held_key_shows_the_default_look_as_it_resolves_now() {
        let variables = VariableStore::default();
        let title = VariableRef::new("mpris.default", "title");
        variables.set(
            title.clone(),
            VariableValue::Text("Blue Monday".to_string()),
        );
        let context = RenderContext::new(&variables);
        let subject = control(vec![text("$(mpris.default:title)", white())]);
        let label = |resolved: &ResolvedState| match resolved.layers.first() {
            Some(ResolvedLayer::Text { text, .. }) => Some(text.clone()),
            _ => None,
        };

        assert_eq!(
            label(&context.resolve(&subject, true)),
            Some("Blue Monday".to_string())
        );

        variables.set(title, VariableValue::Text("Atmosphere".to_string()));
        let still_held = context.resolve(&subject, true);
        assert_eq!(
            label(&still_held),
            Some("Atmosphere".to_string()),
            "a value that changes while the key is held shows up under the press"
        );
        assert!(
            still_held.layers.len() > 1,
            "and the key is still marked as held"
        );

        let released = context.resolve(&subject, false);
        assert_eq!(label(&released), Some("Atmosphere".to_string()));
        assert_eq!(released.layers.len(), 1, "release drops the press feedback");
    }
}
