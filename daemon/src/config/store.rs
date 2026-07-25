use std::{fs, path::PathBuf, sync::Mutex};

use anyhow::Result;

use crate::{
    config::{config_directory, runtime::RuntimeStatusStore, state_directory},
    panels::Panel,
    surfaces::managed::{ManagedNetworkSurface, NetworkSurfaceStatus},
};

pub struct Persistence {
    devices_path: PathBuf,
    panels_path: PathBuf,
    runtime: RuntimeStatusStore,
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
        let mut devices = crate::config::devices::load(&devices_path)?;
        let panels = crate::config::panels::load(&panels_path)?;
        let runtime = RuntimeStatusStore::open(&state_directory.join("runtime.sqlite3")).await?;
        runtime.apply(&mut devices).await?;

        Ok((
            Self {
                devices_path,
                panels_path,
                runtime,
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
        crate::config::devices::save(&self.devices_path, devices)?;
        crate::config::panels::save(&self.panels_path, panels)
    }

    pub fn render_panel(&self, panel: Panel) -> Result<String> {
        crate::config::panels::render(vec![panel])
    }

    pub fn render_device(&self, device: ManagedNetworkSurface) -> Result<String> {
        crate::config::devices::render(vec![device])
    }

    /// Every configuration file as one document, separated by the path each section belongs in.
    /// The sections are the same schema the daemon loads, so they can be split back apart verbatim.
    pub fn render_configuration(
        &self,
        devices: Vec<ManagedNetworkSurface>,
        panels: Vec<Panel>,
    ) -> Result<String> {
        let devices = crate::config::devices::render(devices)?;
        let panels = crate::config::panels::render(panels)?;
        Ok(format!(
            "# devices.toml\n{devices}\n# panels.toml\n{panels}"
        ))
    }

    pub async fn record_status(
        &self,
        surface_id: String,
        status: NetworkSurfaceStatus,
        last_error: Option<String>,
    ) -> Result<()> {
        self.runtime.record(surface_id, status, last_error).await
    }
}
