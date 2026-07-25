use serde::Serialize;

use crate::{
    identifiers::SurfaceId,
    panels::Panel,
    plugins::instance::PluginInstance,
    surfaces::{
        dials::{percent_from_segments, SurfaceDialPress, SurfaceDialState},
        keys::SurfaceKeyEvent,
        logs::SurfaceLogEntry,
        managed::{DiscoveredNetworkSurface, ManagedNetworkSurface},
        registry::SurfaceRegistry,
    },
};

#[derive(Clone, Debug, Serialize)]
pub struct DeviceInventory {
    pub discovered: Vec<DiscoveredNetworkSurface>,
    pub devices: Vec<ManagedNetworkSurface>,
    pub panels: Vec<Panel>,
    /// Filled in by the API layer, which is where the plugin engine is reachable from.
    pub plugin_instances: Vec<PluginInstance>,
    pub recent_key_events: Vec<SurfaceKeyEvent>,
    pub key_states: Vec<SurfaceKeyEvent>,
    pub dial_states: Vec<SurfaceDialState>,
    pub dial_presses: Vec<SurfaceDialPress>,
    pub logs: Vec<SurfaceLogEntry>,
}

impl SurfaceRegistry {
    pub fn inventory(&self) -> DeviceInventory {
        let mut discovered: Vec<_> = self.discovered.read().unwrap().values().cloned().collect();
        let mut devices: Vec<_> = self.managed.read().unwrap().values().cloned().collect();
        let mut panels: Vec<_> = self.panels.read().unwrap().values().cloned().collect();
        let recent_key_events = self
            .recent_key_events
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        let key_states = self
            .key_states
            .read()
            .unwrap()
            .iter()
            .filter(|(_, is_pressed)| **is_pressed)
            .map(|((surface_id, key_index), is_pressed)| SurfaceKeyEvent {
                surface_id: SurfaceId(surface_id.clone()),
                key_index: *key_index,
                is_pressed: *is_pressed,
            })
            .collect();
        let dial_states = self
            .dial_positions
            .read()
            .unwrap()
            .iter()
            .map(
                |((surface_id, dial_index), lit_segments)| SurfaceDialState {
                    surface_id: SurfaceId(surface_id.clone()),
                    dial_index: *dial_index,
                    level: percent_from_segments(*lit_segments),
                },
            )
            .collect();
        let dial_presses = self
            .dial_presses
            .read()
            .unwrap()
            .iter()
            .filter(|(_, is_pressed)| **is_pressed)
            .map(|((surface_id, dial_index), is_pressed)| SurfaceDialPress {
                surface_id: SurfaceId(surface_id.clone()),
                dial_index: *dial_index,
                is_pressed: *is_pressed,
            })
            .collect();
        let mut logs: Vec<_> = self
            .logs
            .read()
            .unwrap()
            .values()
            .flatten()
            .cloned()
            .collect();
        logs.sort_by_key(|entry| entry.sequence);
        discovered.sort_by(|left, right| left.name.cmp(&right.name));
        devices.sort_by(|left, right| left.name.cmp(&right.name));
        panels.sort_by(|left, right| left.name.cmp(&right.name));
        DeviceInventory {
            discovered,
            devices,
            panels,
            plugin_instances: Vec::new(),
            recent_key_events,
            key_states,
            dial_states,
            dial_presses,
            logs,
        }
    }
}
