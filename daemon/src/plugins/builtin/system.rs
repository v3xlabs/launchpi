use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use chrono::Local;
use serde_json::{json, Value as JsonValue};
use sysinfo::{Disks, System as HostSystem};
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
const DATE_VALUE: &str = "date";
const WEEKDAY_VALUE: &str = "weekday";
const TIMER_VALUE: &str = "timer";
const CPU_USAGE_VALUE: &str = "cpu_usage_pct";
const MEMORY_USED_BYTES_VALUE: &str = "memory_used_bytes";
const MEMORY_TOTAL_BYTES_VALUE: &str = "memory_total_bytes";
const MEMORY_USAGE_VALUE: &str = "memory_usage_pct";
const MEMORY_VALUE: &str = "memory";
const LOAD_AVERAGE_1M_VALUE: &str = "load_average_1m";
const LOAD_AVERAGE_5M_VALUE: &str = "load_average_5m";
const LOAD_AVERAGE_15M_VALUE: &str = "load_average_15m";
const DISK_FREE_BYTES_VALUE: &str = "disk_free_bytes";
const DISK_TOTAL_BYTES_VALUE: &str = "disk_total_bytes";
const DISK_USAGE_VALUE: &str = "disk_usage_pct";
const DISK_FREE_VALUE: &str = "disk_free";
const UPTIME_SECONDS_VALUE: &str = "uptime_seconds";
const UPTIME_VALUE: &str = "uptime";
const START_TIMER: &str = "start_timer";
const TELEMETRY_INTERVAL: Duration = Duration::from_secs(5);

pub const FACTORY: PluginFactory = PluginFactory {
    plugin_type: "system",
    manifest,
    start: |config, context| Box::pin(start(config, context)),
};

