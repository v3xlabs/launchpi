use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use serde_json::Value as JsonValue;
use tokio::{
    sync::{mpsc, Notify},
    task::JoinHandle,
};
use tracing::{debug, info, warn};

use crate::{
    assets::AssetStore,
    bindings::action::{Action, ActionTrigger},
    config::{
        plugins::{export_document, InstanceFile, PluginDirectory},
        values::{self, UserValue},
    },
    events::ServerEvent,
    identifiers::{ControlId, IntegrationId, SurfaceId},
    panels::{control::Control, Panel},
    plugins::{
        instance::{
            parse_instance_stem, InstanceConfig, InstanceDocument, InstanceIdentity,
            PluginInstance, PluginInstanceStatus, INSTANCE_DOCUMENT_VERSION,
        },
        manifest::PluginManifest,
        plugin::{cancellation, CancelHandle, LookupOption, Plugin, PluginContext, PluginError},
        preset::{Preset, PresetStore},
        registry,
    },
    rendering::index::{DependencyIndex, RenderTarget},
    surfaces::{layout::SurfacePosition, logs::SurfaceLogLevel, registry::SurfaceRegistry},
    variables::{VariableRef, VariableStore, VariableValue},
};

/// How long the engine waits after the first change before repainting, so a burst of variable
/// updates becomes one repaint rather than one per update.
const COALESCE_WINDOW: Duration = Duration::from_millis(50);
const SIGNAL_QUEUE_SIZE: usize = 1024;
pub const INPUT_QUEUE_SIZE: usize = 256;
/// The lookup a plugin answers with the references it could publish.
pub const SUGGESTION_SOURCE: &str = "values";
/// Enough to scroll, few enough to render. Anything longer is a sign the query needs narrowing.
const SUGGESTION_LIMIT: usize = 50;
/// The ceiling once a single instance has been picked, where the list is a catalogue to browse
/// rather than a ranked page.
const SCOPED_SUGGESTION_LIMIT: usize = 1000;

/// Takes from each source in turn until the page is full, so no one instance can crowd out the
/// others.
///
/// Sorting the union instead would rank by name, which is not relevance at all: an instance whose
/// id sorts early would take every slot and the entity actually being searched for would fall off
/// the end. Each source arrives already ranked by whoever knows how to rank it, and this preserves
/// that while guaranteeing every source is represented.
fn interleave(sources: Vec<Vec<LookupOption>>, limit: usize) -> Vec<LookupOption> {
    let mut taken: Vec<LookupOption> = Vec::new();
    let mut round = 0;

    while taken.len() < limit {
        let mut offered_any = false;

        for source in &sources {
            let Some(option) = source.get(round) else {
                continue;
            };
            offered_any = true;
            if !taken.iter().any(|existing| existing.value == option.value) {
                taken.push(option.clone());
                if taken.len() == limit {
                    return taken;
                }
            }
        }

        if !offered_any {
            break;
        }
        round += 1;
    }

    taken
}

/// A gesture that reached the daemon. Deliberately not the `ServerEvent` broadcast, which drops
/// messages when a receiver lags: a dropped render is cosmetic, a dropped action is a light that
/// did not turn on.
#[derive(Clone, Debug)]
pub enum InputEvent {
    Key {
        surface_id: SurfaceId,
        key_index: u8,
        is_pressed: bool,
        control: Option<Control>,
    },
}

#[derive(Clone, Debug)]
pub enum EngineSignal {
    VariableChanged(VariableRef),
    PresetsChanged(IntegrationId),
    InstanceLog {
        integration_id: IntegrationId,
        level: SurfaceLogLevel,
        message: String,
    },
    InventoryChanged,
}

struct RunningInstance {
    identity: InstanceIdentity,
    document: InstanceDocument,
    status: PluginInstanceStatus,
    plugin: Option<Arc<dyn Plugin>>,
    cancel: Option<CancelHandle>,
}

