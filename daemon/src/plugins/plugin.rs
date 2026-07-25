use std::{fmt, future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, watch};
use tracing::warn;

use crate::{
    models::{identifiers::IntegrationId, network_surface::SurfaceLogLevel},
    plugins::{
        engine::EngineSignal,
        instance::InstanceConfig,
        manifest::PluginManifest,
        variables::{VariableRef, VariableStore, VariableValue},
    },
};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What a plugin instance can be asked to do.
///
/// The boundary is message-shaped on purpose: an action is a name and some JSON, a feedback is a
/// name and some JSON answered with a bool, and everything a plugin publishes leaves through
/// [`PluginContext`] rather than through shared state. Nothing here assumes the implementation is
/// in-process.
#[async_trait]
pub trait Plugin: Send + Sync {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue) -> Result<(), PluginError>;

    /// Called from the render path, so it must be cheap and must not perform I/O. Answer from the
    /// plugin's own view of the world and call [`PluginContext::invalidate_feedbacks`] when that
    /// view changes.
    async fn evaluate(
        &self,
        feedback_name: &str,
        parameters: &JsonValue,
    ) -> Result<bool, PluginError>;

    /// The full current set of what anything on screen is watching, not a delta. Implementations
    /// replace rather than merge.
    async fn subscribe(&self, _subscriptions: &[Subscription]) -> Result<(), PluginError> {
        Ok(())
    }

    async fn shutdown(&self) {}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginError {
    Configuration(String),
    UnknownAction(String),
    UnknownFeedback(String),
    Upstream(String),
    NotImplemented,
}

impl fmt::Display for PluginError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(reason) => write!(formatter, "configuration is invalid: {reason}"),
            Self::UnknownAction(name) => write!(formatter, "unknown action {name}"),
            Self::UnknownFeedback(name) => write!(formatter, "unknown feedback {name}"),
            Self::Upstream(reason) => write!(formatter, "{reason}"),
            Self::NotImplemented => formatter.write_str("not implemented yet"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Subscription {
    Variable {
        name: String,
    },
    Feedback {
        feedback_name: String,
        parameters: JsonValue,
    },
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
    variables: Arc<VariableStore>,
    signals: mpsc::Sender<EngineSignal>,
}

impl PluginContext {
    pub fn new(
        integration_id: IntegrationId,
        variables: Arc<VariableStore>,
        signals: mpsc::Sender<EngineSignal>,
        cancel: CancelToken,
    ) -> Self {
        Self {
            integration_id,
            cancel,
            variables,
            signals,
        }
    }

    pub fn set_variable(&self, name: impl Into<String>, value: VariableValue) {
        let reference = VariableRef {
            integration_id: self.integration_id.clone(),
            name: name.into(),
        };
        if self.variables.set(reference.clone(), value) {
            self.signal(EngineSignal::VariableChanged(reference));
        }
    }

    /// Tells the engine this instance's view of the world moved, so every feedback anything is
    /// watching should be asked again.
    pub fn invalidate_feedbacks(&self) {
        self.signal(EngineSignal::FeedbacksInvalidated(
            self.integration_id.clone(),
        ));
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
