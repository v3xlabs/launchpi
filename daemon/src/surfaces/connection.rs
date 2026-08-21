use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::sync::{mpsc, watch};
use tracing::{debug, warn};

use crate::{
    events::ServerEvent,
    identifiers::SurfaceId,
    plugins::engine::InputEvent,
    surfaces::{command::SurfaceCommand, logs::SurfaceLogLevel, registry::SurfaceRegistry},
};

/// How many pending renders a surface can queue before the daemon starts dropping them.
const SURFACE_COMMAND_QUEUE_SIZE: usize = 64;

pub(super) struct ActiveConnection {
    is_active: Arc<AtomicBool>,
    command_sender: mpsc::Sender<SurfaceCommand>,
    pub(super) brightness_sender: watch::Sender<u8>,
}

impl SurfaceRegistry {
    pub fn activate(
        &self,
        surface_id: &SurfaceId,
    ) -> (
        Arc<AtomicBool>,
        mpsc::Receiver<SurfaceCommand>,
        watch::Receiver<u8>,
    ) {
        self.deactivate(&surface_id.0);
        let is_active = Arc::new(AtomicBool::new(true));
        let (command_sender, command_receiver) = mpsc::channel(SURFACE_COMMAND_QUEUE_SIZE);
        let (brightness_sender, brightness_receiver) =
            watch::channel(self.effective_brightness(surface_id).unwrap_or(100));
        self.active_connections.write().unwrap().insert(
            surface_id.0.clone(),
            ActiveConnection {
                is_active: is_active.clone(),
                command_sender,
                brightness_sender,
            },
        );
        (is_active, command_receiver, brightness_receiver)
    }

    pub fn deactivate(&self, surface_id: &str) {
        self.clear_input_state(&SurfaceId(surface_id.to_string()));
        self.reset_dial_positions(surface_id);
        self.rendered.forget(surface_id);
        if let Some(connection) = self.active_connections.write().unwrap().remove(surface_id) {
            connection.is_active.store(false, Ordering::Release);
        }
    }

    pub fn clear_input_state(&self, surface_id: &SurfaceId) {
        let pressed_keys = {
            let mut key_states = self.key_states.write().unwrap();
            let pressed = key_states
                .iter()
                .filter(|((id, _), is_pressed)| id == &surface_id.0 && **is_pressed)
                .map(|((_, key_index), _)| *key_index)
                .collect::<Vec<_>>();
            key_states.retain(|(id, _), _| id != &surface_id.0);
            pressed
        };
        let pressed_dials = {
            let mut dial_presses = self.dial_presses.write().unwrap();
            let pressed = dial_presses
                .iter()
                .filter(|((id, _), is_pressed)| id == &surface_id.0 && **is_pressed)
                .map(|((_, dial_index), _)| *dial_index)
                .collect::<Vec<_>>();
            dial_presses.retain(|(id, _), _| id != &surface_id.0);
            pressed
        };
        self.pressed_controls
            .write()
            .unwrap()
            .retain(|(id, _), _| id != &surface_id.0);
        self.dismissed_overlay_keys
            .write()
            .unwrap()
            .retain(|(id, _)| id != &surface_id.0);
        self.display_consumed_keys
            .write()
            .unwrap()
            .retain(|(id, _)| id != &surface_id.0);
        for key_index in &pressed_keys {
            self.emit(ServerEvent::KeyState {
                surface_id: surface_id.clone(),
                key_index: *key_index,
                is_pressed: false,
            });
        }
        for dial_index in pressed_dials {
            self.emit(ServerEvent::DialPress {
                surface_id: surface_id.clone(),
                dial_index,
                is_pressed: false,
            });
        }
        if !pressed_keys.is_empty() {
            self.dispatch_input(InputEvent::CancelSurfaceInput {
                surface_id: surface_id.clone(),
            });
        }
    }

    /// Hands a command to the surface's connection task. Both failures used to be swallowed: a full
    /// queue means the device is not keeping up and the surface is now showing something stale,
    /// which is worth a warning rather than silence.
    pub(super) fn dispatch(&self, surface_id: &SurfaceId, command: SurfaceCommand, what: &str) {
        let Some(sender) = self
            .active_connections
            .read()
            .unwrap()
            .get(&surface_id.0)
            .map(|connection| connection.command_sender.clone())
        else {
            debug!(
                surface_id = surface_id.0,
                what, "dropped a command: no active connection for the surface"
            );
            return;
        };
        match sender.try_send(command) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!(
                    surface_id = surface_id.0,
                    what,
                    capacity = SURFACE_COMMAND_QUEUE_SIZE,
                    "surface command queue is full, dropped a command; the device is behind and \
                     its keys or dials will be stale"
                );
                self.log(
                    surface_id,
                    SurfaceLogLevel::Warning,
                    format!("dropped {what}: the device is behind"),
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => debug!(
                surface_id = surface_id.0,
                what, "dropped a command: the connection task has gone"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::defaults::default_panel;

    #[test]
    fn clearing_connection_input_cancels_the_surface_hold_state() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let surface_id = registry.managed_surfaces()[0].surface_id.clone();
        let mut input = registry.take_input_receiver().unwrap();

        registry.record_key_state(&surface_id, 0, true);
        assert!(matches!(input.try_recv(), Ok(InputEvent::Key { .. })));

        registry.clear_input_state(&surface_id);

        assert!(matches!(
            input.try_recv(),
            Ok(InputEvent::CancelSurfaceInput { .. })
        ));
        assert!(registry.key_states.read().unwrap().is_empty());
        assert!(registry.pressed_controls.read().unwrap().is_empty());
    }
}
