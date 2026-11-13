use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Deserialize;

use crate::{
    identifiers::SurfaceId,
    surfaces::{
        holds::HeldInput,
        managed::{status_label, NetworkSurfaceStatus},
        registry::SurfaceRegistry,
    },
};

/// How often ran-out client holds are released. Together with `CLIENT_HOLD_LEASE` this bounds how
/// long a key stays down after the page holding it disappeared.
const HOLD_EXPIRY_INTERVAL: Duration = Duration::from_millis(500);
/// Client ids are opaque tokens a page makes up; this only stops a client from sending a novel.
const CLIENT_ID_MAX_LENGTH: usize = 64;

/// One thing a person did to a device, reported by a client instead of read off the hardware.
///
/// The JSON form is tagged on `kind`, for example
/// `{"kind":"key","key_index":3,"is_pressed":true,"client_id":"tab-1"}`. A press is a hold the
/// client owns: it lasts until the same client releases it or stops renewing it, and other
/// clients and the hardware hold the same key independently.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceInput {
    Key {
        key_index: u8,
        is_pressed: bool,
        client_id: String,
    },
    /// Signed detents, like the hardware report: positive is clockwise.
    DialTurn { dial_index: u8, detents: i8 },
    DialPress {
        dial_index: u8,
        is_pressed: bool,
        client_id: String,
    },
}

/// Why the registry refused an input before it reached the hardware path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SurfaceInputError {
    UnknownSurface,
    Disabled,
    NotConnected(NetworkSurfaceStatus),
    UnknownKey(u8),
    UnknownDial(u8),
    InvalidClientId,
}

impl fmt::Display for SurfaceInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSurface => write!(formatter, "managed device was not found"),
            Self::Disabled => write!(formatter, "device is disabled"),
            Self::NotConnected(status) => {
                write!(
                    formatter,
                    "device is not connected, its status is {}",
                    status_label(status)
                )
            }
            Self::UnknownKey(key_index) => {
                write!(formatter, "key {key_index} does not exist on this device")
            }
            Self::UnknownDial(dial_index) => {
                write!(formatter, "dial {dial_index} does not exist on this device")
            }
            Self::InvalidClientId => write!(
                formatter,
                "client_id must be 1 to {CLIENT_ID_MAX_LENGTH} characters"
            ),
        }
    }
}

impl std::error::Error for SurfaceInputError {}

fn check_client_id(client_id: &str) -> Result<(), SurfaceInputError> {
    if client_id.is_empty() || client_id.len() > CLIENT_ID_MAX_LENGTH {
        return Err(SurfaceInputError::InvalidClientId);
    }
    Ok(())
}

impl SurfaceRegistry {
    /// Feeds a client-reported input into the same key and dial state the hardware driver
    /// updates, so a click in the browser logs, broadcasts, repaints and reaches the action engine
    /// exactly like a press on the device. It only adds the checks a hardware report never needs:
    /// the device must exist, be enabled and be connected, and the key or dial must exist on its
    /// model. A press is held for this client, next to the hardware's and other clients' holds.
    pub fn record_input(
        &self,
        surface_id: &SurfaceId,
        input: SurfaceInput,
    ) -> Result<(), SurfaceInputError> {
        let device = self
            .managed(surface_id)
            .ok_or(SurfaceInputError::UnknownSurface)?;
        if !device.is_enabled {
            return Err(SurfaceInputError::Disabled);
        }
        if device.status != NetworkSurfaceStatus::Connected {
            return Err(SurfaceInputError::NotConnected(device.status));
        }
        match input {
            SurfaceInput::Key {
                key_index,
                is_pressed,
                client_id,
            } => {
                if u32::from(key_index) >= device.layout.key_count() {
                    return Err(SurfaceInputError::UnknownKey(key_index));
                }
                check_client_id(&client_id)?;
                let is_held = self.holds.set_client(
                    &surface_id.0,
                    HeldInput::Key(key_index),
                    &client_id,
                    is_pressed,
                    Instant::now(),
                );
                self.apply_held_key(surface_id, key_index, is_held);
            }
            SurfaceInput::DialTurn {
                dial_index,
                detents,
            } => {
                if !self.has_dial(surface_id, dial_index) {
                    return Err(SurfaceInputError::UnknownDial(dial_index));
                }
                self.record_dial_turn(surface_id, dial_index, detents);
            }
            SurfaceInput::DialPress {
                dial_index,
                is_pressed,
                client_id,
            } => {
                if !self.has_dial(surface_id, dial_index) {
                    return Err(SurfaceInputError::UnknownDial(dial_index));
                }
                check_client_id(&client_id)?;
                let is_held = self.holds.set_client(
                    &surface_id.0,
                    HeldInput::Dial(dial_index),
                    &client_id,
                    is_pressed,
                    Instant::now(),
                );
                self.apply_held_dial(surface_id, dial_index, is_held);
            }
        }
        Ok(())
    }

