use std::{
    collections::{HashMap, HashSet},
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
    models::{
        action::{Action, ActionTrigger},
        control::Control,
        identifiers::{ControlId, IntegrationId, SurfaceId},
        network_surface::{ServerEvent, SurfaceLogLevel},
        panel::Panel,
    },
    plugins::{
        config::{export_document, InstanceFile, PluginDirectory},
        feedback::FeedbackCache,
        index::{DependencyIndex, RenderTarget},
        instance::{
            InstanceConfig, InstanceDocument, InstanceIdentity, PluginInstance,
            PluginInstanceStatus,
        },
        manifest::PluginManifest,
        plugin::{cancellation, CancelHandle, Plugin, PluginContext, PluginError},
        registry,
        variables::{VariableRef, VariableStore, VariableValue},
    },
    state::SurfaceRegistry,
};

/// How long the engine waits after the first change before repainting, so a burst of variable
/// updates becomes one repaint rather than one per update.
const COALESCE_WINDOW: Duration = Duration::from_millis(50);
const SIGNAL_QUEUE_SIZE: usize = 1024;
pub const INPUT_QUEUE_SIZE: usize = 256;

/// A gesture that reached the daemon. Deliberately not the `ServerEvent` broadcast, which drops
/// messages when a receiver lags: a dropped render is cosmetic, a dropped action is a light that
/// did not turn on.
#[derive(Clone, Debug)]
pub enum InputEvent {
    Key {
        surface_id: SurfaceId,
        key_index: u8,
        is_pressed: bool,
    },
}

#[derive(Clone, Debug)]
pub enum EngineSignal {
    VariableChanged(VariableRef),
    FeedbacksInvalidated(IntegrationId),
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
        PluginInstance {
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
    feedbacks: Arc<FeedbackCache>,
    directory: PluginDirectory,
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
        feedbacks: Arc<FeedbackCache>,
        directory: PluginDirectory,
        input: mpsc::Receiver<InputEvent>,
    ) -> Arc<Self> {
        let (signals, signal_receiver) = mpsc::channel(SIGNAL_QUEUE_SIZE);
        let engine = Arc::new(Self {
            surfaces,
            variables,
            feedbacks,
            directory,
            instances: RwLock::default(),
            index: RwLock::default(),
            dirty: Mutex::default(),
            dirty_notify: Notify::new(),
            signals,
            hold_timers: Mutex::default(),
        });

        engine.load_instances().await;
        engine.rebuild_index().await;

        tokio::spawn(run_signals(engine.clone(), signal_receiver));
        tokio::spawn(run_input(engine.clone(), input));
        tokio::spawn(run_flush(engine.clone()));
        tokio::spawn(watch_inventory(engine.clone()));

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
            self.signals.clone(),
            cancel_token,
        );
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
        for integration_id in watched {
            self.reevaluate_feedbacks(&integration_id).await;
        }
    }

    fn active_panels(&self) -> Vec<(SurfaceId, Panel)> {
        self.surfaces
            .managed_surfaces()
            .into_iter()
            .filter(|device| device.is_enabled)
            .filter_map(|device| {
                let panel_id = device.active_panel_id?;
                let panel = self.surfaces.panel(&panel_id.0)?;
                Some((device.surface_id, panel))
            })
            .collect()
    }

    async fn reevaluate_feedbacks(&self, integration_id: &IntegrationId) {
        let Some(plugin) = self.plugin(integration_id) else {
            return;
        };
        let keys = self.index.read().unwrap().feedback_keys_for(integration_id);
        let mut dirty = Vec::new();
        for key in keys {
            match plugin.evaluate(&key.feedback_name, &key.parameters()).await {
                Ok(result) => {
                    if self.feedbacks.set(key.clone(), result) {
                        dirty.extend(self.index.read().unwrap().targets_for_feedback(&key));
                    }
                }
                Err(error) => debug!(
                    integration_id = integration_id.0,
                    feedback = key.feedback_name,
                    %error,
                    "feedback could not be evaluated"
                ),
            }
        }
        self.mark_dirty(dirty);
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
        let reference = VariableRef::user(name);
        if self.variables.set(reference.clone(), variable_value(value)) {
            let targets = self.index.read().unwrap().targets_for_variable(&reference);
            self.mark_dirty(targets);
        }
    }

    async fn handle_input(self: &Arc<Self>, event: InputEvent) {
        let InputEvent::Key {
            surface_id,
            key_index,
            is_pressed,
        } = event;
        let Some(control) = self.surfaces.control_at(&surface_id, key_index) else {
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
            tokio::spawn(async move { engine.run_actions(surface_id, actions).await });
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
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(duration_ms)).await;
                    engine.run_actions(surface_id, actions).await;
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

    async fn run_actions(&self, surface_id: SurfaceId, actions: Vec<Action>) {
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
                Action::Wait { duration_ms } => {
                    tokio::time::sleep(Duration::from_millis(duration_ms)).await
                }
            }
        }
    }
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
            }
            EngineSignal::FeedbacksInvalidated(integration_id) => {
                engine.reevaluate_feedbacks(&integration_id).await
            }
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
