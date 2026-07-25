use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::{
    identifiers::SurfaceId,
    surfaces::{command::SurfaceCommand, logs::SurfaceLogLevel, registry::SurfaceRegistry},
};

/// How many pending renders a surface can queue before the daemon starts dropping them.
const SURFACE_COMMAND_QUEUE_SIZE: usize = 64;

pub(super) struct ActiveConnection {
    is_active: Arc<AtomicBool>,
    command_sender: mpsc::Sender<SurfaceCommand>,
}

impl SurfaceRegistry {
    pub fn activate(
        &self,
        surface_id: &SurfaceId,
    ) -> (Arc<AtomicBool>, mpsc::Receiver<SurfaceCommand>) {
        self.deactivate(&surface_id.0);
        let is_active = Arc::new(AtomicBool::new(true));
        let (command_sender, command_receiver) = mpsc::channel(SURFACE_COMMAND_QUEUE_SIZE);
        self.active_connections.write().unwrap().insert(
            surface_id.0.clone(),
            ActiveConnection {
                is_active: is_active.clone(),
                command_sender,
            },
        );
        (is_active, command_receiver)
    }

    pub fn deactivate(&self, surface_id: &str) {
        self.reset_dial_positions(surface_id);
        self.rendered.forget(surface_id);
        if let Some(connection) = self.active_connections.write().unwrap().remove(surface_id) {
            connection.is_active.store(false, Ordering::Release);
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
