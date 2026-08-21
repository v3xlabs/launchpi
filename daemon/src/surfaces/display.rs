use std::collections::HashSet;

use crate::{
    events::ServerEvent,
    identifiers::SurfaceId,
    panels::rendered_state::RgbaColor,
    plugins::engine::InputEvent,
    surfaces::{logs::SurfaceLogLevel, managed::ManagedNetworkSurface, registry::SurfaceRegistry},
};

pub const MAX_BRIGHTNESS: u8 = 100;

impl SurfaceRegistry {
    pub fn effective_brightness(&self, surface_id: &SurfaceId) -> Option<u8> {
        self.managed(surface_id)
            .filter(|surface| surface.capabilities.supports_brightness)
            .map(|surface| {
                if surface.is_display_off {
                    0
                } else {
                    surface.brightness
                }
            })
    }

    pub fn is_display_off(&self, surface_id: &SurfaceId) -> bool {
        self.managed(surface_id)
            .is_some_and(|surface| surface.is_display_off)
    }

    pub fn set_brightness_for(
        &self,
        surface_ids: &[SurfaceId],
        brightness: u8,
    ) -> Result<Vec<ManagedNetworkSurface>, String> {
        if brightness > MAX_BRIGHTNESS {
            return Err(format!("brightness must be between 0 and {MAX_BRIGHTNESS}"));
        }
        let targets = self.resolve_display_targets(surface_ids)?;
        Ok(targets
            .into_iter()
            .filter_map(|surface_id| self.set_surface_brightness(&surface_id, brightness))
            .collect())
    }

    pub fn set_display_off_for(
        &self,
        surface_ids: &[SurfaceId],
        is_display_off: bool,
    ) -> Result<Vec<ManagedNetworkSurface>, String> {
        let targets = self.resolve_display_targets(surface_ids)?;
        Ok(targets
            .into_iter()
            .filter_map(|surface_id| self.set_surface_display_off(&surface_id, is_display_off))
            .collect())
    }

    fn resolve_display_targets(&self, surface_ids: &[SurfaceId]) -> Result<Vec<SurfaceId>, String> {
        if surface_ids.is_empty() {
            return Err("at least one surface is required".to_string());
        }
        let managed = self.managed.read().unwrap();
        let dock_children = self.dock_children.read().unwrap();
        let mut seen = HashSet::new();
        let mut targets = Vec::new();
        for surface_id in surface_ids {
            let surface = managed
                .get(&surface_id.0)
                .ok_or_else(|| format!("surface {} was not found", surface_id.0))?;
            if surface.capabilities.supports_brightness {
                if seen.insert(surface_id.0.clone()) {
                    targets.push(surface_id.clone());
                }
                continue;
            }
            if surface.model != "Stream Deck Network Dock" {
                return Err(format!(
                    "surface {} does not support display control",
                    surface_id.0
                ));
            }
            let children = dock_children
                .get(&surface_id.0)
                .into_iter()
                .flatten()
                .filter_map(|child_id| managed.get(child_id))
                .filter(|child| child.capabilities.supports_brightness)
                .collect::<Vec<_>>();
            if children.is_empty() {
                return Err(format!(
                    "network dock {} has no attached display",
                    surface_id.0
                ));
            }
            for child in children {
                if seen.insert(child.surface_id.0.clone()) {
                    targets.push(child.surface_id.clone());
                }
            }
        }
        Ok(targets)
    }

    fn set_surface_brightness(
        &self,
        surface_id: &SurfaceId,
        brightness: u8,
    ) -> Option<ManagedNetworkSurface> {
        let surface = {
            let mut managed = self.managed.write().unwrap();
            let parent_id = managed.get(&surface_id.0)?.parent_surface_id.clone();
            let surface = managed.get_mut(&surface_id.0)?;
            surface.brightness = brightness;
            let surface = surface.clone();
            if let Some(parent_id) = parent_id {
                if let Some(parent) = managed.get_mut(&parent_id.0) {
                    parent.brightness = brightness;
                }
            }
            surface
        };
        self.send_effective_brightness(&surface);
        self.emit_display_state(&surface);
        Some(surface)
    }

    fn set_surface_display_off(
        &self,
        surface_id: &SurfaceId,
        is_display_off: bool,
    ) -> Option<ManagedNetworkSurface> {
        let surface = {
            let mut managed = self.managed.write().unwrap();
            let surface = managed.get_mut(&surface_id.0)?;
            if surface.is_display_off == is_display_off {
                return Some(surface.clone());
            }
            surface.is_display_off = is_display_off;
            surface.clone()
        };
        if is_display_off {
            let pressed_keys = self
                .key_states
                .read()
                .unwrap()
                .iter()
                .filter(|((id, _), is_pressed)| id == &surface_id.0 && **is_pressed)
                .map(|((id, key_index), _)| (id.clone(), *key_index))
                .collect::<Vec<_>>();
            self.display_consumed_keys
                .write()
                .unwrap()
                .extend(pressed_keys.iter().cloned());
            self.pressed_controls
                .write()
                .unwrap()
                .retain(|(id, _), _| id != &surface_id.0);
            self.dial_presses
                .write()
                .unwrap()
                .retain(|(id, _), _| id != &surface_id.0);
            for (_, key_index) in &pressed_keys {
                self.emit(ServerEvent::KeyState {
                    surface_id: surface_id.clone(),
                    key_index: *key_index,
                    is_pressed: false,
                });
                if let Some(rendering) = self.rendering_for_key(surface_id, *key_index, false) {
                    self.send_rendering(surface_id, rendering);
                }
            }
            self.dispatch_input(InputEvent::CancelSurfaceInput {
                surface_id: surface_id.clone(),
            });
        }
        self.send_effective_brightness(&surface);
        if is_display_off {
            for dial in self.surface_dials(surface_id) {
                self.send_dial_color(surface_id, dial.index, RgbaColor::opaque(0, 0, 0), 0);
            }
        } else {
            for (dial_index, color, lit_segments) in self.active_dial_rings(surface_id) {
                self.send_dial_color(surface_id, dial_index, color, lit_segments);
            }
        }
        self.log(
            surface_id,
            SurfaceLogLevel::Info,
            if is_display_off {
                "display off".to_string()
            } else {
                "display awake".to_string()
            },
        );
        self.emit_display_state(&surface);
        Some(surface)
    }

