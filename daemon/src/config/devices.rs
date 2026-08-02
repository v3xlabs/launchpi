use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    config::write_toml,
    drivers::streamdeck::model::{model_by_name, STREAM_DECK, STREAM_DECK_NETWORK_DOCK},
    identifiers::{PanelId, SurfaceId},
    surfaces::{
        layout::{SurfaceCapabilities, SurfaceLayout},
        managed::{ManagedNetworkSurface, NetworkSurfaceStatus},
    },
};

#[derive(Deserialize)]
struct DevicesDocument {
    version: u8,
    devices: Vec<ConfiguredDevice>,
}

#[derive(Deserialize)]
struct ConfiguredDevice {
    surface_id: SurfaceId,
    name: String,
    host: String,
    port: u16,
    serial_number: Option<String>,
    model: String,
    #[serde(default)]
    layout: Option<SurfaceLayout>,
    #[serde(default)]
    capabilities: Option<SurfaceCapabilities>,
    active_panel_id: Option<PanelId>,
    is_enabled: bool,
}

impl From<ConfiguredDevice> for ManagedNetworkSurface {
    fn from(device: ConfiguredDevice) -> Self {
        let model = model_by_name(&device.model);
        Self {
            surface_id: device.surface_id,
            name: device.name,
            host: device.host,
            port: device.port,
            serial_number: device.serial_number,
            layout: device
                .layout
                .unwrap_or_else(|| model.map_or(STREAM_DECK.layout, |model| model.layout)),
            capabilities: device.capabilities.unwrap_or_else(|| {
                if model == Some(&STREAM_DECK_NETWORK_DOCK) {
                    SurfaceCapabilities::default()
                } else {
                    stream_deck_capabilities()
                }
            }),
            model: device.model,
            active_panel_id: device.active_panel_id,
            open_subpanels: Vec::new(),
            is_enabled: device.is_enabled,
            parent_surface_id: None,
            status: NetworkSurfaceStatus::Connecting,
            last_error: None,
        }
    }
}

fn stream_deck_capabilities() -> SurfaceCapabilities {
    SurfaceCapabilities {
        supports_color: true,
        supports_images: true,
        supports_text: true,
        supports_brightness: true,
        supports_haptics: false,
    }
}

#[derive(Serialize)]
struct PersistedDevicesDocument {
    version: u8,
    devices: Vec<PersistedDevice>,
}

#[derive(Serialize)]
struct PersistedDevice {
    surface_id: SurfaceId,
    name: String,
    host: String,
    port: u16,
    serial_number: Option<String>,
    model: String,
    layout: SurfaceLayout,
    capabilities: SurfaceCapabilities,
    active_panel_id: Option<PanelId>,
    is_enabled: bool,
}

impl From<ManagedNetworkSurface> for PersistedDevice {
    fn from(device: ManagedNetworkSurface) -> Self {
        Self {
            surface_id: device.surface_id,
            name: device.name,
            host: device.host,
            port: device.port,
            serial_number: device.serial_number,
            model: device.model,
            layout: device.layout,
            capabilities: device.capabilities,
            active_panel_id: device.active_panel_id,
            is_enabled: device.is_enabled,
        }
    }
}

#[derive(Deserialize)]
struct LegacySurfacesDocument {
    surfaces: Vec<ManagedNetworkSurface>,
}

pub fn load(path: &Path) -> Result<Vec<ManagedNetworkSurface>> {
    if !path.exists() {
        return load_legacy_surfaces(&path.with_file_name("surfaces.toml"));
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("unable to read {}", path.display()))?;
    let config: DevicesDocument =
        toml::from_str(&contents).with_context(|| format!("unable to parse {}", path.display()))?;
    if config.version != 1 {
        anyhow::bail!(
            "unsupported device configuration version {}",
            config.version
        );
    }
    Ok(config.devices.into_iter().map(ManagedNetworkSurface::from).collect())
}

fn load_legacy_surfaces(path: &Path) -> Result<Vec<ManagedNetworkSurface>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents =
        fs::read_to_string(path).with_context(|| format!("unable to read {}", path.display()))?;
    let config: LegacySurfacesDocument =
        toml::from_str(&contents).with_context(|| format!("unable to parse {}", path.display()))?;

    Ok(config.surfaces)
}

pub fn save(path: &Path, devices: Vec<ManagedNetworkSurface>) -> Result<()> {
    write_toml(path, &document(devices))
}

pub fn render(devices: Vec<ManagedNetworkSurface>) -> Result<String> {
    Ok(toml::to_string_pretty(&document(devices))?)
}

fn document(devices: Vec<ManagedNetworkSurface>) -> PersistedDevicesDocument {
    PersistedDevicesDocument {
        version: 1,
        devices: devices.into_iter().map(PersistedDevice::from).collect(),
    }
}
