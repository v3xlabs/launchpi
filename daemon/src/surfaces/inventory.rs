use serde::Serialize;

use crate::{
    drivers::streamdeck::model::{model_by_name, DialPlacement},
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

/// A managed device as the browser sees it: everything that is stored about it, plus what its
/// model says the hardware is. The extras are not fields of `ManagedNetworkSurface` because that
/// struct is the one that round-trips through `devices.toml`, and hardware facts belong to the
/// model table, not to the user's configuration.
#[derive(Clone, Debug, Serialize)]
pub struct DeviceView {
    #[serde(flatten)]
    pub device: ManagedNetworkSurface,
    pub dials: &'static [DialPlacement],
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceInventory {
    pub discovered: Vec<DiscoveredNetworkSurface>,
    pub devices: Vec<DeviceView>,
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
        let mut devices: Vec<_> = self
            .managed
            .read()
            .unwrap()
            .values()
            .cloned()
            .map(|device| DeviceView {
                dials: model_by_name(&device.model).map_or(&[][..], |model| model.dials),
                device,
            })
            .collect();
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
        devices.sort_by(|left, right| left.device.name.cmp(&right.device.name));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::defaults::default_panel;

    #[test]
    fn a_device_reports_its_models_dials_without_them_reaching_the_stored_device() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let inventory = registry.inventory();
        let device = inventory.devices.first().expect("the default device");

        let payload = serde_json::to_value(device).expect("a device view serialises");
        assert_eq!(payload["surface_id"], "stream-deck-studio-1");
        assert_eq!(payload["model"], "Stream Deck Studio");
        assert_eq!(payload["dials"][0]["column"], -1);
        assert_eq!(payload["dials"][1]["column"], 16);
        assert_eq!(payload["dials"][1]["row_span"], 2);

        let stored = crate::config::devices::render(registry.managed_surfaces())
            .expect("the stored document renders");
        assert!(!stored.contains("dials"));
        assert!(stored.contains("model = \"Stream Deck Studio\""));
    }

    #[test]
    fn a_device_whose_model_has_no_dials_reports_none() {
        let mut device = crate::surfaces::defaults::default_device(&[], None);
        device.model = "Stream Deck XL".to_string();
        let registry = SurfaceRegistry::from_configuration(vec![device], vec![default_panel()]);

        assert!(registry.inventory().devices[0].dials.is_empty());
    }
}
