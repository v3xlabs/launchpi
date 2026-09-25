use serde_json::{json, Value as JsonValue};

use crate::{
    bindings::action::{Action, ActionBinding, ActionTrigger},
    identifiers::IntegrationId,
    panels::{
        control::ControlTemplate,
        rendered_state::{ColorBinding, Layer, RenderedState},
    },
    plugins::{
        builtin::missiond::{
            connection::View, ACTIVATE_PLAYLIST, ACTIVATE_TAB, NEXT_TAB, PAUSE, PREVIOUS_TAB,
            RESUME, SLEEP_SCREEN, TOGGLE_PAUSE, TOGGLE_SCREEN, WAKE_SCREEN,
        },
        preset::Preset,
    },
};

/// The fixed controls, then one key per playlist and one per tab, in missiond's own order so that
/// republishing after an edit does not shuffle the picker.
pub fn from_view(view: &View) -> Vec<Preset> {
    let mut presets = vec![
        pressing(
            preset("pause", "Playback", "Pause", "mdi:pause", "Pause"),
            PAUSE,
        ),
        pressing(
            preset("resume", "Playback", "Resume", "mdi:play", "Resume"),
            RESUME,
        ),
        outlined(
            pressing(
                preset(
                    "toggle-pause",
                    "Playback",
                    "Pause or resume",
                    "mdi:play-pause",
                    "$(self:playback_state)",
                ),
                TOGGLE_PAUSE,
            ),
            "$(self:playback_color)",
        ),
        pressing(
            preset("next-tab", "Playback", "Next tab", "mdi:skip-next", "Next"),
            NEXT_TAB,
        ),
        pressing(
            preset(
                "previous-tab",
                "Playback",
                "Previous tab",
                "mdi:skip-previous",
                "Previous",
            ),
            PREVIOUS_TAB,
        ),
        pressing(
            preset(
                "sleep",
                "Screen",
                "Sleep screen",
                "mdi:power-sleep",
                "Sleep",
            ),
            SLEEP_SCREEN,
        ),
        pressing(
            preset(
                "wake",
                "Screen",
                "Wake screen",
                "mdi:white-balance-sunny",
                "Wake",
            ),
            WAKE_SCREEN,
        ),
        outlined(
            pressing(
                preset(
                    "toggle-screen",
                    "Screen",
                    "Sleep or wake screen",
                    "mdi:monitor",
                    "$(self:screen_state)",
                ),
                TOGGLE_SCREEN,
            ),
            "$(self:screen_color)",
        ),
        preset(
            "current-playlist",
            "Now showing",
            "Current playlist",
            "mdi:playlist-play",
            "$(self:playlist_name)",
        ),
        preset(
            "current-tab",
            "Now showing",
            "Current tab",
            "mdi:tab",
            "$(self:tab_name)",
        ),
        preset(
            "next-tab-countdown",
            "Now showing",
            "Time to next tab",
            "mdi:timer-sand",
            "$(self:next_tab_in)",
        ),
    ];

    for playlist in &view.playlists {
        let playlist_id = &playlist.playlist_id;
        presets.push(outlined(
            invoking(
                preset(
                    format!("playlist:{playlist_id}"),
                    "Playlists",
                    &playlist.name,
                    "mdi:playlist-play",
                    &playlist.name,
                ),
                ACTIVATE_PLAYLIST,
                json!({ "playlist_id": playlist_id }),
            ),
            &format!("$(self:playlists.{playlist_id}.color)"),
        ));
    }

    for (_, tab) in view.tabs() {
        let tab_id = &tab.tab_id;
        presets.push(outlined(
            invoking(
                preset(
                    format!("tab:{tab_id}"),
                    "Tabs",
                    &tab.name,
                    "mdi:tab",
                    &tab.name,
                ),
                ACTIVATE_TAB,
                json!({ "tab_id": tab_id }),
            ),
            &format!("$(self:tabs.{tab_id}.color)"),
        ));
    }

    presets
}

fn preset(
    preset_id: impl Into<String>,
    category: &str,
    name: &str,
    icon: &str,
    label: &str,
) -> Preset {
    Preset {
        preset_id: preset_id.into(),
        category: category.to_string(),
        name: name.to_string(),
        description: None,
        control: ControlTemplate {
            name: name.to_string(),
            default_state: RenderedState::icon_and_label(icon, label),
            pressed_state: None,
            action_bindings: Vec::new(),
        },
    }
}

fn pressing(preset: Preset, action_name: &str) -> Preset {
    invoking(preset, action_name, json!({}))
}

fn invoking(mut preset: Preset, action_name: &str, parameters: JsonValue) -> Preset {
    preset.control.action_bindings.push(ActionBinding {
        gesture: ActionTrigger::Press,
        actions: vec![Action::InvokeIntegration {
            integration_id: IntegrationId("self".to_string()),
            action_name: action_name.to_string(),
            parameters,
        }],
    });
    preset
}

/// The state colour goes on the outline so the label stays legible whether the key is lit or not.
fn outlined(mut preset: Preset, color: &str) -> Preset {
    preset.control.default_state.layers.push(Layer::Border {
        color: ColorBinding::Reference(color.to_string()),
        width: 5,
    });
    preset
}
