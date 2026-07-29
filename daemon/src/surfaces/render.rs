use crate::{
    identifiers::SurfaceId,
    panels::{
        control::Control,
        rendered_state::{ResolvedLayer, RgbaColor},
    },
    rendering::context::RenderContext,
    surfaces::{
        command::{KeyRendering, SurfaceCommand},
        keys::key_index_for,
        registry::SurfaceRegistry,
    },
};

impl SurfaceRegistry {
    pub(super) fn render_context(&self) -> RenderContext<'_> {
        RenderContext::new(&self.variables)
    }

    pub fn active_key_renderings(&self, surface_id: &SurfaceId) -> Vec<KeyRendering> {
        let Some(device) = self.managed(surface_id) else {
            return Vec::new();
        };
        let Some(panel_id) = device.active_panel_id else {
            return Vec::new();
        };
        let Some(panel) = self.panel(&panel_id.0) else {
            return Vec::new();
        };
        if self.has_open_subpanel(surface_id) {
            let crate::surfaces::layout::SurfaceLayout::Grid { columns, rows } = device.layout
            else {
                return Vec::new();
            };
            return (0..u32::from(columns) * u32::from(rows))
                .filter_map(|index| {
                    let key_index = u8::try_from(index).ok()?;
                    self.rendering_for_key(surface_id, key_index, false)
                        .or(Some(KeyRendering {
                            key_index,
                            layers: vec![ResolvedLayer::Fill {
                                color: RgbaColor::opaque(0, 0, 0),
                            }],
                            is_dimmed: true,
                        }))
                })
                .collect();
        }
        let context = self.render_context();
        panel
            .controls
            .iter()
            .filter_map(|control| {
                rendering_for_control(
                    control,
                    false,
                    panel.layout.columns,
                    false,
                    panel.font_family.as_deref(),
                    &context,
                )
            })
            .collect()
    }

    pub(super) fn rendering_for_key(
        &self,
        surface_id: &SurfaceId,
        key_index: u8,
        is_pressed: bool,
    ) -> Option<KeyRendering> {
        let device = self.managed(surface_id)?;
        let panel_id = device.active_panel_id?;
        let panel = self.panel(&panel_id.0)?;
        if self.has_open_subpanel(surface_id) {
            let subpanel_font_family = device
                .open_subpanels
                .last()
                .and_then(|layer| self.panel(&layer.panel_id.0))
                .and_then(|panel| panel.font_family);
            if let Some(control) = self.top_subpanel_control_at(surface_id, key_index) {
                let columns = match device.layout {
                    crate::surfaces::layout::SurfaceLayout::Grid { columns, .. } => columns,
                    crate::surfaces::layout::SurfaceLayout::Freeform => return None,
                };
                return rendering_for_control(
                    &control,
                    is_pressed,
                    columns,
                    false,
                    subpanel_font_family.as_deref(),
                    &self.render_context(),
                );
            }
            return panel.controls.iter().find_map(|control| {
                (key_index_for(control, panel.layout.columns) == Some(key_index)).then(|| {
                    rendering_for_control(
                        control,
                        is_pressed,
                        panel.layout.columns,
                        true,
                        panel.font_family.as_deref(),
                        &self.render_context(),
                    )
                })?
            });
        }
        panel.controls.iter().find_map(|control| {
            (key_index_for(control, panel.layout.columns) == Some(key_index)).then(|| {
                rendering_for_control(
                    control,
                    is_pressed,
                    panel.layout.columns,
                    false,
                    panel.font_family.as_deref(),
                    &self.render_context(),
                )
            })?
        })
    }

    pub(super) fn render_active_panel(&self, surface_id: &str) {
        let surface_id = SurfaceId(surface_id.to_string());
        for rendering in self.active_key_renderings(&surface_id) {
            self.send_rendering(&surface_id, rendering);
        }
        for (dial_index, color, lit_segments) in self.active_dial_rings(&surface_id) {
            self.send_dial_color(&surface_id, dial_index, color, lit_segments);
        }
    }

    /// Drops every key's remembered rendering, so the next repaint is actually sent. Used when an
    /// image an id points at arrives after the key was already drawn without it: the
    /// `KeyRendering` is byte-identical, so the repaint would otherwise be dropped as a duplicate.
    pub fn forget_renderings(&self) {
        self.rendered.forget_all();
    }

    /// Drops a repaint that would produce the identical image.
    pub(super) fn send_rendering(&self, surface_id: &SurfaceId, rendering: KeyRendering) {
        let key_index = rendering.key_index;
        if !self.rendered.record(&surface_id.0, &rendering) {
            return;
        }
        self.dispatch(
            surface_id,
            SurfaceCommand::RenderKey(rendering),
            &format!("key {key_index} rendering"),
        );
    }

    pub(super) fn send_dial_color(
        &self,
        surface_id: &SurfaceId,
        dial_index: u8,
        color: RgbaColor,
        lit_segments: u8,
    ) {
        self.dispatch(
            surface_id,
            SurfaceCommand::RenderDialColor {
                dial_index,
                color,
                lit_segments,
            },
            &format!("dial {dial_index} ring"),
        );
    }
}

