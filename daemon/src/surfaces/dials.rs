use serde::Serialize;
use tracing::debug;

use crate::{
    events::ServerEvent,
    identifiers::SurfaceId,
    panels::{rendered_state::RgbaColor, Panel},
    surfaces::{defaults::white, logs::SurfaceLogLevel, registry::SurfaceRegistry},
};

/// Rotary dials on a Stream Deck Studio, and the LED segments making up one dial ring.
/// A single detent of the knob is one segment.
pub const DIAL_COUNT: u8 = 2;
pub const DIAL_RING_SEGMENTS: u8 = 24;

/// Where a dial currently stands, as a percentage of its ring. Runtime only - the panel keeps the
/// level the dial starts from, and turning the knob never rewrites it.
#[derive(Clone, Debug, Serialize)]
pub struct SurfaceDialState {
    pub surface_id: SurfaceId,
    pub dial_index: u8,
    pub level: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct SurfaceDialPress {
    pub surface_id: SurfaceId,
    pub dial_index: u8,
    pub is_pressed: bool,
}

impl SurfaceRegistry {
    /// Every dial the active panel configures, as (index, colour, lit segments).
    pub fn active_dial_rings(&self, surface_id: &SurfaceId) -> Vec<(u8, RgbaColor, u8)> {
        let Some(device) = self.managed(surface_id) else {
            return Vec::new();
        };
        let Some(panel_id) = device.active_panel_id else {
            return Vec::new();
        };
        let Some(panel) = self.panel(&panel_id.0) else {
            return Vec::new();
        };
        panel
            .dial_colors
            .iter()
            .take(usize::from(DIAL_COUNT))
            .enumerate()
            .filter_map(|(index, color)| {
                let dial_index = u8::try_from(index).ok()?;
                Some((
                    dial_index,
                    color.clone(),
                    self.lit_segments(surface_id, dial_index, &panel, index),
                ))
            })
            .collect()
    }

    /// Turning the knob moves the ring one segment per detent, clamped at both ends. The new
    /// position is runtime state: it is pushed back to the device and broadcast, never persisted.
    pub fn record_dial_turn(&self, surface_id: &SurfaceId, dial_index: u8, detents: i8) {
        if dial_index >= DIAL_COUNT || detents == 0 {
            return;
        }
        let Some((color, current)) = self.dial_ring(surface_id, dial_index) else {
            debug!(
                surface_id = surface_id.0,
                dial_index, detents, "ignored a dial turn: the surface has no active panel"
            );
            return;
        };
        let moved = i16::from(current) + i16::from(detents);
        let next = u8::try_from(moved.clamp(0, i16::from(DIAL_RING_SEGMENTS))).unwrap_or(0);
        if next == current {
            debug!(
                surface_id = surface_id.0,
                dial_index,
                detents,
                lit_segments = current,
                "dial turn hit the end of its ring"
            );
            return;
        }
        debug!(
            surface_id = surface_id.0,
            dial_index,
            detents,
            from_segments = current,
            to_segments = next,
            "dial moved"
        );
        self.dial_positions
            .write()
            .unwrap()
            .insert((surface_id.0.clone(), dial_index), next);
        self.send_dial_color(surface_id, dial_index, color, next);
        let level = percent_from_segments(next);
        self.log(
            surface_id,
            SurfaceLogLevel::Input,
            format!(
                "dial {dial_index} turned {detents:+} to {level}% ({next}/{DIAL_RING_SEGMENTS})"
            ),
        );
        self.emit(ServerEvent::DialState {
            surface_id: surface_id.clone(),
            dial_index,
            level,
        });
    }

    /// Records a dial being pushed in or released. Returns whether that changed anything, so the
    /// caller can log edges rather than every report. Nothing is bound to a dial press yet.
    pub fn record_dial_press(
        &self,
        surface_id: &SurfaceId,
        dial_index: u8,
        is_pressed: bool,
    ) -> bool {
        if dial_index >= DIAL_COUNT {
            return false;
        }
        let previous = self
            .dial_presses
            .write()
            .unwrap()
            .insert((surface_id.0.clone(), dial_index), is_pressed);
        if previous == Some(is_pressed) {
            return false;
        }
        self.log(
            surface_id,
            SurfaceLogLevel::Input,
            format!(
                "dial {dial_index} {}",
                if is_pressed { "pressed" } else { "released" }
            ),
        );
        self.emit(ServerEvent::DialPress {
            surface_id: surface_id.clone(),
            dial_index,
            is_pressed,
        });
        true
    }

