use std::sync::Arc;

use crate::{
    assets::AssetStore,
    config::{plugins::PluginDirectory, store::Persistence},
    identifiers::SurfaceId,
    plugins::engine::PluginEngine,
    surfaces::{defaults::default_panel, managed::NetworkSurfaceStatus, registry::SurfaceRegistry},
};

#[derive(Clone)]
pub struct AppState {
    pub surfaces: Arc<SurfaceRegistry>,
    pub plugins: Arc<PluginEngine>,
    pub assets: Arc<AssetStore>,
    persistence: Arc<Persistence>,
}

impl AppState {
    pub async fn load() -> anyhow::Result<Self> {
        let (persistence, devices, mut panels) = Persistence::open().await?;
        if panels.is_empty() {
            panels.push(default_panel());
        }
        let surfaces = Arc::new(SurfaceRegistry::from_configuration(devices, panels));
        let config_directory = crate::config::config_directory()?;
        let directory = PluginDirectory::open(&config_directory)?;
        let assets = Arc::new(AssetStore::open(crate::config::cache_directory()?)?);
        let assets_for_engine = assets.clone();
        let input = surfaces
            .take_input_receiver()
            .expect("the input receiver has not been taken yet");
        let plugins = PluginEngine::start(
            surfaces.clone(),
            surfaces.variables(),
            directory,
            config_directory.join("values.toml"),
            assets_for_engine,
            input,
        )
        .await;
        Ok(Self {
            surfaces,
            plugins,
            assets,
            persistence: Arc::new(persistence),
        })
    }

    pub fn persist_configuration(&self) -> anyhow::Result<()> {
        let devices = self
            .surfaces
            .managed_surfaces()
            .into_iter()
            .filter(|device| device.parent_surface_id.is_none())
            .collect();
        self.persistence
            .save_configuration(devices, self.surfaces.panels())
    }

    pub fn export_panel_configuration(&self, panel_id: &str) -> anyhow::Result<Option<String>> {
        let Some(panel) = self.surfaces.panel(panel_id) else {
            return Ok(None);
        };
        self.persistence.render_panel(panel).map(Some)
    }

    pub fn export_device_configuration(&self, surface_id: &str) -> anyhow::Result<Option<String>> {
        let Some(device) = self.surfaces.managed(&SurfaceId(surface_id.to_string())) else {
            return Ok(None);
        };
        self.persistence.render_device(device).map(Some)
    }

    pub fn export_configuration(&self) -> anyhow::Result<String> {
        let devices = self
            .surfaces
            .managed_surfaces()
            .into_iter()
            .filter(|device| device.parent_surface_id.is_none())
            .collect();
        self.persistence
            .render_configuration(devices, self.surfaces.panels())
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