impl RunningInstance {
    fn describe(&self) -> PluginInstance {
        let mut config = self.document.config.clone();
        for key in secret_keys(&self.identity.plugin_type) {
            config.remove(&key);
        }

        PluginInstance {
            config: serde_json::to_value(&config).unwrap_or(serde_json::Value::Null),
            integration_id: self.identity.integration_id(),
            plugin_type: self.identity.plugin_type.clone(),
            name: self.identity.name.clone(),
            display_name: self
                .document
                .display_name
                .clone()
                .unwrap_or_else(|| self.identity.integration_id().0),
            is_enabled: self.document.enabled,
            status: self.status.clone(),
        }
    }
}

pub struct PluginEngine {
    surfaces: Arc<SurfaceRegistry>,
    variables: Arc<VariableStore>,
    presets: Arc<PresetStore>,
    directory: PluginDirectory,
    values_path: PathBuf,
    http: reqwest::Client,
    assets: Arc<AssetStore>,
    instances: RwLock<HashMap<IntegrationId, RunningInstance>>,
    index: RwLock<DependencyIndex>,
    dirty: Mutex<HashSet<RenderTarget>>,
    dirty_notify: Notify,
    signals: mpsc::Sender<EngineSignal>,
    hold_timers: Mutex<HashMap<(SurfaceId, ControlId), Vec<JoinHandle<()>>>>,
}

impl PluginEngine {
    pub async fn start(
        surfaces: Arc<SurfaceRegistry>,
        variables: Arc<VariableStore>,
        directory: PluginDirectory,
        values_path: PathBuf,
        assets: Arc<AssetStore>,
        assets_ready: mpsc::Receiver<String>,
        input: mpsc::Receiver<InputEvent>,
    ) -> Arc<Self> {
        let (signals, signal_receiver) = mpsc::channel(SIGNAL_QUEUE_SIZE);
        let engine = Arc::new(Self {
            surfaces,
            variables,
            presets: Arc::default(),
            directory,
            values_path,
            http: reqwest::Client::new(),
            assets,
            instances: RwLock::default(),
            index: RwLock::default(),
            dirty: Mutex::default(),
            dirty_notify: Notify::new(),
            signals,
            hold_timers: Mutex::default(),
        });

        engine.load_user_values();
        engine.load_instances().await;
        engine.rebuild_index().await;

        tokio::spawn(run_signals(engine.clone(), signal_receiver));
        tokio::spawn(run_input(engine.clone(), input));
        tokio::spawn(run_flush(engine.clone()));
        tokio::spawn(watch_inventory(engine.clone()));
        tokio::spawn(watch_assets(engine.clone(), assets_ready));

        engine
    }

    pub fn manifests(&self) -> Vec<PluginManifest> {
        registry()
            .iter()
            .map(|factory| (factory.manifest)())
            .collect()
    }

    pub fn instances(&self) -> Vec<PluginInstance> {
        let mut described: Vec<_> = self
            .instances
            .read()
            .unwrap()
            .values()
            .map(RunningInstance::describe)
            .collect();
        described.sort_by(|left, right| left.integration_id.0.cmp(&right.integration_id.0));
        described
    }

    pub fn variable_snapshot(&self) -> Vec<(VariableRef, VariableValue)> {
        self.variables.snapshot()
    }

    /// Seeds the `user` namespace from `values.toml`. A user value is the one kind worth
    /// persisting; everything else is re-derived from its source when an instance starts.
    fn load_user_values(&self) {
        match values::load(&self.values_path) {
            Ok(values) => {
                for value in values {
                    self.variables
                        .set(VariableRef::user(&value.name), value.as_variable());
                }
            }
            Err(error) => warn!(%error, "unable to read user values"),
        }
    }

    pub fn user_values(&self) -> Vec<UserValue> {
        values::load(&self.values_path).unwrap_or_default()
    }

