use std::sync::atomic::Ordering;

use serde::Serialize;

use crate::{events::ServerEvent, identifiers::SurfaceId, surfaces::registry::SurfaceRegistry};

/// How many log lines a surface keeps. Enough to cover a burst of dial turns and still show what
/// came before it.
pub(super) const SURFACE_LOG_CAPACITY: usize = 400;

/// One line of a device's activity log: what the daemon saw the device do, and what it did back.
/// Memory only - a live diagnostic, not history worth persisting.
#[derive(Clone, Debug, Serialize)]
pub struct SurfaceLogEntry {
    pub surface_id: SurfaceId,
    /// Per-surface and monotonic, so the web can dedupe a snapshot against the live stream.
    pub sequence: u64,
    pub at_ms: u64,
    pub level: SurfaceLogLevel,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceLogLevel {
    Input,
    Info,
    Warning,
}

impl SurfaceRegistry {
    /// Appends one line to a surface's log and streams it to anyone watching the device page.
    pub fn log(&self, surface_id: &SurfaceId, level: SurfaceLogLevel, message: String) {
        let entry = SurfaceLogEntry {
            surface_id: surface_id.clone(),
            sequence: self.next_log_sequence.fetch_add(1, Ordering::Relaxed),
            at_ms: unix_epoch_ms(),
            level,
            message,
        };
        {
            let mut logs = self.logs.write().unwrap();
            let entries = logs.entry(surface_id.0.clone()).or_default();
            if entries.len() == SURFACE_LOG_CAPACITY {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }
        self.emit(ServerEvent::Log(entry));
    }
}

fn unix_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