fn manifest() -> PluginManifest {
    PluginManifest {
        plugin_type: "system",
        display_name: "System",
        description: "Local clock, timers, and machine telemetry.",
        config_schema: Vec::new(),
        actions: vec![ActionDefinition::new(START_TIMER)
            .label("Start countdown")
            .parameters(vec![ConfigField::number("duration_seconds")
                .label("Duration (seconds)")
                .required()])],
        variables: vec![
            VariableDefinition::new(CLOCK_VALUE, VariableKind::Text)
                .description("Current local time in 24-hour format."),
            VariableDefinition::new(DATE_VALUE, VariableKind::Text)
                .description("Current local date."),
            VariableDefinition::new(WEEKDAY_VALUE, VariableKind::Text)
                .description("Current local weekday."),
            VariableDefinition::new(TIMER_VALUE, VariableKind::Text)
                .description("Time remaining in the active countdown."),
            VariableDefinition::new(CPU_USAGE_VALUE, VariableKind::Number)
                .description("CPU use as a percentage on the machine running Launchpi."),
            VariableDefinition::new(MEMORY_USED_BYTES_VALUE, VariableKind::Number),
            VariableDefinition::new(MEMORY_TOTAL_BYTES_VALUE, VariableKind::Number),
            VariableDefinition::new(MEMORY_USAGE_VALUE, VariableKind::Number)
                .description("Memory use as a percentage."),
            VariableDefinition::new(MEMORY_VALUE, VariableKind::Text)
                .description("Used and total memory."),
            VariableDefinition::new(LOAD_AVERAGE_1M_VALUE, VariableKind::Number),
            VariableDefinition::new(LOAD_AVERAGE_5M_VALUE, VariableKind::Number),
            VariableDefinition::new(LOAD_AVERAGE_15M_VALUE, VariableKind::Number),
            VariableDefinition::new(DISK_FREE_BYTES_VALUE, VariableKind::Number)
                .description("Free bytes on the root filesystem."),
            VariableDefinition::new(DISK_TOTAL_BYTES_VALUE, VariableKind::Number),
            VariableDefinition::new(DISK_USAGE_VALUE, VariableKind::Number)
                .description("Root filesystem use as a percentage."),
            VariableDefinition::new(DISK_FREE_VALUE, VariableKind::Text)
                .description("Free and total space on the root filesystem."),
            VariableDefinition::new(UPTIME_SECONDS_VALUE, VariableKind::Number),
            VariableDefinition::new(UPTIME_VALUE, VariableKind::Text)
                .description("Time since the machine started."),
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
    tokio::spawn(publish_telemetry(plugin.context.clone()));
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
        context.set_value(DATE_VALUE, VariableValue::Text(local_date()));
        context.set_value(WEEKDAY_VALUE, VariableValue::Text(local_weekday()));
        tokio::select! {
            _ = context.cancel.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
    }
}

async fn publish_telemetry(context: PluginContext) {
    let mut system = HostSystem::new();
    let mut disks = Disks::new_with_refreshed_list();
    loop {
        system.refresh_cpu_usage();
        system.refresh_memory();
        disks.refresh(false);
        publish_metrics(&context, &system, &disks);
        tokio::select! {
            _ = context.cancel.cancelled() => return,
            _ = tokio::time::sleep(TELEMETRY_INTERVAL) => {}
        }
    }
}

fn publish_metrics(context: &PluginContext, system: &HostSystem, disks: &Disks) {
    let memory_used = system.used_memory();
    let memory_total = system.total_memory();
    let memory_usage = percentage(memory_used, memory_total);
    let load = HostSystem::load_average();
    let uptime = HostSystem::uptime();
    let disk = disks
        .list()
        .iter()
        .find(|disk| disk.mount_point() == Path::new("/"));
    let (disk_free, disk_total) = disk
        .map(|disk| (disk.available_space(), disk.total_space()))
        .unwrap_or_default();

    context.set_value(
        CPU_USAGE_VALUE,
        VariableValue::Number(f64::from(system.global_cpu_usage())),
    );
    context.set_value(
        MEMORY_USED_BYTES_VALUE,
        VariableValue::Number(memory_used as f64),
    );
    context.set_value(
        MEMORY_TOTAL_BYTES_VALUE,
        VariableValue::Number(memory_total as f64),
    );
    context.set_value(MEMORY_USAGE_VALUE, VariableValue::Number(memory_usage));
    context.set_value(
        MEMORY_VALUE,
        VariableValue::Text(format!(
            "{} / {}",
            format_bytes(memory_used),
            format_bytes(memory_total)
        )),
    );
    context.set_value(LOAD_AVERAGE_1M_VALUE, VariableValue::Number(load.one));
    context.set_value(LOAD_AVERAGE_5M_VALUE, VariableValue::Number(load.five));
    context.set_value(LOAD_AVERAGE_15M_VALUE, VariableValue::Number(load.fifteen));
    context.set_value(
        DISK_FREE_BYTES_VALUE,
        VariableValue::Number(disk_free as f64),
    );
    context.set_value(
        DISK_TOTAL_BYTES_VALUE,
        VariableValue::Number(disk_total as f64),
    );
    context.set_value(
        DISK_USAGE_VALUE,
        VariableValue::Number(percentage(disk_total.saturating_sub(disk_free), disk_total)),
    );
    context.set_value(
        DISK_FREE_VALUE,
        VariableValue::Text(format!(
            "{} free of {}",
            format_bytes(disk_free),
            format_bytes(disk_total)
        )),
    );
    context.set_value(UPTIME_SECONDS_VALUE, VariableValue::Number(uptime as f64));
    context.set_value(UPTIME_VALUE, VariableValue::Text(format_uptime(uptime)));
}

fn local_time() -> String {
    Local::now().format("%H:%M").to_string()
}

fn local_date() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn local_weekday() -> String {
    Local::now().format("%A").to_string()
}

fn percentage(value: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (value as f64 * 100.0 / total as f64).round()
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = seconds / 3_600 % 24;
    let minutes = seconds / 60 % 60;
    match days {
        0 => format!("{hours:02}:{minutes:02}"),
        _ => format!("{days}d {hours:02}:{minutes:02}"),
    }
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
    presets.extend([
        readout_preset("date", "Time", "Date", "mdi:calendar", "$(self:date)"),
        readout_preset(
            "weekday",
            "Time",
            "Weekday",
            "mdi:calendar-week",
            "$(self:weekday)",
        ),
    ]);
    for duration_minutes in [5, 10, 15] {
        presets.push(timer_preset(duration_minutes));
    }
    presets.extend([
        readout_preset(
            "cpu-usage",
            "Machine",
            "CPU usage",
            "mdi:cpu-64-bit",
            "CPU\n$(self:cpu_usage_pct)%",
        ),
        readout_preset(
            "memory-usage",
            "Machine",
            "Memory usage",
            "mdi:memory",
            "Memory\n$(self:memory_usage_pct)%",
        ),
        readout_preset(
            "load-average",
            "Machine",
            "Load average",
            "mdi:chart-line",
            "Load\n$(self:load_average_1m)",
        ),
        readout_preset(
            "disk-free",
            "Machine",
            "Disk free",
            "mdi:harddisk",
            "Disk\n$(self:disk_free)",
        ),
        readout_preset(
            "uptime",
            "Machine",
            "System uptime",
            "mdi:clock-check-outline",
            "Uptime\n$(self:uptime)",
        ),
    ]);
    presets
}

fn readout_preset(preset_id: &str, category: &str, name: &str, icon: &str, text: &str) -> Preset {
    Preset {
        preset_id: preset_id.to_string(),
        category: category.to_string(),
        name: name.to_string(),
        description: None,
        control: ControlTemplate {
            name: name.to_string(),
            default_state: face(icon, text),
            pressed_state: None,
            action_bindings: Vec::new(),
        },
    }
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
    fn presets_offer_time_and_machine_readouts() {
        let offered = presets();
        assert_eq!(
            offered
                .iter()
                .map(|preset| preset.preset_id.as_str())
                .collect::<Vec<_>>(),
            [
                "clock",
                "date",
                "weekday",
                "timer-5-minutes",
                "timer-10-minutes",
                "timer-15-minutes",
                "cpu-usage",
                "memory-usage",
                "load-average",
                "disk-free",
                "uptime"
            ]
        );
    }

    #[test]
    fn remaining_time_rounds_up_to_the_next_second() {
        assert_eq!(format_remaining(Duration::from_secs(300)), "05:00");
        assert_eq!(format_remaining(Duration::from_millis(1)), "00:01");
    }

    #[test]
    fn bytes_and_uptime_are_key_sized_readouts() {
        assert_eq!(format_bytes(1_536), "1.5 KiB");
        assert_eq!(format_uptime(3_900), "01:05");
        assert_eq!(format_uptime(90_000), "1d 01:00");
    }
}
