use crate::{
    identifiers::{ControlId, PanelId, SurfaceId},
    panels::{
        control::Control,
        rendered_state::{RenderedState, RgbaColor},
        {Panel, PanelLayout},
    },
    surfaces::{
        layout::{SurfaceCapabilities, SurfaceLayout, SurfacePosition},
        managed::{ManagedNetworkSurface, NetworkSurfaceStatus},
        panels::is_compatible,
    },
};

pub fn studio_capabilities() -> SurfaceCapabilities {
    SurfaceCapabilities {
        supports_color: true,
        supports_images: true,
        supports_text: true,
        supports_brightness: true,
        supports_haptics: false,
    }
}

pub(super) fn default_device(
    devices: &[ManagedNetworkSurface],
    active_panel_id: Option<PanelId>,
) -> ManagedNetworkSurface {
    ManagedNetworkSurface {
        surface_id: SurfaceId(format!("stream-deck-studio-{}", devices.len() + 1)),
        name: "Stream Deck Studio".to_string(),
        host: "127.0.0.1".to_string(),
        port: crate::drivers::streamdeck::studio::default_port(),
        serial_number: None,
        model: "Stream Deck Studio".to_string(),
        layout: SurfaceLayout::Grid {
            columns: 16,
            rows: 2,
        },
        capabilities: studio_capabilities(),
        active_panel_id,
        open_subpanels: Vec::new(),
        is_enabled: true,
        parent_surface_id: None,
        status: NetworkSurfaceStatus::Connecting,
        last_error: None,
    }
}

pub(crate) fn default_panel() -> Panel {
    Panel {
        panel_id: PanelId("studio-panel-1".to_string()),
        name: "Hello".to_string(),
        layout: PanelLayout {
            columns: 16,
            rows: 2,
        },
        capabilities: studio_capabilities(),
        controls: vec![Control {
            control_id: ControlId("hello".to_string()),
            name: "Hello".to_string(),
            position: SurfacePosition { column: 0, row: 0 },
            default_state: RenderedState {
                text: Some("Hello".to_string()),
                foreground_color: Some(white().into()),
                background_color: Some(color(35, 88, 165).into()),
                image: None,
                progress: None,
                content_layout: Default::default(),
                is_pressed: false,
            },
            pressed_state: Some(RenderedState {
                text: Some("Hello".to_string()),
                foreground_color: Some(white().into()),
                background_color: Some(color(18, 44, 83).into()),
                image: None,
                progress: None,
                content_layout: Default::default(),
                is_pressed: true,
            }),
            action_bindings: Vec::new(),
        }],
        dial_colors: vec![color(35, 88, 165), color(35, 88, 165)],
        dial_ring_levels: vec![100, 100],
    }
}

pub(super) fn repair_panel_assignments(
    devices: &mut [ManagedNetworkSurface],
    panels: &mut Vec<Panel>,
) {
    let needs_studio_panel = devices.iter().any(|device| {
        device.active_panel_id.as_ref().is_none_or(|panel_id| {
            panels
                .iter()
                .find(|panel| panel.panel_id == *panel_id)
                .is_none_or(|panel| !is_compatible(device, panel))
        }) && matches!(
            device.layout,
            SurfaceLayout::Grid {
                columns: 16,
                rows: 2
            }
        )
    });
    if needs_studio_panel
        && !panels
            .iter()
            .any(|panel| panel.layout.columns == 16 && panel.layout.rows == 2)
    {
        panels.push(default_panel());
    }

    for device in devices {
        let assignment_is_valid = device.active_panel_id.as_ref().and_then(|panel_id| {
            panels
                .iter()
                .find(|panel| panel.panel_id == *panel_id)
                .filter(|panel| is_compatible(device, panel))
        });
        if assignment_is_valid.is_some() {
            continue;
        }
        device.active_panel_id = panels
            .iter()
            .find(|panel| is_compatible(device, panel))
            .map(|panel| panel.panel_id.clone());
    }
}

pub(super) fn white() -> RgbaColor {
    color(u8::MAX, u8::MAX, u8::MAX)
}

pub(super) fn color(red: u8, green: u8, blue: u8) -> RgbaColor {
    RgbaColor {
        red,
        green,
        blue,
        alpha: u8::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::registry::SurfaceRegistry;

    #[test]
    fn repairs_a_missing_studio_panel_and_keeps_dial_settings_available() {
        let mut incompatible = default_panel();
        incompatible.panel_id = PanelId("studio-panel-2".to_string());
        incompatible.layout = PanelLayout {
            columns: 8,
            rows: 4,
        };
        let device = default_device(&[], Some(incompatible.panel_id.clone()));
        let registry = SurfaceRegistry::from_configuration(vec![device], vec![incompatible]);
        let device = registry
            .managed(&SurfaceId("stream-deck-studio-1".to_string()))
            .expect("configured device should exist");

        assert_eq!(
            device.active_panel_id.as_ref().map(|id| id.0.as_str()),
            Some("studio-panel-1")
        );
        assert_eq!(registry.active_dial_rings(&device.surface_id).len(), 2);
    }
}
