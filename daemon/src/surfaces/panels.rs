use std::{collections::HashMap, sync::atomic::Ordering};

use crate::{
    events::ServerEvent,
    identifiers::{ControlId, PanelId},
    panels::{
        control::Control,
        rendered_state::RenderedState,
        {Panel, PanelLayout},
    },
    surfaces::{
        defaults::{color, studio_capabilities, white},
        layout::{SurfaceCapabilities, SurfaceLayout, SurfacePosition},
        logs::SurfaceLogLevel,
        managed::ManagedNetworkSurface,
        registry::SurfaceRegistry,
    },
};

impl SurfaceRegistry {
    pub fn panels(&self) -> Vec<Panel> {
        self.panels.read().unwrap().values().cloned().collect()
    }

    pub fn panel(&self, panel_id: &str) -> Option<Panel> {
        self.panels.read().unwrap().get(panel_id).cloned()
    }

    pub fn create_panel_id(&self) -> PanelId {
        PanelId(format!(
            "studio-panel-{}",
            self.next_panel_number.fetch_add(1, Ordering::Relaxed) + 1
        ))
    }

    pub fn ensure_default_panel_for_layout(&self, layout: &SurfaceLayout) -> Option<PanelId> {
        let SurfaceLayout::Grid { columns, rows } = layout else {
            return None;
        };
        let (columns, rows) = (*columns, *rows);
        if let Some(existing) = self
            .panels()
            .into_iter()
            .find(|panel| panel.layout.columns == columns && panel.layout.rows == rows)
        {
            return Some(existing.panel_id);
        }
        let controls = (0..rows)
            .flat_map(|row| {
                (0..columns).map(move |column| {
                    let index = row * columns + column;
                    Control {
                        control_id: ControlId(format!("auto-{row}-{column}")),
                        name: format!("Key {index}"),
                        position: SurfacePosition { column, row },
                        default_state: RenderedState::labelled(
                            format!("{index}"),
                            white(),
                            color(30, 41, 59),
                            false,
                        ),
                        pressed_state: None,
                        action_bindings: Vec::new(),
                    }
                })
            })
            .collect();
        let panel = Panel {
            panel_id: self.create_panel_id(),
            name: format!("Auto {columns}x{rows}"),
            layout: PanelLayout { columns, rows },
            font_family: None,
            capabilities: studio_capabilities(),
            controls,
            dials: Vec::new(),
        };
        self.upsert_panel(panel).ok().map(|panel| panel.panel_id)
    }

    pub fn upsert_panel(&self, panel: Panel) -> Result<Panel, String> {
        validate_panel(&panel)?;
        if self.managed.read().unwrap().values().any(|device| {
            device.active_panel_id.as_ref() == Some(&panel.panel_id)
                && !is_compatible(device, &panel)
        }) {
            return Err("panel is incompatible with a device it is assigned to".to_string());
        }
        self.panels
            .write()
            .unwrap()
            .insert(panel.panel_id.0.clone(), panel.clone());
        let assigned_surface_ids: Vec<_> = self
            .managed
            .read()
            .unwrap()
            .values()
            .filter(|device| device.active_panel_id.as_ref() == Some(&panel.panel_id))
            .map(|device| device.surface_id.0.clone())
            .collect();

        for surface_id in assigned_surface_ids {
            self.render_active_panel(&surface_id);
        }
        self.emit(ServerEvent::Changed);
        Ok(panel)
    }

    pub fn remove_panel(&self, panel_id: &str) -> Option<Panel> {
        let removed = self.panels.write().unwrap().remove(panel_id)?;
        let remaining = self.panels();
        let reassigned: Vec<_> = {
            let mut devices = self.managed.write().unwrap();
            devices
                .values_mut()
                .filter_map(|device| {
                    let closed_overlay = device
                        .open_subpanels
                        .last()
                        .is_some_and(|layer| layer.panel_id == removed.panel_id);
                    device
                        .open_subpanels
                        .retain(|layer| layer.panel_id != removed.panel_id);
                    if device.active_panel_id.as_ref() != Some(&removed.panel_id) {
                        return closed_overlay.then(|| device.surface_id.0.clone());
                    }
                    device.active_panel_id = remaining
                        .iter()
                        .find(|panel| is_compatible(device, panel))
                        .map(|panel| panel.panel_id.clone());
                    device.open_subpanels.clear();
                    Some(device.surface_id.0.clone())
                })
                .collect()
        };
        for surface_id in reassigned {
            self.reset_dial_positions(&surface_id);
            self.render_active_panel(&surface_id);
        }
        self.emit(ServerEvent::Changed);
        Some(removed)
    }

