use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    config::write_toml,
    drivers::streamdeck::model::{model_by_name, STREAM_DECK},
    identifiers::{PanelId, SurfaceId},
    surfaces::{
        defaults::stream_deck_capabilities,
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
    #[serde(default = "crate::surfaces::managed::default_brightness")]
    brightness: u8,
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
            capabilities: device
                .capabilities
                .unwrap_or_else(|| stream_deck_capabilities(model.unwrap_or(&STREAM_DECK))),
            model: device.model,
            active_panel_id: device.active_panel_id,
            brightness: device.brightness,
            is_display_off: false,
            open_subpanels: Vec::new(),
            is_enabled: device.is_enabled,
            parent_surface_id: None,
            status: NetworkSurfaceStatus::Connecting,
            last_error: None,
        }
    }
}

#[derive(Serialize)]
struct PersistedDevicesDocument {
    version: u8,
    devices: Vec<PersistedDevice>,
}

#[derive(Serialize)]
pub struct PersistedDevice {
    pub surface_id: SurfaceId,
    pub name: String,
    host: String,
    port: u16,
    serial_number: Option<String>,
    model: String,
    layout: SurfaceLayout,
    capabilities: SurfaceCapabilities,
    active_panel_id: Option<PanelId>,
    brightness: u8,
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
            brightness: device.brightness,
            is_enabled: device.is_enabled,
        }
    }
}

impl PersistedDevice {
    /// The shape of one `services.launchpi.settings.devices` entry. A known model derives its
    /// layout and capabilities on load, so the entry leaves them out.
    pub fn nix_settings(&self) -> Result<toml::Table> {
        let mut table = toml::Table::try_from(self)?;
        if let Some(is_enabled) = table.remove("is_enabled") {
            table.insert("enable".to_string(), is_enabled);
        }
        if model_by_name(&self.model).is_some() {
            table.remove("layout");
            table.remove("capabilities");
        }
        Ok(table)
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
    if !matches!(config.version, 1 | 2) {
        anyhow::bail!(
            "unsupported device configuration version {}",
            config.version
        );
    }
    if let Some(device) = config.devices.iter().find(|device| device.brightness > 100) {
        anyhow::bail!(
            "device {} has brightness {}, expected 0 through 100",
            device.surface_id.0,
            device.brightness
        );
    }
    Ok(config
        .devices
        .into_iter()
        .map(ManagedNetworkSurface::from)
        .collect())
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
        version: 2,
        devices: devices.into_iter().map(PersistedDevice::from).collect(),
    }
}