    /// Colour and current segment count for one dial. Dials the panel leaves unconfigured still
    /// respond to a turn, lit white, so the knob is never dead.
    fn dial_ring(&self, surface_id: &SurfaceId, dial_index: u8) -> Option<(RgbaColor, u8)> {
        let device = self.managed(surface_id)?;
        let panel = self.panel(&device.active_panel_id?.0)?;
        let index = usize::from(dial_index);
        let color = panel.dial_colors.get(index).cloned().unwrap_or_else(white);
        Some((
            color,
            self.lit_segments(surface_id, dial_index, &panel, index),
        ))
    }

    fn lit_segments(
        &self,
        surface_id: &SurfaceId,
        dial_index: u8,
        panel: &Panel,
        index: usize,
    ) -> u8 {
        self.dial_positions
            .read()
            .unwrap()
            .get(&(surface_id.0.clone(), dial_index))
            .copied()
            .unwrap_or_else(|| {
                segments_from_percent(panel.dial_ring_levels.get(index).copied().unwrap_or(100))
            })
    }

    /// Drops runtime dial state so the dials fall back to the panel's configured levels.
    pub(super) fn reset_dial_positions(&self, surface_id: &str) {
        self.dial_positions
            .write()
            .unwrap()
            .retain(|(id, _), _| id != surface_id);
        self.dial_presses
            .write()
            .unwrap()
            .retain(|(id, _), _| id != surface_id);
    }
}

fn segments_from_percent(percent: u8) -> u8 {
    let segments = u16::from(percent.min(100)) * u16::from(DIAL_RING_SEGMENTS) / 100;
    u8::try_from(segments).unwrap_or(DIAL_RING_SEGMENTS)
}

/// Rounds up, so a level reported to the API converts back to the same segment count the device is
/// lighting - one lit segment reads as 5%, not 4% (which would floor back to zero).
pub(super) fn percent_from_segments(lit_segments: u8) -> u8 {
    let segments = u16::from(lit_segments.min(DIAL_RING_SEGMENTS));
    u8::try_from((segments * 100).div_ceil(u16::from(DIAL_RING_SEGMENTS))).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::defaults::default_panel;

    #[test]
    fn a_dial_turn_moves_one_ring_segment_per_detent_and_clamps() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());
        // The default panel starts both dials at 100%, so a full ring.
        assert_eq!(registry.active_dial_rings(&surface_id)[0].2, 24);

        registry.record_dial_turn(&surface_id, 0, -4);
        assert_eq!(registry.active_dial_rings(&surface_id)[0].2, 20);
        assert_eq!(dial_level(&registry, &surface_id, 0), Some(84));

        registry.record_dial_turn(&surface_id, 0, -60);
        assert_eq!(registry.active_dial_rings(&surface_id)[0].2, 0);
        assert_eq!(dial_level(&registry, &surface_id, 0), Some(0));

        registry.record_dial_turn(&surface_id, 0, 1);
        assert_eq!(dial_level(&registry, &surface_id, 0), Some(5));

        registry.record_dial_turn(&surface_id, 0, 99);
        assert_eq!(dial_level(&registry, &surface_id, 0), Some(100));
        // The other dial keeps the panel's configured level.
        assert_eq!(dial_level(&registry, &surface_id, 1), None);
    }

    #[test]
    fn reports_dial_presses_and_releases_once_per_edge() {
        let registry = SurfaceRegistry::from_configuration(Vec::new(), vec![default_panel()]);
        let surface_id = SurfaceId("stream-deck-studio-1".to_string());

        assert!(registry.record_dial_press(&surface_id, 0, true));
        assert!(!registry.record_dial_press(&surface_id, 0, true));
        assert_eq!(registry.inventory().dial_presses.len(), 1);

        assert!(registry.record_dial_press(&surface_id, 0, false));
        assert!(registry.inventory().dial_presses.is_empty());
    }

    #[test]
    fn reported_dial_levels_survive_a_round_trip_to_segments() {
        for lit_segments in 0..=24 {
            let level = percent_from_segments(lit_segments);
            assert_eq!(segments_from_percent(level), lit_segments);
        }
    }

    fn dial_level(
        registry: &SurfaceRegistry,
        surface_id: &SurfaceId,
        dial_index: u8,
    ) -> Option<u8> {
        registry
            .inventory()
            .dial_states
            .into_iter()
            .find(|state| &state.surface_id == surface_id && state.dial_index == dial_index)
            .map(|state| state.level)
    }
}
