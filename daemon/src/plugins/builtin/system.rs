use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use chrono::Local;
use serde_json::{json, Value as JsonValue};
use tokio::task::JoinHandle;

use crate::{
    bindings::action::{Action, ActionBinding, ActionTrigger},
    identifiers::{AssetId, IntegrationId},
    panels::{
        control::ControlTemplate,
        rendered_state::{Anchor9, Fit, Layer, RenderedState, RgbaColor},
    },
    plugins::{
        instance::InstanceConfig,
        manifest::{
            ActionDefinition, ConfigField, PluginManifest, VariableDefinition, VariableKind,
        },
        plugin::{Plugin, PluginContext, PluginError, PluginFactory},
        preset::Preset,
    },
    variables::VariableValue,
};

const CLOCK_VALUE: &str = "clock";
const TIMER_VALUE: &str = "timer";
const START_TIMER: &str = "start_timer";

pub const FACTORY: PluginFactory = PluginFactory {
    plugin_type: "system",
    manifest,
    start: |config, context| Box::pin(start(config, context)),
};

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_type: "system",
        display_name: "System",
        description: "Local clock and countdown timers.",
        config_schema: Vec::new(),
        actions: vec![ActionDefinition::new(START_TIMER)
            .label("Start countdown")
            .parameters(vec![ConfigField::number("duration_seconds")
                .label("Duration (seconds)")
                .required()])],
        variables: vec![
            VariableDefinition::new(CLOCK_VALUE, VariableKind::Text)
                .description("Current local time in 24-hour format."),
            VariableDefinition::new(TIMER_VALUE, VariableKind::Text)
                .description("Time remaining in the active countdown."),
        ],
    }
}

pub async fn start(
    _config: InstanceConfig,
    context: PluginContext,
) -> Result<Arc<dyn Plugin>, PluginError> {
    context.set_presets(presets());
    context.set_value(CLOCK_VALUE, VariableValue::Text(local_time()));
    context.set_value(TIMER_VALUE, VariableValue::Text("00:00".to_string()));
    let plugin = Arc::new(SystemPlugin {
        context: context.clone(),
        timer_task: Mutex::new(None),
        timer_generation: Arc::new(AtomicU64::new(0)),
    });
    tokio::spawn(publish_clock(context));
    Ok(plugin)
}

struct SystemPlugin {
    context: PluginContext,
    timer_task: Mutex<Option<JoinHandle<()>>>,
    timer_generation: Arc<AtomicU64>,
}

#[async_trait]
impl Plugin for SystemPlugin {
    async fn invoke(&self, action_name: &str, parameters: &JsonValue) -> Result<(), PluginError> {
        if action_name != START_TIMER {
            return Err(PluginError::UnknownAction(action_name.to_string()));
        }
        let duration_seconds = parameters
            .get("duration_seconds")
            .and_then(JsonValue::as_u64)
            .filter(|duration_seconds| *duration_seconds > 0)
            .ok_or_else(|| {
                PluginError::Configuration(
                    "duration_seconds must be a positive integer".to_string(),
                )
            })?;
        self.start_timer(Duration::from_secs(duration_seconds));
        Ok(())
    }

    async fn shutdown(&self) {
        if let Some(task) = self.timer_task.lock().unwrap().take() {
            task.abort();
        }
    }
}

impl SystemPlugin {
    fn start_timer(&self, duration: Duration) {
        if let Some(task) = self.timer_task.lock().unwrap().take() {
            task.abort();
        }
        let generation = self.timer_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.context
            .set_value(TIMER_VALUE, VariableValue::Text(format_remaining(duration)));
        let context = self.context.clone();
        let timer_generation = self.timer_generation.clone();
        let task = tokio::spawn(async move {
            let deadline = Instant::now() + duration;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if timer_generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                context.set_value(
                    TIMER_VALUE,
                    VariableValue::Text(format_remaining(remaining)),
                );
                if remaining.is_zero() {
                    return;
                }
                tokio::select! {
                    _ = context.cancel.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                }
            }
        });
        *self.timer_task.lock().unwrap() = Some(task);
    }
}

async fn publish_clock(context: PluginContext) {
    loop {
        context.set_value(CLOCK_VALUE, VariableValue::Text(local_time()));
        tokio::select! {
            _ = context.cancel.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
    }
}

fn local_time() -> String {
    Local::now().format("%H:%M").to_string()
}

fn format_remaining(duration: Duration) -> String {
    let total_seconds = duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() > 0));
    format!("{:02}:{:02}", total_seconds / 60, total_seconds % 60)
}

fn presets() -> Vec<Preset> {
    let mut presets = vec![Preset {
        preset_id: "clock".to_string(),
        category: "Time".to_string(),
        name: "Clock".to_string(),
        description: Some("Shows the current local time.".to_string()),
        control: ControlTemplate {
            name: "Clock".to_string(),
            default_state: face("mdi:clock-outline", "$(self:clock)"),
            pressed_state: None,
            action_bindings: Vec::new(),
        },
    }];
    for duration_minutes in [5, 10, 15] {
        presets.push(timer_preset(duration_minutes));
    }
    presets
}

fn timer_preset(duration_minutes: u64) -> Preset {
    let name = format!("{duration_minutes} minute timer");
    Preset {
        preset_id: format!("timer-{duration_minutes}-minutes"),
        category: "Time".to_string(),
        name: name.clone(),
        description: Some(format!(
            "Starts or resets a {duration_minutes}-minute countdown."
        )),
        control: ControlTemplate {
            name,
            default_state: face("mdi:timer-sand", "$(self:timer)"),
            pressed_state: None,
            action_bindings: vec![ActionBinding {
                gesture: ActionTrigger::Press,
                actions: vec![Action::InvokeIntegration {
                    integration_id: IntegrationId("self".to_string()),
                    action_name: START_TIMER.to_string(),
                    parameters: json!({ "duration_seconds": duration_minutes * 60 }),
                }],
            }],
        },
    }
}

fn face(icon: &str, text: &str) -> RenderedState {
    RenderedState {
        layers: vec![
            Layer::Fill {
                color: RgbaColor::opaque(0, 0, 0).into(),
            },
            Layer::Image {
                image: AssetId(icon.to_string()),
                fit: Fit::Contain,
                anchor: Anchor9::TopCenter,
                scale_percent: 50,
                tint: None,
            },
            Layer::Text {
                text: text.to_string(),
                color: RgbaColor::opaque(255, 255, 255).into(),
                anchor: Anchor9::BottomCenter,
                font_family: None,
                font_size: None,
            },
        ],
        is_pressed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_offer_a_clock_and_requested_timer_durations() {
        let offered = presets();
        assert_eq!(
            offered
                .iter()
                .map(|preset| preset.preset_id.as_str())
                .collect::<Vec<_>>(),
            [
                "clock",
                "timer-5-minutes",
                "timer-10-minutes",
                "timer-15-minutes"
            ]
        );
    }

    #[test]
    fn remaining_time_rounds_up_to_the_next_second() {
        assert_eq!(format_remaining(Duration::from_secs(300)), "05:00");
        assert_eq!(format_remaining(Duration::from_millis(1)), "00:01");
    }
}