    /// Releases every key and dial whose last client lease ran out, as if the client had let go.
    pub fn expire_client_holds(&self, now: Instant) {
        for (surface_id, input) in self.holds.expire(now) {
            let surface_id = SurfaceId(surface_id);
            match input {
                HeldInput::Key(key_index) => {
                    self.apply_held_key(&surface_id, key_index, false);
                }
                HeldInput::Dial(dial_index) => {
                    self.apply_held_dial(&surface_id, dial_index, false);
                }
            }
        }
    }
}

/// Runs for the daemon's lifetime, so a client that disappears mid-press cannot hold a key forever.
pub fn start_hold_expiry(surfaces: Arc<SurfaceRegistry>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(HOLD_EXPIRY_INTERVAL);
        loop {
            interval.tick().await;
            surfaces.expire_client_holds(Instant::now());
        }
    });
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast;

    use super::*;
    use crate::{
        events::ServerEvent,
        plugins::engine::InputEvent,
        surfaces::holds::CLIENT_HOLD_LEASE,
        surfaces::{defaults::default_panel, logs::SurfaceLogLevel},
    };

    fn studio_registry() -> (SurfaceRegistry, SurfaceId) {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        (registry, SurfaceId("stream-deck-studio-1".to_string()))
    }

    fn connected_studio_registry() -> (SurfaceRegistry, SurfaceId) {
        let (registry, surface_id) = studio_registry();
        registry.update_status(&surface_id, NetworkSurfaceStatus::Connected, None);
        (registry, surface_id)
    }

    fn drain(receiver: &mut broadcast::Receiver<ServerEvent>) -> Vec<ServerEvent> {
        std::iter::from_fn(|| receiver.try_recv().ok()).collect()
    }

    fn logged(events: &[ServerEvent], expected: &str) -> bool {
        events.iter().any(|event| {
            matches!(
                event,
                ServerEvent::Log(entry)
                    if matches!(entry.level, SurfaceLogLevel::Input) && entry.message == expected
            )
        })
    }

    fn key(key_index: u8, is_pressed: bool) -> SurfaceInput {
        client_key("tab-a", key_index, is_pressed)
    }

    fn client_key(client_id: &str, key_index: u8, is_pressed: bool) -> SurfaceInput {
        SurfaceInput::Key {
            key_index,
            is_pressed,
            client_id: client_id.to_string(),
        }
    }

    fn is_key_down(registry: &SurfaceRegistry, surface_id: &SurfaceId, key_index: u8) -> bool {
        registry
            .inventory()
            .key_states
            .iter()
            .any(|state| state.surface_id == *surface_id && state.key_index == key_index)
    }

    #[test]
    fn one_holder_letting_go_does_not_release_another() {
        let (registry, surface_id) = connected_studio_registry();
        registry
            .record_input(&surface_id, client_key("tab-a", 3, true))
            .unwrap();
        registry
            .record_input(&surface_id, client_key("tab-b", 3, true))
            .unwrap();
        // The Studio reports every key on each report, so an unrelated press reports key 3 as up.
        registry.record_key_state(&surface_id, 3, false);
        assert!(
            is_key_down(&registry, &surface_id, 3),
            "the browsers still hold key 3"
        );
        registry
            .record_input(&surface_id, client_key("tab-a", 3, false))
            .unwrap();
        assert!(
            is_key_down(&registry, &surface_id, 3),
            "tab-b still holds key 3"
        );
        registry
            .record_input(&surface_id, client_key("tab-b", 3, false))
            .unwrap();
        assert!(
            !is_key_down(&registry, &surface_id, 3),
            "nobody holds key 3"
        );

        registry.record_key_state(&surface_id, 4, true);
        registry
            .record_input(&surface_id, client_key("tab-a", 4, false))
            .unwrap();
        assert!(
            is_key_down(&registry, &surface_id, 4),
            "a client release leaves the hardware hold"
        );
    }

    #[test]
    fn a_client_hold_that_is_not_renewed_runs_out() {
        let (registry, surface_id) = connected_studio_registry();
        let pressed_at = Instant::now();
        registry.record_input(&surface_id, key(5, true)).unwrap();
        registry
            .record_input(
                &surface_id,
                SurfaceInput::DialPress {
                    dial_index: 0,
                    is_pressed: true,
                    client_id: "tab-a".to_string(),
                },
            )
            .unwrap();
        registry.expire_client_holds(pressed_at + Duration::from_secs(1));
        assert!(
            is_key_down(&registry, &surface_id, 5),
            "the lease has not run out yet"
        );
        registry.expire_client_holds(pressed_at + CLIENT_HOLD_LEASE + Duration::from_millis(100));
        assert!(
            !is_key_down(&registry, &surface_id, 5),
            "the lease ran out, so the key is up"
        );
        assert!(
            registry.inventory().dial_presses.is_empty(),
            "the dial push ran out too"
        );
    }

    #[test]
    fn a_client_id_is_required() {
        let (registry, surface_id) = connected_studio_registry();
        assert_eq!(
            registry.record_input(&surface_id, client_key("", 0, true)),
            Err(SurfaceInputError::InvalidClientId)
        );
    }

    #[test]
    fn a_key_press_from_a_client_takes_the_path_hardware_input_takes() {
        let (registry, surface_id) = connected_studio_registry();
        let mut receiver = registry.subscribe();
        let mut actions = registry
            .take_input_receiver()
            .expect("the receiver is available once");

        registry
            .record_input(&surface_id, key(3, true))
            .expect("a connected device takes input");

        let pressed: Vec<_> = registry
            .inventory()
            .key_states
            .iter()
            .map(|state| (state.key_index, state.is_pressed))
            .collect();
        assert_eq!(pressed, [(3, true)]);
        let events = drain(&mut receiver);
        assert!(events.iter().any(|event| matches!(
            event,
            ServerEvent::KeyState {
                key_index: 3,
                is_pressed: true,
                ..
            }
        )));
        assert!(logged(&events, "key 3 pressed"));
        assert!(matches!(
            actions.try_recv(),
            Ok(InputEvent::Key {
                is_pressed: true,
                ..
            })
        ));

        registry
            .record_input(&surface_id, key(3, false))
            .expect("a connected device takes input");

        assert!(registry.inventory().key_states.is_empty());
        assert!(logged(&drain(&mut receiver), "key 3 released"));
    }

    #[test]
    fn dial_input_from_a_client_moves_the_ring_and_reports_the_press() {
        let (registry, surface_id) = connected_studio_registry();
        let mut receiver = registry.subscribe();

        registry
            .record_input(
                &surface_id,
                SurfaceInput::DialTurn {
                    dial_index: 0,
                    detents: -4,
                },
            )
            .expect("a connected device takes input");
        registry
            .record_input(
                &surface_id,
                SurfaceInput::DialPress {
                    dial_index: 0,
                    is_pressed: true,
                    client_id: "tab-a".to_string(),
                },
            )
            .expect("a connected device takes input");

        let inventory = registry.inventory();
        assert_eq!(inventory.dial_states.len(), 1);
        assert_eq!(inventory.dial_states[0].level, 84);
        assert_eq!(inventory.dial_presses.len(), 1);
        let events = drain(&mut receiver);
        assert!(logged(&events, "dial 0 turned -4 to 84% (20/24)"));
        assert!(logged(&events, "dial 0 pressed"));
    }

    #[test]
    fn a_disabled_device_refuses_input() {
        let (registry, surface_id) = connected_studio_registry();
        registry.set_enabled(&surface_id.0, false);
        let mut actions = registry
            .take_input_receiver()
            .expect("the receiver is available once");

        let refused = registry.record_input(&surface_id, key(0, true));

        assert_eq!(refused, Err(SurfaceInputError::Disabled));
        assert!(registry.inventory().key_states.is_empty());
        assert!(actions.try_recv().is_err());
    }

    #[test]
    fn a_device_that_is_not_connected_refuses_input() {
        let (registry, surface_id) = studio_registry();
        let mut actions = registry
            .take_input_receiver()
            .expect("the receiver is available once");

        assert_eq!(
            registry.record_input(&surface_id, key(0, true)),
            Err(SurfaceInputError::NotConnected(
                NetworkSurfaceStatus::Connecting
            ))
        );

        registry.update_status(
            &surface_id,
            NetworkSurfaceStatus::Unavailable,
            Some("connection refused".to_string()),
        );
        assert_eq!(
            registry.record_input(&surface_id, key(0, true)),
            Err(SurfaceInputError::NotConnected(
                NetworkSurfaceStatus::Unavailable
            ))
        );
        assert!(registry.inventory().key_states.is_empty());
        assert!(actions.try_recv().is_err());
    }

    #[test]
    fn a_key_or_dial_the_model_does_not_have_is_refused() {
        let (registry, surface_id) = connected_studio_registry();
        let missing = SurfaceId("nothing-here".to_string());

        assert_eq!(
            registry.record_input(&missing, key(0, true)),
            Err(SurfaceInputError::UnknownSurface)
        );
        // The Studio has 16 columns by 2 rows, so 31 is its last key.
        assert_eq!(
            registry.record_input(&surface_id, key(32, true)),
            Err(SurfaceInputError::UnknownKey(32))
        );
        assert_eq!(registry.record_input(&surface_id, key(31, true)), Ok(()));
        assert_eq!(
            registry.record_input(
                &surface_id,
                SurfaceInput::DialTurn {
                    dial_index: 2,
                    detents: 1
                }
            ),
            Err(SurfaceInputError::UnknownDial(2))
        );
        assert_eq!(
            registry.record_input(
                &surface_id,
                SurfaceInput::DialPress {
                    dial_index: 2,
                    is_pressed: true,
                    client_id: "tab-a".to_string(),
                }
            ),
            Err(SurfaceInputError::UnknownDial(2))
        );
    }
}
