use crate::{
    identifiers::SurfaceId,
    panels::{control::Control, rendered_state::RgbaColor},
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
        let context = self.render_context();
        panel
            .controls
            .iter()
            .filter_map(|control| {
                rendering_for_control(control, false, panel.layout.columns, &context)
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
        panel.controls.iter().find_map(|control| {
            (key_index_for(control, panel.layout.columns) == Some(key_index)).then(|| {
                rendering_for_control(
                    control,
                    is_pressed,
                    panel.layout.columns,
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
    context: &RenderContext<'_>,
) -> Option<KeyRendering> {
    let state = context.resolve(control, is_pressed);
    Some(KeyRendering {
        key_index: key_index_for(control, columns)?,
        text: state.text,
        icon: None,
        image: state.image,
        progress: state.progress,
        foreground_color: state.foreground_color,
        background_color: state.background_color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        panels::rendered_state::ColorBinding,
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
        panel.controls[0].default_state.text = Some("$(http.local:value)".to_string());
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
        assert_eq!(rendering.text, Some("21".to_string()));
    }

    #[test]
    fn a_repaint_that_resolves_to_the_same_image_is_not_sent_again() {
        let mut panel = default_panel();
        panel.controls[0].default_state.text = Some("$(http.local:value)".to_string());
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
            next_rendering(&mut commands).and_then(|rendering| rendering.text),
            Some("22".to_string())
        );
    }

    #[test]
    fn a_colour_bound_to_a_value_repaints_when_that_value_changes() {
        let mut panel = default_panel();
        panel.controls[0].pressed_state = None;
        panel.controls[0].default_state.background_color = Some(ColorBinding::Reference(
            "$(hass.home:light.kitchen.color)".to_string(),
        ));
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![panel]);
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());
        let (_is_active, mut commands) = registry.activate(&surface_id);
        let reference = VariableRef::new("hass.home", "light.kitchen.color");

        registry.refresh_key(&surface_id, 0);
        let before = next_rendering(&mut commands).expect("a baseline repaint");
        assert_eq!(
            before.background_color, None,
            "an unresolved colour should leave the key unstyled, not black"
        );

        registry
            .variables()
            .set(reference, VariableValue::Color(amber()));
        registry.refresh_key(&surface_id, 0);

        let after = next_rendering(&mut commands).expect("the new colour should repaint the key");
        assert_eq!(after.background_color, Some(amber()));
    }
}