    pub fn set_user_value(&self, value: UserValue) -> Result<(), String> {
        if value.name.trim().is_empty() {
            return Err("a value needs a name".to_string());
        }
        let mut values = self.user_values();
        let is_new = match values
            .iter_mut()
            .find(|existing| existing.name == value.name)
        {
            Some(existing) => {
                *existing = value.clone();
                false
            }
            None => {
                values.push(value.clone());
                true
            }
        };
        values.sort_by(|left, right| left.name.cmp(&right.name));
        values::save(&self.values_path, values).map_err(|error| error.to_string())?;
        self.publish_user_value(&value.name, value.as_variable());
        if is_new {
            // The set of user values changed, not just one reading.
            self.surfaces.emit_event(ServerEvent::Changed);
        }
        Ok(())
    }

    pub fn remove_user_value(&self, name: &str) -> Result<(), String> {
        let mut values = self.user_values();
        let before = values.len();
        values.retain(|existing| existing.name != name);
        if values.len() == before {
            return Err(format!("{name} was not found"));
        }
        values::save(&self.values_path, values).map_err(|error| error.to_string())?;
        let reference = VariableRef::user(name);
        self.variables.clear_one(&reference);
        let targets = self.index.read().unwrap().targets_for_variable(&reference);
        self.mark_dirty(targets);
        self.surfaces.emit_event(ServerEvent::Changed);
        Ok(())
    }

    fn publish_user_value(&self, name: &str, value: VariableValue) {
        self.publish(VariableRef::user(name), value);
    }

    pub fn export_instance(
        &self,
        integration_id: &IntegrationId,
    ) -> Option<Result<String, String>> {
        let instances = self.instances.read().unwrap();
        let instance = instances.get(integration_id)?;
        let manifest = manifest_for(&instance.identity.plugin_type)?;
        Some(export_document(
            integration_id,
            &instance.document,
            &manifest,
        ))
    }

    /// What every running instance currently recommends, already rewritten to name real instances.
    pub fn presets(&self) -> Vec<(IntegrationId, Vec<Preset>)> {
        self.presets.snapshot()
    }

    pub async fn lookup(
        &self,
        integration_id: &IntegrationId,
        source: &str,
        query: &str,
    ) -> Result<Vec<LookupOption>, PluginError> {
        let plugin = self.plugin(integration_id).ok_or_else(|| {
            PluginError::Configuration(format!("{} is not running", integration_id.0))
        })?;
        plugin.lookup(source, query).await
    }

    /// Everything a `$(...)` reference could name, for the editor's autocomplete: what is already
    /// published, plus what each running instance says it could publish. The second half matters
    /// because a plugin only publishes what something already watches, so a fresh installation
    /// would otherwise suggest nothing at all.
    ///
    /// Only the first page is returned, so which suggestions survive is the whole feature: see
    /// [`interleave`]. Narrowing to one instance lifts that ceiling, because there is nothing left
    /// to crowd out and browsing one installation's entities is the point of asking.
    pub async fn suggest_references(
        &self,
        query: &str,
        instance: Option<&IntegrationId>,
    ) -> Vec<LookupOption> {
        let needle = query.trim().to_lowercase();
        let matches =
            |haystack: &str| needle.is_empty() || haystack.to_lowercase().contains(&needle);
        let wanted =
            |integration_id: &IntegrationId| instance.is_none_or(|only| only == integration_id);

        let mut live: Vec<LookupOption> = self
            .variables
            .snapshot()
            .into_iter()
            .filter(|(reference, _)| wanted(&reference.integration_id))
            .filter(|(reference, _)| {
                matches(&reference.name) || matches(&reference.integration_id.0)
            })
            .map(|(reference, value)| {
                LookupOption::new(
                    format!("$({}:{})", reference.integration_id.0, reference.name),
                    reference.name.clone(),
                )
                .group(reference.integration_id.0)
                .preview(value.to_string())
            })
            .collect();
        live.sort_by(|left, right| left.value.cmp(&right.value));

        let mut sources = vec![live];
        let instances: Vec<IntegrationId> = self
            .instances
            .read()
            .unwrap()
            .keys()
            .filter(|integration_id| wanted(integration_id))
            .cloned()
            .collect();
        for integration_id in instances {
            let Some(plugin) = self.plugin(&integration_id) else {
                continue;
            };
            let Ok(offered) = plugin.lookup(SUGGESTION_SOURCE, &needle).await else {
                continue;
            };
            // Kept in the order the plugin gave them: it ranked them by how well each answers the
            // query, and it is the only thing here that knows how.
            sources.push(
                offered
                    .into_iter()
                    .map(|option| LookupOption {
                        value: format!("$({}:{})", integration_id.0, option.value),
                        group: Some(integration_id.0.clone()),
                        ..option
                    })
                    .collect(),
            );
        }

        interleave(
            sources,
            if instance.is_none() {
                SUGGESTION_LIMIT
            } else {
                SCOPED_SUGGESTION_LIMIT
            },
        )
    }

