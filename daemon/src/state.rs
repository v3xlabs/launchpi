use std::sync::Arc;

use crate::{
    assets::AssetStore,
    config::{
        changes::{ConfigurationChanges, Entry, Snapshot},
        devices::PersistedDevice,
        plugins::PluginDirectory,
        store::Persistence,
        ExportFormat,
    },
    identifiers::SurfaceId,
    plugins::engine::PluginEngine,
    surfaces::{
        defaults::default_panel,
        managed::{ManagedNetworkSurface, NetworkSurfaceStatus},
        registry::SurfaceRegistry,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub surfaces: Arc<SurfaceRegistry>,
    pub plugins: Arc<PluginEngine>,
    pub assets: Arc<AssetStore>,
    persistence: Arc<Persistence>,
    /// The configuration as it stood once loading finished, which is what a declarative setup
    /// already describes. Everything since is what the user would have to carry over by hand.
    baseline: Arc<Snapshot>,
}

impl AppState {
    pub async fn load() -> anyhow::Result<Self> {
        let (persistence, devices, mut panels) = Persistence::open().await?;
        let persistence = Arc::new(persistence);
        if panels.is_empty() {
            panels.push(default_panel());
        }
        let surfaces = Arc::new(SurfaceRegistry::from_configuration(devices, panels));
        let config_directory = crate::config::config_directory()?;
        let directory = PluginDirectory::open(&config_directory)?;
        // The engine repaints when bytes land, so the store needs a way to say so.
        let (assets_ready, assets_ready_receiver) = tokio::sync::mpsc::channel::<String>(8);
        let assets = Arc::new(AssetStore::open(
            crate::config::cache_directory()?.join("assets"),
            reqwest::Client::new(),
            assets_ready,
        )?);
        let assets_for_engine = assets.clone();
        let input = surfaces
            .take_input_receiver()
            .expect("the input receiver has not been taken yet");
        let plugins = PluginEngine::start(
            surfaces.clone(),
            surfaces.variables(),
            directory,
            persistence.clone(),
            config_directory.join("values.toml"),
            assets_for_engine,
            assets_ready_receiver,
            input,
        )
        .await;
        let baseline = Arc::new(Snapshot::new(&Self::entries(&surfaces, &plugins))?);
        Ok(Self {
            surfaces,
            plugins,
            assets,
            persistence,
            baseline,
        })
    }

    /// Child devices are discovered behind a dock on every connect, so they are never persisted.
    fn root_devices(surfaces: &SurfaceRegistry) -> Vec<ManagedNetworkSurface> {
        surfaces
            .managed_surfaces()
            .into_iter()
            .filter(|device| device.parent_surface_id.is_none())
            .collect()
    }

    fn entries(surfaces: &SurfaceRegistry, plugins: &PluginEngine) -> Vec<Entry> {
        let devices = Self::root_devices(surfaces)
            .into_iter()
            .map(|device| Entry::Device(PersistedDevice::from(device)));
        let panels = surfaces.panels().into_iter().map(Entry::Panel);
        let instances = plugins
            .instance_documents()
            .into_iter()
            .map(|(identity, document)| Entry::Plugin(identity, document));
        let values = plugins.user_values().into_iter().map(Entry::Value);
        devices
            .chain(panels)
            .chain(instances)
            .chain(values)
            .collect()
    }

    fn configuration_entries(&self) -> Vec<Entry> {
        Self::entries(&self.surfaces, &self.plugins)
    }

    pub fn configuration_changes(&self) -> anyhow::Result<ConfigurationChanges> {
        ConfigurationChanges::new(&self.baseline, &self.configuration_entries())
    }

    pub fn configuration_change_count(&self) -> anyhow::Result<usize> {
        Ok(Snapshot::new(&self.configuration_entries())?
            .changes_since(&self.baseline)
            .len())
    }

    pub fn persist_configuration(&self) -> anyhow::Result<()> {
        self.persistence
            .save_configuration(Self::root_devices(&self.surfaces), self.surfaces.panels())
    }

    pub fn export_panel_configuration(
        &self,
        panel_id: &str,
        format: ExportFormat,
    ) -> anyhow::Result<Option<String>> {
        let Some(panel) = self.surfaces.panel(panel_id) else {
            return Ok(None);
        };
        match format {
            ExportFormat::Toml => self.persistence.render_panel(panel),
            ExportFormat::Nix => Entry::Panel(panel).nix(),
        }
        .map(Some)
    }

    pub fn export_device_configuration(
        &self,
        surface_id: &str,
        format: ExportFormat,
    ) -> anyhow::Result<Option<String>> {
        let Some(device) = self.surfaces.managed(&SurfaceId(surface_id.to_string())) else {
            return Ok(None);
        };
        match format {
            ExportFormat::Toml => self.persistence.render_device(device),
            ExportFormat::Nix => Entry::Device(PersistedDevice::from(device)).nix(),
        }
        .map(Some)
    }

    pub fn export_configuration(&self) -> anyhow::Result<String> {
        let configuration = self
            .persistence
            .render_configuration(Self::root_devices(&self.surfaces), self.surfaces.panels())?;
        let values = self
            .plugins
            .export_user_values()
            .map_err(anyhow::Error::msg)?;
        Ok(format!("{configuration}\n# values.toml\n{values}"))
    }

    pub fn update_status(
        &self,
        surface_id: &SurfaceId,
        status: NetworkSurfaceStatus,
        last_error: Option<String>,
    ) {
        self.surfaces
            .update_status(surface_id, status.clone(), last_error.clone());
        let persistence = self.persistence.clone();
        let surface_id = surface_id.0.clone();
        tokio::spawn(async move {
            let _ = persistence
                .record_status(surface_id, status, last_error)
                .await;
        });
    }
}
