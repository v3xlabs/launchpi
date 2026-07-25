use crate::{
    identifiers::AssetId,
    panels::{
        control::Control,
        rendered_state::{
            Border, ColorBinding, ContentLayout, OverlayImage, Progress, RenderedState,
            ResolvedBorder, ResolvedOverlay, RgbaColor,
        },
    },
    variables::{template, VariableStore},
};

/// What a control resolved to: every binding replaced by the value it names. This is what the
/// renderer draws, and what the de-duplication ledger hashes.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ResolvedState {
    pub text: Option<String>,
    pub image: Option<AssetId>,
    pub overlay_image: Option<ResolvedOverlay>,
    pub foreground_color: Option<RgbaColor>,
    pub background_color: Option<RgbaColor>,
    pub border: Option<ResolvedBorder>,
    pub progress: Option<Progress>,
    pub content_layout: ContentLayout,
}

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
            text: state
                .text
                .as_deref()
                .map(|text| self.interpolate(text))
                .filter(|text| !text.is_empty()),
            image: state
                .image
                .as_ref()
                .and_then(|asset| self.resolve_asset(asset)),
            overlay_image: self.resolve_overlay(state.overlay_image.as_ref()),
            foreground_color: self.resolve_color(state.foreground_color.as_ref()),
            background_color: self.resolve_color(state.background_color.as_ref()),
            border: self.resolve_border(state.border.as_ref()),
            progress: state.progress.clone(),
            content_layout: state.content_layout,
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

    /// A border whose colour does not resolve is not drawn, for the same reason an unresolved
    /// background leaves the key unstyled rather than black. That is how a plugin spells "nothing
    /// to report": publish an empty colour and the outline disappears.
    fn resolve_border(&self, border: Option<&Border>) -> Option<ResolvedBorder> {
        let border = border?;
        let color = self.resolve_color(Some(&border.color))?;

        (border.width > 0).then_some(ResolvedBorder {
            color,
            width: border.width,
        })
    }

    fn resolve_overlay(&self, overlay: Option<&OverlayImage>) -> Option<ResolvedOverlay> {
        let overlay = overlay?;
        let image = self.resolve_asset(&overlay.image)?;

        (overlay.scale_percent > 0).then_some(ResolvedOverlay {
            image,
            anchor: overlay.anchor,
            scale_percent: overlay.scale_percent,
        })
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
        panels::rendered_state::Anchor9,
        surfaces::layout::SurfacePosition,
        variables::{VariableRef, VariableValue},
    };

    fn control(default_state: RenderedState) -> Control {
        Control {
            control_id: ControlId("key".to_string()),
            name: "Key".to_string(),
            position: SurfacePosition { column: 0, row: 0 },
            default_state,
            pressed_state: None,
            action_bindings: Vec::new(),
        }
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
            &control(RenderedState {
                text: Some("$(mpris.default:title)".to_string()),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.text, Some("Blue Monday".to_string()));
    }

    #[test]
    fn a_literal_colour_passes_through() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);
        let color = RgbaColor::opaque(10, 20, 30);

        let resolved = context.resolve(
            &control(RenderedState {
                background_color: Some(color.clone().into()),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.background_color, Some(color));
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
            &control(RenderedState {
                background_color: Some(ColorBinding::Reference(
                    "$(hass.home:light.kitchen.color)".to_string(),
                )),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(
            resolved.background_color,
            Some(RgbaColor::opaque(232, 185, 35))
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
            &control(RenderedState {
                background_color: Some(ColorBinding::Reference(
                    "$(hass.home:light.kitchen.color)".to_string(),
                )),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(
            resolved.background_color,
            Some(RgbaColor::opaque(0, 255, 127))
        );
    }

    #[test]
    fn an_unresolved_colour_reference_leaves_the_key_unstyled() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(RenderedState {
                background_color: Some(ColorBinding::Reference("$(hass.home:missing)".to_string())),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.background_color, None);
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
            &control(RenderedState {
                image: Some(AssetId("$(mpris.default:art)".to_string())),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.image, Some(AssetId("hash:abc123".to_string())));
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
            &control(RenderedState {
                border: Some(Border {
                    color: ColorBinding::Reference(
                        "$(discord.home:channel_members_0_status_color)".to_string(),
                    ),
                    width: 6,
                }),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(
            resolved.border,
            Some(ResolvedBorder {
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
            &control(RenderedState {
                border: Some(Border {
                    color: ColorBinding::Reference(
                        "$(discord.home:channel_members_0_status_color)".to_string(),
                    ),
                    width: 6,
                }),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.border, None);
    }

    #[test]
    fn a_zero_width_border_is_not_drawn() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(RenderedState {
                border: Some(Border {
                    color: RgbaColor::opaque(1, 2, 3).into(),
                    width: 0,
                }),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.border, None);
    }

    #[test]
    fn an_overlay_image_reference_resolves_to_the_published_asset() {
        let variables = VariableStore::default();
        variables.set(
            VariableRef::new("discord.home", "channel_members_0_status_icon"),
            VariableValue::Image(AssetId("hash:badge".to_string())),
        );
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(RenderedState {
                overlay_image: Some(OverlayImage {
                    image: AssetId("$(discord.home:channel_members_0_status_icon)".to_string()),
                    anchor: Anchor9::BottomEnd,
                    scale_percent: 32,
                }),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(
            resolved.overlay_image.map(|overlay| overlay.image),
            Some(AssetId("hash:badge".to_string()))
        );
    }

    #[test]
    fn an_overlay_that_resolves_to_nothing_draws_no_badge() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);

        let resolved = context.resolve(
            &control(RenderedState {
                overlay_image: Some(OverlayImage {
                    image: AssetId("$(discord.home:missing)".to_string()),
                    anchor: Anchor9::BottomEnd,
                    scale_percent: 32,
                }),
                ..RenderedState::default()
            }),
            false,
        );
        assert_eq!(resolved.overlay_image, None);
    }

    #[test]
    fn the_pressed_state_falls_back_to_the_default_state() {
        let variables = VariableStore::default();
        let context = RenderContext::new(&variables);
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
