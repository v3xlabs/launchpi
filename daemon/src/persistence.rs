use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Mutex,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};

use crate::models::{
    identifiers::{PanelId, SurfaceId},
    network_surface::{ManagedNetworkSurface, NetworkSurfaceStatus},
    panel::Panel,
    surface::{SurfaceCapabilities, SurfaceLayout},
};

#[derive(Deserialize, Serialize)]
struct DevicesDocument {
    version: u8,
    devices: Vec<ManagedNetworkSurface>,
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

#[derive(Deserialize, Serialize)]
struct PanelsDocument {
    version: u8,
    panels: Vec<Panel>,
}

pub struct Persistence {
    devices_path: PathBuf,
    panels_path: PathBuf,
    database: SqlitePool,
    write_lock: Mutex<()>,
}

impl Persistence {
    pub async fn open() -> Result<(Self, Vec<ManagedNetworkSurface>, Vec<Panel>)> {
        let config_directory = config_directory()?;
        let state_directory = state_directory()?;
        fs::create_dir_all(&config_directory)?;
        fs::create_dir_all(&state_directory)?;

        let devices_path = config_directory.join("devices.toml");
        let panels_path = config_directory.join("panels.toml");
        let mut devices = load_devices(&devices_path)?;
        let panels = load_panels(&panels_path)?;
        let database_path = state_directory.join("runtime.sqlite3");
        let connection_options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", database_path.display()))?
                .create_if_missing(true);
        let database = SqlitePool::connect_with(connection_options).await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS surface_runtime (
                surface_id TEXT PRIMARY KEY NOT NULL,
                status TEXT NOT NULL,
                last_error TEXT,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&database)
        .await?;

        let runtime: HashMap<_, _> = sqlx::query_as::<_, (String, String, Option<String>)>(
            "SELECT surface_id, status, last_error FROM surface_runtime",
        )
        .fetch_all(&database)
        .await?
        .into_iter()
        .map(|(surface_id, status, last_error)| (surface_id, (status, last_error)))
        .collect();
        for device in &mut devices {
            if let Some((status, last_error)) = runtime.get(&device.surface_id.0) {
                device.status = parse_status(status)?;
                device.last_error = last_error.clone();
            }
        }

        Ok((
            Self {
                devices_path,
                panels_path,
                database,
                write_lock: Mutex::new(()),
            },
            devices,
            panels,
        ))
    }

    pub fn save_configuration(
        &self,
        devices: Vec<ManagedNetworkSurface>,
        panels: Vec<Panel>,
    ) -> Result<()> {
        let _write_lock = self.write_lock.lock().unwrap();
        write_toml(
            &self.devices_path,
            &PersistedDevicesDocument {
                version: 1,
                devices: devices.into_iter().map(PersistedDevice::from).collect(),
            },
        )?;
        write_toml(&self.panels_path, &PanelsDocument { version: 1, panels })
    }

    pub fn render_panel(&self, panel: Panel) -> Result<String> {
        Ok(toml::to_string_pretty(&PanelsDocument {
            version: 1,
            panels: vec![panel],
        })?)
    }

    pub async fn record_status(
        &self,
        surface_id: String,
        status: NetworkSurfaceStatus,
        last_error: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO surface_runtime (surface_id, status, last_error, updated_at)
             VALUES (?, ?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT(surface_id) DO UPDATE SET
                 status = excluded.status,
                 last_error = excluded.last_error,
                 updated_at = excluded.updated_at",
        )
        .bind(surface_id)
        .bind(status_name(&status))
        .bind(last_error)
        .execute(&self.database)
        .await?;
        Ok(())
    }
}

fn write_toml<T: Serialize>(path: &Path, document: &T) -> Result<()> {
    let temporary_path = path.with_extension("toml.tmp");
    fs::write(&temporary_path, toml::to_string_pretty(document)?)?;
    fs::rename(temporary_path, path)?;
    Ok(())
}

fn load_devices(path: &Path) -> Result<Vec<ManagedNetworkSurface>> {
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
    Ok(config.devices)
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

fn load_panels(path: &Path) -> Result<Vec<Panel>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("unable to read {}", path.display()))?;
    let config: PanelsDocument =
        toml::from_str(&contents).with_context(|| format!("unable to parse {}", path.display()))?;
    if config.version != 1 {
        anyhow::bail!("unsupported panel configuration version {}", config.version);
    }
    Ok(config.panels)
}

fn config_directory() -> Result<PathBuf> {
    if let Some(path) = env::var_os("LAUNCHPI_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("launchpi"));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/launchpi"))
}

fn state_directory() -> Result<PathBuf> {
    if let Some(path) = env::var_os("LAUNCHPI_STATE_DIR") {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("launchpi"));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/state/launchpi"))
}

fn status_name(status: &NetworkSurfaceStatus) -> &'static str {
    match status {
        NetworkSurfaceStatus::Connecting => "connecting",
        NetworkSurfaceStatus::Connected => "connected",
        NetworkSurfaceStatus::Unavailable => "unavailable",
        NetworkSurfaceStatus::Disabled => "disabled",
    }
}

fn parse_status(status: &str) -> Result<NetworkSurfaceStatus> {
    match status {
        "connecting" => Ok(NetworkSurfaceStatus::Connecting),
        "connected" => Ok(NetworkSurfaceStatus::Connected),
        "unavailable" => Ok(NetworkSurfaceStatus::Unavailable),
        "disabled" => Ok(NetworkSurfaceStatus::Disabled),
        _ => anyhow::bail!("unsupported runtime status {status}"),
    }
}
