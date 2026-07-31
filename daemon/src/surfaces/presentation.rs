use serde::Serialize;

use crate::{
    bindings::action::SubpanelPlacement,
    identifiers::SurfaceId,
    panels::{control::Control, Panel},
    surfaces::{
        layout::{SurfaceLayout, SurfacePosition},
        managed::OpenSubpanel,
        registry::SurfaceRegistry,
    },
};

#[derive(Clone, Debug, Serialize)]
pub struct PresentationControl {
    pub control: Control,
    pub key_index: u8,
    pub is_dimmed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SurfacePresentation {
    pub columns: u16,
    pub rows: u16,
    pub controls: Vec<PresentationControl>,
}

impl SurfaceRegistry {
    pub fn open_subpanel(
        &self,
        surface_id: &SurfaceId,
        panel_id: &str,
        anchor: SurfacePosition,
        placement: SubpanelPlacement,
        offset_columns: i16,
        offset_rows: i16,
    ) -> Result<(), String> {
        let device = self
            .managed(surface_id)
            .ok_or_else(|| "managed device was not found".to_string())?;
        let panel = self
            .panel(panel_id)
            .ok_or_else(|| "subpanel was not found".to_string())?;
        let SurfaceLayout::Grid { columns, rows } = device.layout else {
            return Err("subpanels require a grid surface".to_string());
        };
        let (column, row) = subpanel_origin(
            anchor,
            panel.layout.columns,
            panel.layout.rows,
            columns,
            rows,
            placement,
            offset_columns,
            offset_rows,
        );
        let mut devices = self.managed.write().unwrap();
        let device = devices
            .get_mut(&surface_id.0)
            .ok_or_else(|| "managed device was not found".to_string())?;
        device.open_subpanels.push(OpenSubpanel {
            panel_id: panel.panel_id,
            column,
            row,
        });
        drop(devices);
        self.render_active_panel(&surface_id.0);
        self.emit(crate::events::ServerEvent::PresentationChanged {
            surface_id: surface_id.clone(),
        });
        Ok(())
    }

    pub fn close_subpanel(&self, surface_id: &SurfaceId) -> bool {
        let closed = self
            .managed
            .write()
            .unwrap()
            .get_mut(&surface_id.0)
            .is_some_and(|device| device.open_subpanels.pop().is_some());
        if closed {
            self.render_active_panel(&surface_id.0);
            self.emit(crate::events::ServerEvent::PresentationChanged {
                surface_id: surface_id.clone(),
            });
        }
        closed
    }

    pub(super) fn top_subpanel_control_at(
        &self,
        surface_id: &SurfaceId,
        key_index: u8,
    ) -> Option<Control> {
        let device = self.managed(surface_id)?;
        let layer = device.open_subpanels.last()?;
        let panel = self.panel(&layer.panel_id.0)?;
        let SurfaceLayout::Grid { columns, rows } = device.layout else {
            return None;
        };
        let column = u16::from(key_index) % columns;
        let row = u16::from(key_index) / columns;
        if row >= rows {
            return None;
        }
        let panel_column = i32::from(column) - i32::from(layer.column);
        let panel_row = i32::from(row) - i32::from(layer.row);
        if panel_column < 0 || panel_row < 0 {
            return None;
        }
        panel.controls.into_iter().find_map(|mut control| {
            (control.position.column == panel_column as u16 && control.position.row == panel_row as u16)
                .then(|| {
                    control.position = SurfacePosition { column, row };
                    control
                })
        })
    }

    pub(super) fn has_open_subpanel(&self, surface_id: &SurfaceId) -> bool {
        self.managed(surface_id)
            .is_some_and(|device| !device.open_subpanels.is_empty())
    }

    pub fn presentation_panels(&self, surface_id: &SurfaceId) -> Vec<Panel> {
        let Some(device) = self.managed(surface_id) else {
            return Vec::new();
        };
        let Some(root_id) = device.active_panel_id else {
            return Vec::new();
        };
        let Some(root) = self.panel(&root_id.0) else {
            return Vec::new();
        };
        let SurfaceLayout::Grid { columns, rows } = device.layout else {
            return vec![root];
        };
        let mut visible = vec![root];
        for layer in device.open_subpanels {
            let Some(mut panel) = self.panel(&layer.panel_id.0) else {
                continue;
            };
            panel.layout.columns = columns;
            panel.layout.rows = rows;
            panel.controls = panel
                .controls
                .into_iter()
                .filter_map(|mut control| {
                    let column = i32::from(layer.column) + i32::from(control.position.column);
                    let row = i32::from(layer.row) + i32::from(control.position.row);
                    if column < 0 || row < 0 || column >= i32::from(columns) || row >= i32::from(rows) {
                        return None;
                    }
                    control.position = SurfacePosition {
                        column: column as u16,
                        row: row as u16,
                    };
                    Some(control)
                })
                .collect();
            visible.push(panel);
        }
        visible
    }