    pub async fn invoke(
        &self,
        integration_id: &IntegrationId,
        action_name: &str,
        parameters: &JsonValue,
    ) -> Result<(), PluginError> {
        let plugin = self.plugin(integration_id).ok_or_else(|| {
            PluginError::Configuration(format!("{} is not running", integration_id.0))
        })?;
        plugin.invoke(action_name, parameters).await
    }

    fn plugin(&self, integration_id: &IntegrationId) -> Option<Arc<dyn Plugin>> {
        self.instances
            .read()
            .unwrap()
            .get(integration_id)
            .and_then(|instance| instance.plugin.clone())
    }

    async fn load_instances(&self) {
        let files = match self.directory.list() {
            Ok(files) => files,
            Err(error) => {
                warn!(%error, "unable to read the plugin directory");
                return;
            }
        };
        for file in files {
            match file {
                InstanceFile::Loaded { identity, document } => {
                    self.start_instance(identity, document).await
                }
                InstanceFile::Invalid { file_name, reason } => {
                    warn!(file_name, reason, "ignoring an unreadable plugin instance")
                }
            }
        }
    }

    async fn start_instance(&self, identity: InstanceIdentity, document: InstanceDocument) {
        let integration_id = identity.integration_id();
        if !document.enabled {
            self.record_instance(RunningInstance {
                identity,
                document,
                status: PluginInstanceStatus::Disabled,
                plugin: None,
                cancel: None,
            });
            return;
        }

        let Some(factory) = registry()
            .iter()
            .find(|factory| factory.plugin_type == identity.plugin_type)
        else {
            let reason = format!(
                "{} is not a plugin type this daemon knows",
                identity.plugin_type
            );
            warn!(
                integration_id = integration_id.0,
                reason, "plugin instance did not start"
            );
            self.record_instance(RunningInstance {
                identity,
                document,
                status: PluginInstanceStatus::Error { reason },
                plugin: None,
                cancel: None,
            });
            return;
        };

        let (cancel_handle, cancel_token) = cancellation();
        let context = PluginContext::new(
            integration_id.clone(),
            self.variables.clone(),
            self.presets.clone(),
            self.signals.clone(),
            cancel_token,
            self.http.clone(),
        )
        .with_assets(self.assets.clone());
        let config = InstanceConfig {
            integration_id: integration_id.clone(),
            values: document.config.clone(),
        };

        match (factory.start)(config, context).await {
            Ok(plugin) => {
                info!(integration_id = integration_id.0, "plugin instance started");
                self.record_instance(RunningInstance {
                    identity,
                    document,
                    status: PluginInstanceStatus::Running,
                    plugin: Some(plugin),
                    cancel: Some(cancel_handle),
                });
            }
            Err(error) => {
                let reason = error.to_string();
                warn!(
                    integration_id = integration_id.0,
                    reason, "plugin instance did not start"
                );
                cancel_handle.cancel();
                self.record_instance(RunningInstance {
                    identity,
                    document,
                    status: PluginInstanceStatus::Error { reason },
                    plugin: None,
                    cancel: None,
                });
            }
        }
    }

    pub fn describe_instance(&self, integration_id: &IntegrationId) -> Option<PluginInstance> {
        self.instances
            .read()
            .unwrap()
            .get(integration_id)
            .map(RunningInstance::describe)
    }

