use std::{fmt, future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, watch};
use tracing::warn;

use crate::{
    assets::AssetStore,
    identifiers::IntegrationId,
    plugins::{engine::EngineSignal, instance::InstanceConfig, manifest::PluginManifest},
    surfaces::logs::SurfaceLogLevel,
    variables::{template, VariableRef, VariableStore, VariableValue},
};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What a plugin instance can be asked to do.
///
/// A plugin is push-only for values: when its view of the world moves it calls `set_value`, and
/// the engine works out what that repaints. It is never asked to compute anything during a render,
/// which is what keeps the render path free of I/O.
///
/// The boundary is message-shaped on purpose: an action is a name and some JSON, a value is a name
/// and a scalar, and everything a plugin publishes leaves through [`PluginContext`] rather than
/// through shared state. Nothing here assumes the implementation is in-process.
#[async_trait]
pub trait Plugin: Send + Sync {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue) -> Result<(), PluginError>;

    /// The full current set of what anything on screen is watching, not a delta. Implementations
    /// replace rather than merge.
    async fn subscribe(&self, _subscriptions: &[Subscription]) -> Result<(), PluginError> {
        Ok(())
    }

    /// Options for a `ConfigFieldKind::Lookup` field, and the reference suggestions behind the
    /// editor's autocomplete. Answered from what the instance already knows, so it must not go to
    /// the network.
    ///
    /// `query` is what the user has typed. Filtering here rather than in the browser is what keeps
    /// this usable against an installation with thousands of entities.
    async fn lookup(&self, _source: &str, _query: &str) -> Result<Vec<LookupOption>, PluginError> {
        Ok(Vec::new())
    }

    async fn shutdown(&self) {}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginError {
    Configuration(String),
    UnknownAction(String),
    Upstream(String),
    NotImplemented,
}

impl fmt::Display for PluginError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(reason) => write!(formatter, "configuration is invalid: {reason}"),
            Self::UnknownAction(name) => write!(formatter, "unknown action {name}"),
            Self::Upstream(reason) => write!(formatter, "{reason}"),
            Self::NotImplemented => formatter.write_str("not implemented yet"),
        }
    }
}

/// A value name something on screen is watching. Free-form, so `hass` receives
/// `light.kitchen.color` and knows exactly which entity and attribute that means; nothing in the
/// daemon parses these.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscription {
    pub name: String,
}

/// One choice for a lookup field. `group` is what the UI sorts and labels by — a domain, a device
/// type, whatever the plugin's own vocabulary is.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LookupOption {
    pub value: String,
    pub label: String,
    pub group: Option<String>,
}

pub struct PluginFactory {
    pub plugin_type: &'static str,
    pub manifest: fn() -> PluginManifest,
    pub start: fn(
        InstanceConfig,
        PluginContext,
    ) -> BoxFuture<'static, Result<Arc<dyn Plugin>, PluginError>>,
}

/// Everything a running instance is given: its identity, the sinks it publishes through, and the
/// token that ends it.
#[derive(Clone)]
pub struct PluginContext {
    pub integration_id: IntegrationId,
    pub cancel: CancelToken,
    /// Shared across every instance so connection pools and DNS caching are not duplicated.
    pub http: reqwest::Client,
    /// Absent only in tests that never publish an image; `set_image` says so rather than panicking.
    assets: Option<Arc<AssetStore>>,
    variables: Arc<VariableStore>,
    signals: mpsc::Sender<EngineSignal>,
}

impl PluginContext {
    pub fn new(
        integration_id: IntegrationId,
        variables: Arc<VariableStore>,
        signals: mpsc::Sender<EngineSignal>,
        cancel: CancelToken,
        http: reqwest::Client,
    ) -> Self {
        Self {
            integration_id,
            cancel,
            http,
            assets: None,
            variables,
            signals,
        }
    }

    /// Resolves `$(instance:name)` references against live variables, so an action's parameters
    /// can depend on what other plugins are publishing.
    pub fn interpolate(&self, template: &str) -> String {
        template::interpolate(template, |reference| self.variables.text(reference))
    }

    pub fn with_assets(mut self, assets: Arc<AssetStore>) -> Self {
        self.assets = Some(assets);
        self
    }

    /// Stores bytes and publishes the resulting id as a value, so a key can show them. Identical
    /// bytes produce the same id, which means re-announcing the same artwork repaints nothing.
    pub fn set_image(&self, name: impl Into<String>, bytes: &[u8]) -> Result<(), PluginError> {
        let assets = self
            .assets
            .as_ref()
            .ok_or_else(|| PluginError::Upstream("no asset store is available".to_string()))?;
        let asset = assets
            .insert_bytes(bytes)
            .map_err(|error| PluginError::Upstream(error.to_string()))?;
        self.set_value(name, VariableValue::Image(asset));
        Ok(())
    }

    pub fn set_value(&self, name: impl Into<String>, value: VariableValue) {
        let reference = VariableRef {
            integration_id: self.integration_id.clone(),
            name: name.into(),
        };
        if self.variables.set(reference.clone(), value) {
            self.signal(EngineSignal::VariableChanged(reference));
        }
    }

    pub fn log(&self, level: SurfaceLogLevel, message: impl Into<String>) {
        self.signal(EngineSignal::InstanceLog {
            integration_id: self.integration_id.clone(),
            level,
            message: message.into(),
        });
    }

    fn signal(&self, signal: EngineSignal) {
        if self.signals.try_send(signal).is_err() {
            warn!(
                integration_id = self.integration_id.0,
                "plugin signal queue is full or closed, dropped an update"
            );
        }
    }
}

/// Cooperative shutdown for the tasks an instance spawns. Disabling an instance or editing its
/// configuration cancels the token, awaits `shutdown`, and starts a fresh instance; a task that
/// ignores the token outlives its plugin.
#[derive(Clone)]
pub struct CancelToken {
    receiver: watch::Receiver<bool>,
}

pub struct CancelHandle {
    sender: watch::Sender<bool>,
}

pub fn cancellation() -> (CancelHandle, CancelToken) {
    let (sender, receiver) = watch::channel(false);
    (CancelHandle { sender }, CancelToken { receiver })
}

impl CancelToken {
    pub async fn cancelled(&self) {
        let mut receiver = self.receiver.clone();
        let _ = receiver.wait_for(|cancelled| *cancelled).await;
    }

    pub fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }
}

impl CancelHandle {
    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }
}