    pub fn presentation(&self, surface_id: &SurfaceId) -> Option<SurfacePresentation> {
        let device = self.managed(surface_id)?;
        let SurfaceLayout::Grid { columns, rows } = device.layout else {
            return None;
        };
        let root = self.panel(&device.active_panel_id?.0)?;
        let has_overlay = !device.open_subpanels.is_empty();
        let top = device.open_subpanels.last();
        let controls = (0..u32::from(columns) * u32::from(rows))
            .filter_map(|index| {
                let key_index = u8::try_from(index).ok()?;
                let column = u16::from(key_index) % columns;
                let row = u16::from(key_index) / columns;
                let top_control = top.and_then(|layer| {
                    let panel = self.panel(&layer.panel_id.0)?;
                    let panel_column = i32::from(column) - i32::from(layer.column);
                    let panel_row = i32::from(row) - i32::from(layer.row);
                    panel.controls.into_iter().find_map(|mut control| {
                        (panel_column >= 0
                            && panel_row >= 0
                            && control.position.column == panel_column as u16
                            && control.position.row == panel_row as u16)
                            .then(|| {
                                control.position = SurfacePosition { column, row };
                                control
                            })
                    })
                });
                let is_dimmed = has_overlay && top_control.is_none();
                let control = top_control.or_else(|| {
                    root.controls
                        .iter()
                        .find(|control| control.position.column == column && control.position.row == row)
                        .cloned()
                })?;
                Some(PresentationControl {
                    control,
                    key_index,
                    is_dimmed,
                })
            })
            .collect();
        Some(SurfacePresentation { columns, rows, controls })
    }
}

fn subpanel_origin(
    anchor: SurfacePosition,
    panel_columns: u16,
    panel_rows: u16,
    viewport_columns: u16,
    viewport_rows: u16,
    placement: SubpanelPlacement,
    offset_columns: i16,
    offset_rows: i16,
) -> (i16, i16) {
    let anchor_column = i32::from(anchor.column);
    let anchor_row = i32::from(anchor.row);
    let panel_columns = i32::from(panel_columns);
    let panel_rows = i32::from(panel_rows);
    let (column, row) = match placement {
        SubpanelPlacement::TopStart => (anchor_column, anchor_row - panel_rows),
        SubpanelPlacement::TopCenter => (anchor_column - panel_columns / 2, anchor_row - panel_rows),
        SubpanelPlacement::TopEnd => (anchor_column - panel_columns + 1, anchor_row - panel_rows),
        SubpanelPlacement::StartCenter => (anchor_column - panel_columns, anchor_row - panel_rows / 2),
        SubpanelPlacement::EndCenter => (anchor_column + 1, anchor_row - panel_rows / 2),
        SubpanelPlacement::BottomStart => (anchor_column, anchor_row + 1),
        SubpanelPlacement::BottomCenter => (anchor_column - panel_columns / 2, anchor_row + 1),
        SubpanelPlacement::BottomEnd => (anchor_column - panel_columns + 1, anchor_row + 1),
    };
    let max_column = (i32::from(viewport_columns) - panel_columns).max(0);
    let max_row = (i32::from(viewport_rows) - panel_rows).max(0);
    (
        (column + i32::from(offset_columns)).clamp(0, max_column) as i16,
        (row + i32::from(offset_rows)).clamp(0, max_row) as i16,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        identifiers::{ControlId, PanelId},
        surfaces::defaults::default_panel,
    };

    #[test]
    fn clamps_an_oversized_subpanel_to_the_surface_origin() {
        assert_eq!(
            subpanel_origin(
                SurfacePosition { column: 3, row: 1 },
                8,
                4,
                4,
                2,
                SubpanelPlacement::BottomEnd,
                0,
                0,
            ),
            (0, 0)
        );
    }

    #[test]
    fn applies_offsets_after_placement() {
        assert_eq!(
            subpanel_origin(
                SurfacePosition { column: 1, row: 0 },
                2,
                1,
                8,
                4,
                SubpanelPlacement::BottomStart,
                2,
                1,
            ),
            (3, 2)
        );
    }

    #[test]
    fn topmost_subpanel_receives_input_and_closes_on_an_outside_press() {
        let root = default_panel();
        let mut subpanel = default_panel();
        subpanel.panel_id = PanelId("subpanel".to_string());
        subpanel.layout.columns = 1;
        subpanel.layout.rows = 1;
        subpanel.controls.truncate(1);
        subpanel.controls[0].control_id = ControlId("subpanel-control".to_string());
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![root]);
        registry.upsert_panel(subpanel).expect("subpanel is valid");
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());

        registry
            .open_subpanel(
                &surface_id,
                "subpanel",
                SurfacePosition { column: 0, row: 0 },
                SubpanelPlacement::BottomStart,
                0,
                0,
            )
            .expect("subpanel opens");

        assert_eq!(
            registry
                .top_subpanel_control_at(&surface_id, 16)
                .map(|control| control.control_id.0),
            Some("subpanel-control".to_string())
        );
        assert!(registry.top_subpanel_control_at(&surface_id, 0).is_none());
        assert!(registry.close_subpanel(&surface_id));
    }
}