    pub async fn create_instance(
        &self,
        plugin_type: &str,
        name: &str,
        display_name: Option<String>,
        config: toml::Table,
    ) -> Result<PluginInstance, String> {
        let identity = parse_instance_stem(&format!("{plugin_type}.{name}"))?;
        if !registry()
            .iter()
            .any(|factory| factory.plugin_type == identity.plugin_type)
        {
            return Err(format!(
                "{plugin_type} is not a plugin type this daemon knows"
            ));
        }
        let integration_id = identity.integration_id();
        if self.instances.read().unwrap().contains_key(&integration_id) {
            return Err(format!("{} already exists", integration_id.0));
        }
        let document = InstanceDocument {
            version: INSTANCE_DOCUMENT_VERSION,
            enabled: true,
            display_name,
            config,
        };
        self.write_and_restart(identity, document).await
    }

    pub async fn update_instance(
        &self,
        integration_id: &IntegrationId,
        is_enabled: Option<bool>,
        display_name: Option<String>,
        config: Option<toml::Table>,
    ) -> Result<PluginInstance, String> {
        let (identity, mut document) = {
            let instances = self.instances.read().unwrap();
            let instance = instances
                .get(integration_id)
                .ok_or_else(|| format!("{} was not found", integration_id.0))?;
            (instance.identity.clone(), instance.document.clone())
        };
        if let Some(is_enabled) = is_enabled {
            document.enabled = is_enabled;
        }
        if let Some(display_name) = display_name {
            document.display_name = Some(display_name);
        }
        if let Some(mut config) = config {
            // The browser is never sent a stored secret, so an unchanged one comes back absent
            // rather than unchanged. Carrying it over is what makes "leave blank to keep" true.
            for key in secret_keys(&identity.plugin_type) {
                if let (false, Some(existing)) =
                    (config.contains_key(&key), document.config.get(&key))
                {
                    config.insert(key, existing.clone());
                }
            }
            document.config = config;
        }
        self.write_and_restart(identity, document).await
    }

    pub async fn delete_instance(&self, integration_id: &IntegrationId) -> Result<(), String> {
        let identity = {
            let instances = self.instances.read().unwrap();
            instances
                .get(integration_id)
                .ok_or_else(|| format!("{} was not found", integration_id.0))?
                .identity
                .clone()
        };
        self.stop_instance(integration_id).await;
        self.instances.write().unwrap().remove(integration_id);
        self.directory
            .delete(&identity)
            .map_err(|error| error.to_string())?;
        self.rebuild_index().await;
        self.surfaces.emit_event(ServerEvent::Changed);
        Ok(())
    }

    /// Configuration changes replace the instance rather than reconfiguring it: a plugin still
    /// holding a connection opened under old credentials is harder to reason about than one that
    /// starts fresh.
    async fn write_and_restart(
        &self,
        identity: InstanceIdentity,
        document: InstanceDocument,
    ) -> Result<PluginInstance, String> {
        let integration_id = identity.integration_id();
        self.directory
            .save(&identity, &document)
            .map_err(|error| error.to_string())?;
        self.stop_instance(&integration_id).await;
        self.start_instance(identity, document).await;
        self.rebuild_index().await;
        self.surfaces.emit_event(ServerEvent::Changed);
        self.describe_instance(&integration_id)
            .ok_or_else(|| format!("{} did not come back up", integration_id.0))
    }

    async fn stop_instance(&self, integration_id: &IntegrationId) {
        let (plugin, cancel) = {
            let mut instances = self.instances.write().unwrap();
            match instances.get_mut(integration_id) {
                Some(instance) => (instance.plugin.take(), instance.cancel.take()),
                None => (None, None),
            }
        };
        if let Some(cancel) = cancel {
            cancel.cancel();
        }
        if let Some(plugin) = plugin {
            plugin.shutdown().await;
        }
        // A stopped instance's recommendations have to leave the picker with it.
        if self.presets.clear_instance(integration_id) {
            self.surfaces.emit_event(ServerEvent::PresetsChanged {
                integration_id: integration_id.clone(),
            });
        }
        let stale = self.variables.clear_instance(integration_id);
        let targets = {
            let index = self.index.read().unwrap();
            stale
                .iter()
                .flat_map(|reference| index.targets_for_variable(reference))
                .collect()
        };
        self.mark_dirty(targets);
    }

