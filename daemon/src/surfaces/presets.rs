use crate::{
    bindings::action::{Action, ActionBinding, ActionTrigger},
    panels::{control::ControlTemplate, rendered_state::RenderedState},
    plugins::preset::Preset,
};

/// Ready-made buttons for the backlight controls the daemon owns itself. They are published beside
/// the plugin-supplied presets rather than from a plugin, because no plugin owns a surface's
/// backlight and asking the user to install one to sleep a deck would be absurd.
pub const DISPLAY_PRESET_SOURCE: &str = "displays";
pub const DISPLAY_PRESET_NAME: &str = "Displays";

/// Every preset targets only the surface it is pressed on. Naming another surface is an edit the
/// picker cannot make for you, because it cannot know which of your decks you meant.
pub fn display_presets() -> Vec<Preset> {
    let mut presets = vec![preset(
        "sleep",
        "Power",
        "Sleep",
        "mdi:power-sleep",
        "Sleep",
        Action::SetSurfaceDisplay {
            surface_ids: Vec::new(),
            include_triggering_surface: true,
            is_display_off: true,
        },
    )];
    presets.extend(
        [
            (25, "mdi:brightness-5"),
            (50, "mdi:brightness-6"),
            (100, "mdi:brightness-7"),
        ]
        .map(|(brightness, icon)| {
            preset(
                &format!("brightness-{brightness}"),
                "Brightness",
                &format!("{brightness}%"),
                icon,
                &format!("{brightness}%"),
                Action::SetSurfaceBrightness {
                    surface_ids: Vec::new(),
                    include_triggering_surface: true,
                    brightness,
                },
            )
        }),
    );
    presets
}

fn preset(
    preset_id: &str,
    category: &str,
    name: &str,
    icon: &str,
    label: &str,
    action: Action,
) -> Preset {
    Preset {
        preset_id: preset_id.to_string(),
        category: category.to_string(),
        name: name.to_string(),
        description: None,
        control: ControlTemplate {
            name: name.to_string(),
            default_state: RenderedState::icon_and_label(icon, label),
            pressed_state: None,
            action_bindings: vec![ActionBinding {
                gesture: ActionTrigger::Press,
                actions: vec![action],
            }],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleeping_targets_the_surface_the_key_was_pressed_on() {
        let sleep = display_presets()
            .into_iter()
            .find(|preset| preset.preset_id == "sleep")
            .expect("sleep is offered");

        assert_eq!(
            sleep.control.action_bindings[0].actions,
            vec![Action::SetSurfaceDisplay {
                surface_ids: Vec::new(),
                include_triggering_surface: true,
                is_display_off: true,
            }]
        );
    }

    #[test]
    fn every_brightness_step_is_offered_once() {
        assert_eq!(
            display_presets()
                .iter()
                .filter_map(
                    |preset| match &preset.control.action_bindings[0].actions[0] {
                        Action::SetSurfaceBrightness { brightness, .. } => Some(*brightness),
                        _ => None,
                    }
                )
                .collect::<Vec<_>>(),
            [25, 50, 100]
        );
    }
}