    fn send_effective_brightness(&self, surface: &ManagedNetworkSurface) {
        let brightness = if surface.is_display_off {
            0
        } else {
            surface.brightness
        };
        if let Some(connection) = self
            .active_connections
            .read()
            .unwrap()
            .get(&surface.surface_id.0)
        {
            let _ = connection.brightness_sender.send(brightness);
        }
    }

    fn emit_display_state(&self, surface: &ManagedNetworkSurface) {
        self.emit(ServerEvent::DisplayState {
            surface_id: surface.surface_id.clone(),
            brightness: surface.brightness,
            is_display_off: surface.is_display_off,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        panels::rendered_state::Layer,
        surfaces::{command::SurfaceCommand, defaults::default_panel, layout::SurfaceCapabilities},
    };

    #[test]
    fn display_off_releases_a_held_key_visually_without_running_its_release_action() {
        let mut panel = default_panel();
        panel.controls[0].pressed_state.as_mut().unwrap().layers[0] = Layer::Fill {
            color: RgbaColor::opaque(255, 0, 0).into(),
        };
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![panel]);
        let surface_id = registry.managed_surfaces()[0].surface_id.clone();
        let mut events = registry.subscribe();
        let (_active, mut commands, _brightness) = registry.activate(&surface_id);

        assert!(registry.record_key_state(&surface_id, 0, true));
        while commands.try_recv().is_ok() {}
        registry
            .set_display_off_for(std::slice::from_ref(&surface_id), true)
            .expect("the Stream Deck can turn its display off");

        let mut emitted_release = false;
        while let Ok(event) = events.try_recv() {
            if matches!(
                event,
                ServerEvent::KeyState {
                    key_index: 0,
                    is_pressed: false,
                    ..
                }
            ) {
                emitted_release = true;
            }
        }
        assert!(emitted_release);
        assert!(registry.inventory().key_states.is_empty());
        let mut rendered_release = false;
        while let Ok(command) = commands.try_recv() {
            if matches!(command, SurfaceCommand::RenderKey(rendering) if rendering.key_index == 0) {
                rendered_release = true;
            }
        }
        assert!(rendered_release);
        assert!(registry.record_key_state(&surface_id, 0, false));
        assert!(registry.display_consumed_keys.read().unwrap().is_empty());
    }

    #[test]
    fn display_off_wakes_on_the_first_key_without_emitting_a_control_press() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let surface_id = registry.managed_surfaces()[0].surface_id.clone();
        let (_active, _commands, mut brightness) = registry.activate(&surface_id);

        registry
            .set_brightness_for(std::slice::from_ref(&surface_id), 42)
            .expect("the Stream Deck supports brightness");
        registry
            .set_display_off_for(std::slice::from_ref(&surface_id), true)
            .expect("the Stream Deck can turn its display off");
        assert_eq!(*brightness.borrow_and_update(), 0);

        assert!(registry.record_key_state(&surface_id, 3, true));
        assert!(!registry.is_display_off(&surface_id));
        assert_eq!(*brightness.borrow_and_update(), 42);
        assert!(registry.record_key_state(&surface_id, 3, false));
        assert!(registry.recent_key_events.read().unwrap().is_empty());
    }

    #[test]
    fn a_batch_is_validated_before_any_display_changes() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let stream_deck = registry.managed_surfaces()[0].surface_id.clone();
        let mut unsupported = registry.managed(&stream_deck).unwrap();
        unsupported.surface_id = SurfaceId("launchpad-1".to_string());
        unsupported.model = "Launchpad X".to_string();
        unsupported.capabilities = SurfaceCapabilities::default();
        registry.add_managed(unsupported.clone());

        assert!(registry
            .set_display_off_for(&[stream_deck.clone(), unsupported.surface_id], true)
            .is_err());
        assert!(!registry.is_display_off(&stream_deck));
    }

    #[test]
    fn a_dock_target_controls_its_child_and_keeps_the_preference_on_the_parent() {
        let mut parent = crate::surfaces::defaults::default_device(&[], None);
        parent.surface_id = SurfaceId("dock-1".to_string());
        parent.model = "Stream Deck Network Dock".to_string();
        parent.capabilities = SurfaceCapabilities::default();
        let parent_id = parent.surface_id.clone();
        let registry = SurfaceRegistry::from_configuration(vec![parent], vec![default_panel()]);
        let mut child = crate::surfaces::defaults::default_device(&[], None);
        child.surface_id = SurfaceId("dock-child-1".to_string());
        child.parent_surface_id = Some(parent_id.clone());
        let child_id = child.surface_id.clone();
        registry.add_managed_child(&parent_id, child);

        registry
            .set_brightness_for(std::slice::from_ref(&parent_id), 37)
            .expect("the dock resolves to its child");

        assert_eq!(registry.managed(&child_id).unwrap().brightness, 37);
        assert_eq!(registry.managed(&parent_id).unwrap().brightness, 37);

        registry.set_enabled(&parent_id.0, false).unwrap();
        assert!(registry.managed(&child_id).is_none());
    }
}
