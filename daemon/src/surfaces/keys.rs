use serde::Serialize;

use crate::{
    events::ServerEvent,
    identifiers::SurfaceId,
    panels::control::Control,
    plugins::engine::InputEvent,
    surfaces::{logs::SurfaceLogLevel, registry::SurfaceRegistry},
};

#[derive(Clone, Debug, Serialize)]
pub struct SurfaceKeyEvent {
    pub surface_id: SurfaceId,
    pub key_index: u8,
    pub is_pressed: bool,
}

impl SurfaceRegistry {
    /// The control a key belongs to on whatever panel the surface is showing.
    pub fn control_at(&self, surface_id: &SurfaceId, key_index: u8) -> Option<Control> {
        if self.has_open_subpanel(surface_id) {
            return self.top_subpanel_control_at(surface_id, key_index);
        }
        let device = self.managed(surface_id)?;
        let panel = self.panel(&device.active_panel_id?.0)?;
        panel
            .controls
            .into_iter()
            .find(|control| key_index_for(control, panel.layout.columns) == Some(key_index))
    }

    /// Re-resolves one key against current plugin state and pushes it if anything changed.
    pub fn refresh_key(&self, surface_id: &SurfaceId, key_index: u8) {
        let is_pressed = self
            .key_states
            .read()
            .unwrap()
            .get(&(surface_id.0.clone(), key_index))
            .copied()
            .unwrap_or(false);
        if let Some(rendering) = self.rendering_for_key(surface_id, key_index, is_pressed) {
            self.send_rendering(surface_id, rendering);
        }
    }

    pub fn record_key_state(
        &self,
        surface_id: &SurfaceId,
        key_index: u8,
        is_pressed: bool,
    ) -> bool {
        let mut key_states = self.key_states.write().unwrap();
        let previous_state = key_states.insert((surface_id.0.clone(), key_index), is_pressed);
        if previous_state == Some(is_pressed) {
            return false;
        }
        drop(key_states);
        if is_pressed && self.has_open_subpanel(surface_id) && self.top_subpanel_control_at(surface_id, key_index).is_none() {
            self.dismissed_overlay_keys
                .write()
                .unwrap()
                .insert((surface_id.0.clone(), key_index));
            self.close_subpanel(surface_id);
            return true;
        }
        if !is_pressed && self
            .dismissed_overlay_keys
            .write()
            .unwrap()
            .remove(&(surface_id.0.clone(), key_index))
        {
            return true;
        }
        let control = if is_pressed {
            let control = self.control_at(surface_id, key_index);
            if let Some(control) = &control {
                self.pressed_controls
                    .write()
                    .unwrap()
                    .insert((surface_id.0.clone(), key_index), control.clone());
            }
            control
        } else {
            self.pressed_controls
                .write()
                .unwrap()
                .remove(&(surface_id.0.clone(), key_index))
        };
        let mut events = self.recent_key_events.write().unwrap();
        events.push_front(SurfaceKeyEvent {
            surface_id: surface_id.clone(),
            key_index,
            is_pressed,
        });
        events.truncate(50);
        drop(events);
        self.log(
            surface_id,
            SurfaceLogLevel::Input,
            format!(
                "key {key_index} {}",
                if is_pressed { "pressed" } else { "released" }
            ),
        );
        self.emit(ServerEvent::KeyState {
            surface_id: surface_id.clone(),
            key_index,
            is_pressed,
        });
        if let Some(rendering) = self.rendering_for_key(surface_id, key_index, is_pressed) {
            self.send_rendering(surface_id, rendering);
        }
        self.dispatch_input(InputEvent::Key {
            surface_id: surface_id.clone(),
            key_index,
            is_pressed,
            control,
        });
        true
    }
}

pub fn key_index_for(control: &Control, columns: u16) -> Option<u8> {
    u8::try_from(
        u32::from(control.position.row) * u32::from(columns) + u32::from(control.position.column),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::defaults::default_panel;

    #[test]
    fn a_key_press_is_handed_to_the_action_engine() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());
        let mut input = registry
            .take_input_receiver()
            .expect("the receiver is available once");

        assert!(registry.record_key_state(&surface_id, 0, true));

        match input.try_recv().expect("the press should be queued") {
            InputEvent::Key {
                key_index,
                is_pressed,
                ..
            } => {
                assert_eq!(key_index, 0);
                assert!(is_pressed);
            }
        }
    }

    #[test]
    fn broadcasts_a_key_state_event_when_a_key_is_pressed() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let mut receiver = registry.subscribe();
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());

        registry.record_key_state(&surface_id, 0, true);

        let mut key_state = None;
        while let Ok(event) = receiver.try_recv() {
            if let ServerEvent::KeyState {
                key_index,
                is_pressed,
                ..
            } = event
            {
                key_state = Some((key_index, is_pressed));
            }
        }
        assert_eq!(key_state, Some((0, true)));
    }
}