fn rendering_for_control(
    control: &Control,
    is_pressed: bool,
    columns: u16,
    is_dimmed: bool,
    panel_font_family: Option<&str>,
    context: &RenderContext<'_>,
) -> Option<KeyRendering> {
    let state = context.resolve_with_font(control, is_pressed, panel_font_family);
    Some(KeyRendering {
        key_index: key_index_for(control, columns)?,
        layers: state.layers,
        is_dimmed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        panels::rendered_state::{ColorBinding, Layer, RenderedState},
        surfaces::defaults::default_panel,
        variables::{VariableRef, VariableValue},
    };

    fn amber() -> RgbaColor {
        RgbaColor {
            red: 232,
            green: 185,
            blue: 35,
            alpha: 255,
        }
    }

    fn text_of(rendering: &KeyRendering) -> Option<String> {
        rendering.layers.iter().find_map(|layer| match layer {
            ResolvedLayer::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
    }

    fn fill_of(rendering: &KeyRendering) -> Option<RgbaColor> {
        rendering.palette_color()
    }

    fn next_rendering(
        commands: &mut tokio::sync::mpsc::Receiver<SurfaceCommand>,
    ) -> Option<KeyRendering> {
        match commands.try_recv().ok()? {
            SurfaceCommand::RenderKey(rendering) => Some(rendering),
            SurfaceCommand::RenderDialColor { .. } => None,
        }
    }

    #[test]
    fn a_live_variable_reaches_the_device_as_interpolated_text() {
        let mut panel = default_panel();
        panel.controls[0].default_state = RenderedState::labelled(
            "$(http.local:value)",
            RgbaColor::opaque(255, 255, 255),
            RgbaColor::opaque(30, 41, 59),
            false,
        );
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![panel]);
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());
        let (_is_active, mut commands) = registry.activate(&surface_id);

        registry.variables().set(
            VariableRef::new("http.local", "value"),
            VariableValue::Number(21.0),
        );
        registry.refresh_key(&surface_id, 0);

        let rendering = next_rendering(&mut commands).expect("a repaint should have been queued");
        assert_eq!(rendering.key_index, 0);
        assert_eq!(text_of(&rendering), Some("21".to_string()));
    }

    #[test]
    fn a_repaint_that_resolves_to_the_same_image_is_not_sent_again() {
        let mut panel = default_panel();
        panel.controls[0].default_state = RenderedState::labelled(
            "$(http.local:value)",
            RgbaColor::opaque(255, 255, 255),
            RgbaColor::opaque(30, 41, 59),
            false,
        );
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![panel]);
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());
        let (_is_active, mut commands) = registry.activate(&surface_id);
        let reference = VariableRef::new("http.local", "value");

        registry
            .variables()
            .set(reference.clone(), VariableValue::Number(21.0));
        registry.refresh_key(&surface_id, 0);
        assert!(next_rendering(&mut commands).is_some());

        registry.refresh_key(&surface_id, 0);
        assert!(
            commands.try_recv().is_err(),
            "an unchanged resolution should never reach the device"
        );

        registry
            .variables()
            .set(reference, VariableValue::Number(22.0));
        registry.refresh_key(&surface_id, 0);
        assert_eq!(
            next_rendering(&mut commands).and_then(|rendering| text_of(&rendering)),
            Some("22".to_string())
        );
    }

    #[test]
    fn a_colour_bound_to_a_value_repaints_when_that_value_changes() {
        let mut panel = default_panel();
        panel.controls[0].pressed_state = None;
        panel.controls[0].default_state.layers = vec![Layer::Fill {
            color: ColorBinding::Reference("$(hass.home:light.kitchen.color)".to_string()),
        }];
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![panel]);
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());
        let (_is_active, mut commands) = registry.activate(&surface_id);
        let reference = VariableRef::new("hass.home", "light.kitchen.color");

        registry.refresh_key(&surface_id, 0);
        let before = next_rendering(&mut commands).expect("a baseline repaint");
        assert_eq!(
            fill_of(&before),
            None,
            "an unresolved colour should leave the key unstyled, not black"
        );

        registry
            .variables()
            .set(reference, VariableValue::Color(amber()));
        registry.refresh_key(&surface_id, 0);

        let after = next_rendering(&mut commands).expect("the new colour should repaint the key");
        assert_eq!(fill_of(&after), Some(amber()));
    }
}
