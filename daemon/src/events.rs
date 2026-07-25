use serde::Serialize;

use crate::{
    identifiers::{IntegrationId, SurfaceId},
    surfaces::{logs::SurfaceLogEntry, managed::NetworkSurfaceStatus},
};

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    KeyState {
        surface_id: SurfaceId,
        key_index: u8,
        is_pressed: bool,
    },
    DialState {
        surface_id: SurfaceId,
        dial_index: u8,
        level: u8,
    },
    DialPress {
        surface_id: SurfaceId,
        dial_index: u8,
        is_pressed: bool,
    },
    Log(SurfaceLogEntry),
    /// A plugin published a new value. Carries the rendered text so the UI can show it without
    /// re-implementing how each variable kind formats.
    VariableChanged {
        integration_id: IntegrationId,
        name: String,
        rendered: String,
    },
    /// One device's connection state moved. Deliberately separate from [`ServerEvent::Changed`]:
    /// a surface that cannot be reached flips status every reconnect attempt, and making the web
    /// refetch the entire inventory on that cadence is what made the whole UI churn.
    DeviceStatus {
        surface_id: SurfaceId,
        status: NetworkSurfaceStatus,
        last_error: Option<String>,
    },
    /// An image finished downloading. The browser caches rendered keys by request, so it needs
    /// telling that the same request now produces a different picture.
    AssetsChanged,
    /// The set of things changed: something was added, removed, renamed or reassigned. Not for
    /// status, which has its own event.
    Changed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_state_event_serializes_with_a_string_surface_id() {
        let event = ServerEvent::KeyState {
            surface_id: SurfaceId("stream-deck-studio-1".to_string()),
            key_index: 3,
            is_pressed: true,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"key_state","surface_id":"stream-deck-studio-1","key_index":3,"is_pressed":true}"#
        );
    }
}
