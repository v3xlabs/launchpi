use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// How long a client's hold lasts without renewal. A browser renews each hold every second, so a
/// page that disappears without sending its release lets go of the key this long after its last
/// renewal. Shorter risks releasing a key behind a slow network; longer keeps hold actions running
/// after a closed tab.
pub const CLIENT_HOLD_LEASE: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HeldInput {
    Key(u8),
    Dial(u8),
}

#[derive(Debug, Default)]
struct Holders {
    is_hardware: bool,
    /// Client id to the moment its lease runs out.
    clients: HashMap<String, Instant>,
}

impl Holders {
    fn is_held(&self) -> bool {
        self.is_hardware || !self.clients.is_empty()
    }
}

/// Who holds each key and dial down: the hardware, and any clients with a lease. An input counts
/// as down while anyone holds it, so one source letting go never releases another's hold, and a
/// hardware report of every key being up does not release a key a browser holds.
#[derive(Debug, Default)]
pub struct Holds {
    holders: Mutex<HashMap<(String, HeldInput), Holders>>,
}

impl Holds {
    /// Records the hardware's state of an input and returns whether anyone still holds it.
    pub fn set_hardware(&self, surface_id: &str, input: HeldInput, is_pressed: bool) -> bool {
        let mut holders = self
            .holders
            .lock()
            .expect("the holds lock is never poisoned");
        let entry = holders.entry((surface_id.to_string(), input)).or_default();
        entry.is_hardware = is_pressed;
        let is_held = entry.is_held();
        if !is_held {
            holders.remove(&(surface_id.to_string(), input));
        }
        is_held
    }

    /// Takes, renews or gives up a client's hold, and returns whether anyone still holds the input.
    pub fn set_client(
        &self,
        surface_id: &str,
        input: HeldInput,
        client_id: &str,
        is_pressed: bool,
        now: Instant,
    ) -> bool {
        let mut holders = self
            .holders
            .lock()
            .expect("the holds lock is never poisoned");
        let entry = holders.entry((surface_id.to_string(), input)).or_default();
        if is_pressed {
            entry
                .clients
                .insert(client_id.to_string(), now + CLIENT_HOLD_LEASE);
        } else {
            entry.clients.remove(client_id);
        }
        let is_held = entry.is_held();
        if !is_held {
            holders.remove(&(surface_id.to_string(), input));
        }
        is_held
    }

    /// Drops the leases that ran out and returns the inputs that nobody holds any more.
    pub fn expire(&self, now: Instant) -> Vec<(String, HeldInput)> {
        let mut holders = self
            .holders
            .lock()
            .expect("the holds lock is never poisoned");
        let mut released = Vec::new();
        holders.retain(|(surface_id, input), entry| {
            let had_clients = !entry.clients.is_empty();
            entry.clients.retain(|_, expires_at| *expires_at > now);
            if entry.is_held() {
                return true;
            }
            if had_clients {
                released.push((surface_id.clone(), *input));
            }
            false
        });
        released
    }

    pub fn clear_surface(&self, surface_id: &str) {
        self.holders
            .lock()
            .expect("the holds lock is never poisoned")
            .retain(|(id, _), _| id != surface_id);
    }
}