    fn record_instance(&self, instance: RunningInstance) {
        self.instances
            .write()
            .unwrap()
            .insert(instance.identity.integration_id(), instance);
    }

    /// Rebuilt whenever the set of visible controls changes, which is also when the daemon already
    /// repaints a panel.
    async fn rebuild_index(&self) {
        let active_panels = self.active_panels();
        let index = DependencyIndex::build(&active_panels);
        let watched = index.watched_integrations();
        *self.index.write().unwrap() = index;

        for integration_id in &watched {
            let Some(plugin) = self.plugin(integration_id) else {
                continue;
            };
            let subscriptions = self
                .index
                .read()
                .unwrap()
                .subscriptions_for(integration_id)
                .to_vec();
            if let Err(error) = plugin.subscribe(&subscriptions).await {
                warn!(
                    integration_id = integration_id.0,
                    %error, "plugin rejected its subscriptions"
                );
            }
        }
    }

    fn active_panels(&self) -> Vec<(SurfaceId, Panel)> {
        self.surfaces
            .managed_surfaces()
            .into_iter()
            .filter(|device| device.is_enabled)
            .flat_map(|device| {
                self.surfaces
                    .presentation_panels(&device.surface_id)
                    .into_iter()
                    .map(move |panel| (device.surface_id.clone(), panel))
            })
            .collect()
    }

    fn mark_dirty(&self, targets: Vec<RenderTarget>) {
        if targets.is_empty() {
            return;
        }
        self.dirty.lock().unwrap().extend(targets);
        self.dirty_notify.notify_one();
    }

    fn flush_dirty(&self) {
        let targets: Vec<_> = self.dirty.lock().unwrap().drain().collect();
        for target in targets {
            self.surfaces
                .refresh_key(&target.surface_id, target.key_index);
        }
    }

    fn set_user_variable(&self, name: &str, value: &JsonValue) {
        self.publish(VariableRef::user(name), variable_value(value));
    }

    /// The one way a value reaches both consumers. Repaints the keys that read it and tells the
    /// web, so a browser cannot drift from what a device is showing. Everything that publishes a
    /// value goes through here; a path that only marked keys dirty would update the hardware and
    /// leave the UI stale.
    fn publish(&self, reference: VariableRef, value: VariableValue) {
        if !self.variables.set(reference.clone(), value) {
            return;
        }
        let targets = self.index.read().unwrap().targets_for_variable(&reference);
        self.mark_dirty(targets);
        let rendered = self.variables.text(&reference).unwrap_or_default();
        self.surfaces.emit_event(ServerEvent::VariableChanged {
            integration_id: reference.integration_id,
            name: reference.name,
            rendered,
        });
    }

    async fn handle_input(self: &Arc<Self>, event: InputEvent) {
        let InputEvent::Key {
            surface_id,
            is_pressed,
            control,
            ..
        } = event;
        let Some(control) = control else {
            return;
        };
        if is_pressed {
            self.schedule_holds(&surface_id, &control);
            self.fire(&surface_id, &control, &ActionTrigger::Press);
        } else {
            self.cancel_holds(&surface_id, &control.control_id);
            self.fire(&surface_id, &control, &ActionTrigger::Release);
        }
    }

    fn fire(self: &Arc<Self>, surface_id: &SurfaceId, control: &Control, trigger: &ActionTrigger) {
        for binding in control
            .action_bindings
            .iter()
            .filter(|binding| binding.gesture == *trigger)
        {
            let engine = self.clone();
            let surface_id = surface_id.clone();
            let actions = binding.actions.clone();
            let anchor = control.position.clone();
            tokio::spawn(async move { engine.run_actions(surface_id, actions, Some(anchor)).await });
        }
    }