    pub fn assign_active_panel(
        &self,
        surface_id: &str,
        panel_id: &str,
    ) -> Result<ManagedNetworkSurface, String> {
        let panel = self
            .panel(panel_id)
            .ok_or_else(|| "panel was not found".to_string())?;
        let device = {
            let mut devices = self.managed.write().unwrap();
            let device = devices
                .get_mut(surface_id)
                .ok_or_else(|| "managed device was not found".to_string())?;
            if !is_compatible(device, &panel) {
                return Err("panel layout or capabilities are incompatible with device".to_string());
            }
            device.active_panel_id = Some(panel.panel_id);
            device.open_subpanels.clear();
            device.clone()
        };
        self.reset_dial_positions(surface_id);
        self.render_active_panel(surface_id);
        self.log(
            &device.surface_id,
            SurfaceLogLevel::Info,
            format!("active panel set to {}", panel.name),
        );
        self.emit(ServerEvent::Changed);
        Ok(device)
    }
}

pub(super) fn is_compatible(device: &ManagedNetworkSurface, panel: &Panel) -> bool {
    matches!((&device.layout, &panel.layout), (SurfaceLayout::Grid { columns: dc, rows: dr }, PanelLayout { columns: pc, rows: pr }) if dc == pc && dr == pr)
        && supports(&device.capabilities, &panel.capabilities)
}

fn supports(device: &SurfaceCapabilities, panel: &SurfaceCapabilities) -> bool {
    (!panel.supports_color || device.supports_color)
        && (!panel.supports_images || device.supports_images)
        && (!panel.supports_text || device.supports_text)
        && (!panel.supports_brightness || device.supports_brightness)
        && (!panel.supports_haptics || device.supports_haptics)
}

fn validate_panel(panel: &Panel) -> Result<(), String> {
    if panel.name.trim().is_empty() || panel.layout.columns == 0 || panel.layout.rows == 0 {
        return Err("panel name and layout dimensions are required".to_string());
    }
    let mut dial_indices = HashMap::new();
    for dial in &panel.dials {
        if dial_indices.insert(dial.index, ()).is_some() {
            return Err("panel dials cannot share an index".to_string());
        }
    }
    let mut positions = HashMap::new();
    for control in &panel.controls {
        if control.position.column >= panel.layout.columns
            || control.position.row >= panel.layout.rows
        {
            return Err("control position is outside panel layout".to_string());
        }
        if positions
            .insert((control.position.column, control.position.row), ())
            .is_some()
        {
            return Err("panel controls cannot share a position".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{identifiers::SurfaceId, surfaces::defaults::default_panel};

    #[test]
    fn rejects_assigning_a_panel_with_an_incompatible_layout() {
        let panel = default_panel();
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![panel.clone()]);
        let mut incompatible = panel;
        incompatible.panel_id.0 = "incompatible".to_string();
        incompatible.layout.columns = 3;
        registry
            .upsert_panel(incompatible)
            .expect("panel should be valid");

        assert!(registry
            .assign_active_panel("stream-deck-studio-1", "incompatible")
            .is_err());
    }

    #[test]
    fn deleting_the_active_panel_falls_back_to_a_compatible_one() {
        let mut spare = default_panel();
        spare.panel_id = PanelId("studio-panel-2".to_string());
        let registry =
            SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel(), spare]);

        registry
            .remove_panel("studio-panel-1")
            .expect("the active panel should be removable");

        let device = registry
            .managed(&SurfaceId("stream-deck-studio-1".to_string()))
            .expect("default device should exist");
        assert_eq!(
            device.active_panel_id.as_ref().map(|id| id.0.as_str()),
            Some("studio-panel-2")
        );
        assert!(registry.panel("studio-panel-1").is_none());
    }

    #[test]
    fn deleting_the_last_compatible_panel_leaves_the_device_unassigned() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);

        registry
            .remove_panel("studio-panel-1")
            .expect("the active panel should be removable");

        let device = registry
            .managed(&SurfaceId("stream-deck-studio-1".to_string()))
            .expect("default device should exist");
        assert_eq!(device.active_panel_id, None);
        assert!(registry.panels().is_empty());
        assert!(registry.remove_panel("studio-panel-1").is_none());
    }

    #[test]
    fn auto_provisions_a_panel_for_an_unseen_child_layout() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let layout = SurfaceLayout::Grid {
            columns: 5,
            rows: 3,
        };

        let panel_id = registry
            .ensure_default_panel_for_layout(&layout)
            .expect("a panel should be provisioned");
        let panel = registry.panel(&panel_id.0).expect("panel should exist");
        assert_eq!(panel.controls.len(), 15);
        assert_eq!((panel.layout.columns, panel.layout.rows), (5, 3));

        let reused = registry
            .ensure_default_panel_for_layout(&layout)
            .expect("existing panel should be reused");
        assert_eq!(reused, panel_id);
    }
}