    fn schedule_holds(self: &Arc<Self>, surface_id: &SurfaceId, control: &Control) {
        let timers: Vec<_> = control
            .action_bindings
            .iter()
            .filter_map(|binding| match binding.gesture {
                ActionTrigger::Hold { duration_ms } => Some((duration_ms, binding.actions.clone())),
                _ => None,
            })
            .map(|(duration_ms, actions)| {
                let engine = self.clone();
                let surface_id = surface_id.clone();
                let anchor = control.position.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(duration_ms)).await;
                    engine.run_actions(surface_id, actions, Some(anchor)).await;
                })
            })
            .collect();
        if timers.is_empty() {
            return;
        }
        self.hold_timers
            .lock()
            .unwrap()
            .insert((surface_id.clone(), control.control_id.clone()), timers);
    }

    fn cancel_holds(&self, surface_id: &SurfaceId, control_id: &ControlId) {
        let timers = self
            .hold_timers
            .lock()
            .unwrap()
            .remove(&(surface_id.clone(), control_id.clone()));
        for timer in timers.into_iter().flatten() {
            timer.abort();
        }
    }

    async fn run_actions(
        &self,
        surface_id: SurfaceId,
        actions: Vec<Action>,
        anchor: Option<SurfacePosition>,
    ) {
        for action in actions {
            match action {
                Action::InvokeIntegration {
                    integration_id,
                    action_name,
                    parameters,
                } => {
                    if let Err(error) = self
                        .invoke(&integration_id, &action_name, &parameters)
                        .await
                    {
                        self.surfaces.log(
                            &surface_id,
                            SurfaceLogLevel::Warning,
                            format!("{}: {action_name} failed: {error}", integration_id.0),
                        );
                    }
                }
                Action::SetVariable {
                    variable_name,
                    value,
                } => self.set_user_variable(&variable_name, &value),
                Action::ChangePanel { panel_id } => {
                    if let Err(reason) = self
                        .surfaces
                        .assign_active_panel(&surface_id.0, &panel_id.0)
                    {
                        self.surfaces.log(
                            &surface_id,
                            SurfaceLogLevel::Warning,
                            format!("could not switch to panel {}: {reason}", panel_id.0),
                        );
                    }
                }
                Action::OpenSubpanel {
                    panel_id,
                    placement,
                    offset_columns,
                    offset_rows,
                } => {
                    let Some(anchor) = anchor.clone() else {
                        continue;
                    };
                    if let Err(reason) = self.surfaces.open_subpanel(
                        &surface_id,
                        &panel_id.0,
                        anchor,
                        placement,
                        offset_columns,
                        offset_rows,
                    ) {
                        self.surfaces.log(
                            &surface_id,
                            SurfaceLogLevel::Warning,
                            format!("unable to open subpanel {}: {reason}", panel_id.0),
                        );
                    }
                }
                Action::CloseSubpanel => {
                    self.surfaces.close_subpanel(&surface_id);
                }
                Action::Wait { duration_ms } => {
                    tokio::time::sleep(Duration::from_millis(duration_ms)).await
                }
            }
        }
    }
}

fn secret_keys(plugin_type: &str) -> Vec<String> {
    manifest_for(plugin_type)
        .map(|manifest| {
            manifest
                .config_schema
                .iter()
                .filter(|field| field.is_secret())
                .map(|field| field.key.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn manifest_for(plugin_type: &str) -> Option<PluginManifest> {
    registry()
        .iter()
        .find(|factory| factory.plugin_type == plugin_type)
        .map(|factory| (factory.manifest)())
}

fn variable_value(value: &JsonValue) -> VariableValue {
    match value {
        JsonValue::Bool(value) => VariableValue::Boolean(*value),
        JsonValue::Number(value) => VariableValue::Number(value.as_f64().unwrap_or_default()),
        JsonValue::String(value) => VariableValue::Text(value.clone()),
        other => VariableValue::Text(other.to_string()),
    }
}

async fn run_signals(engine: Arc<PluginEngine>, mut signals: mpsc::Receiver<EngineSignal>) {
    while let Some(signal) = signals.recv().await {
        match signal {
            EngineSignal::VariableChanged(reference) => {
                let targets = engine
                    .index
                    .read()
                    .unwrap()
                    .targets_for_variable(&reference);
                engine.mark_dirty(targets);
                let rendered = engine.variables.text(&reference).unwrap_or_default();
                engine.surfaces.emit_event(ServerEvent::VariableChanged {
                    integration_id: reference.integration_id,
                    name: reference.name,
                    rendered,
                });
            }
            EngineSignal::PresetsChanged(integration_id) => engine
                .surfaces
                .emit_event(ServerEvent::PresetsChanged { integration_id }),
            EngineSignal::InstanceLog {
                integration_id,
                level,
                message,
            } => debug!(
                integration_id = integration_id.0,
                ?level,
                message,
                "plugin instance log"
            ),
            EngineSignal::InventoryChanged => engine.rebuild_index().await,
        }
    }
}

async fn run_input(engine: Arc<PluginEngine>, mut input: mpsc::Receiver<InputEvent>) {
    while let Some(event) = input.recv().await {
        engine.handle_input(event).await;
    }
}

async fn run_flush(engine: Arc<PluginEngine>) {
    loop {
        engine.dirty_notify.notified().await;
        tokio::time::sleep(COALESCE_WINDOW).await;
        engine.flush_dirty();
    }
}

/// An image that was not on disk when a key was drawn arrives later. Which key wanted it is not
/// worth tracking: repainting everything is cheap, because the render ledger drops every key whose
/// resolution did not actually change, and this fires once per newly-seen URL.
async fn watch_assets(engine: Arc<PluginEngine>, mut ready: mpsc::Receiver<String>) {
    while let Some(asset) = ready.recv().await {
        engine.surfaces.forget_renderings();
        engine
            .surfaces
            .emit_event(ServerEvent::AssetReady { asset });
        let targets = engine.index.read().unwrap().every_target();
        engine.mark_dirty(targets);
    }
}

/// The registry already broadcasts `Changed` from every call site that alters which controls are
/// visible, so the index follows it rather than adding hooks to each one.
async fn watch_inventory(engine: Arc<PluginEngine>) {
    let mut events = engine.surfaces.subscribe();
    loop {
        match events.recv().await {
            Ok(ServerEvent::Changed) => {
                let _ = engine.signals.try_send(EngineSignal::InventoryChanged);
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                let _ = engine.signals.try_send(EngineSignal::InventoryChanged);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(values: &[&str]) -> Vec<LookupOption> {
        values
            .iter()
            .map(|value| LookupOption::new(*value, *value))
            .collect()
    }

    fn values(options: Vec<LookupOption>) -> Vec<String> {
        options.into_iter().map(|option| option.value).collect()
    }

    /// The failure this exists to prevent: one instance with hundreds of matches filling the page
    /// so that the entity being searched for never appears.
    #[test]
    fn a_long_source_cannot_crowd_out_a_short_one() {
        let crowded: Vec<&str> = ["a1", "a2", "a3", "a4", "a5", "a6"].to_vec();
        let merged = interleave(vec![options(&crowded), options(&["b1"])], 4);

        assert!(values(merged).contains(&"b1".to_string()));
    }

    #[test]
    fn each_source_keeps_the_order_it_was_given() {
        let merged = interleave(vec![options(&["a1", "a2"]), options(&["b1", "b2"])], 10);

        assert_eq!(values(merged), ["a1", "b1", "a2", "b2"]);
    }

    #[test]
    fn an_exhausted_source_does_not_stall_the_rest() {
        let merged = interleave(vec![options(&["a1"]), options(&["b1", "b2", "b3"])], 10);

        assert_eq!(values(merged), ["a1", "b1", "b2", "b3"]);
    }

    /// A live value and the same reference offered by its plugin are one suggestion, not two.
    #[test]
    fn the_same_reference_from_two_sources_appears_once() {
        let merged = interleave(vec![options(&["same"]), options(&["same", "other"])], 10);

        assert_eq!(values(merged), ["same", "other"]);
    }

    #[test]
    fn nothing_to_offer_is_not_an_endless_loop() {
        assert!(interleave(vec![Vec::new(), Vec::new()], 10).is_empty());
        assert!(interleave(Vec::new(), 10).is_empty());
    }
}
