use std::collections::BTreeMap;

use serde_json::json;

use crate::{
    bindings::action::{Action, ActionBinding, ActionTrigger},
    identifiers::IntegrationId,
    panels::{
        control::ControlTemplate,
        rendered_state::{Border, ColorBinding, RenderedState, RgbaColor},
    },
    plugins::{
        builtin::hass::{
            actions::{CALL_SERVICE, LIGHT_TOGGLE},
            connection::CatalogueEntry,
        },
        preset::Preset,
    },
};

/// What one press of a key for this domain should do, and whether reading the entity's state back
/// says anything a person wants on the key. A scene reports the moment it was last applied and a
/// button reports nothing at all, so neither has a state worth printing.
struct Recommended {
    category: &'static str,
    service: &'static str,
    reports_state: bool,
}

/// Only the domains a single press can act on. A sensor is left out deliberately: it has nothing to
/// press, the value picker already offers every one of its readings, and an installation carries
/// enough of them to bury every light behind them.
fn recommended(domain: &str) -> Option<Recommended> {
    let (category, service, reports_state) = match domain {
        "light" => ("Lights", "toggle", true),
        "switch" => ("Switches", "toggle", true),
        "fan" => ("Fans", "toggle", true),
        "input_boolean" => ("Toggles", "toggle", true),
        "cover" => ("Covers", "toggle", true),
        "automation" => ("Automations", "toggle", true),
        "media_player" => ("Media players", "media_play_pause", true),
        "scene" => ("Scenes", "turn_on", false),
        "script" => ("Scripts", "turn_on", false),
        "input_button" => ("Buttons", "press", false),
        _ => return None,
    };

    Some(Recommended {
        category,
        service,
        reports_state,
    })
}

/// One ready-made key per entity worth pressing, in the catalogue's own order so that republishing
/// after a rename does not shuffle the picker around whatever the cursor is over.
pub fn from_catalogue(catalogue: &BTreeMap<String, CatalogueEntry>) -> Vec<Preset> {
    catalogue
        .iter()
        .filter_map(|(entity_id, entry)| {
            recommended(&entry.domain).map(|shape| preset(entity_id, entry, &shape))
        })
        .collect()
}

fn preset(entity_id: &str, entry: &CatalogueEntry, shape: &Recommended) -> Preset {
    let name = entry
        .friendly_name
        .clone()
        .unwrap_or_else(|| entity_id.to_string());
    let text = match shape.reports_state {
        true => format!("{name}\n$(self:{entity_id}.state)"),
        false => name.clone(),
    };

    Preset {
        preset_id: entity_id.to_string(),
        category: shape.category.to_string(),
        name: name.clone(),
        // An installation happily calls five media players "Bedroom", and the id is the only thing
        // that tells them apart.
        description: Some(entity_id.to_string()),
        control: ControlTemplate {
            name,
            default_state: RenderedState {
                text: Some(text),
                foreground_color: Some(RgbaColor::opaque(255, 255, 255).into()),
                background_color: Some(RgbaColor::opaque(30, 41, 59).into()),
                border: outline(&entry.domain, entity_id),
                ..RenderedState::default()
            },
            pressed_state: None,
            action_bindings: vec![ActionBinding {
                gesture: ActionTrigger::Press,
                actions: vec![press(entity_id, &entry.domain, shape.service)],
            }],
        },
    }
}

/// A light is the only domain `fields_for` reports a colour for. It goes on the outline rather than
/// the face because `values.rs` answers black for an entity that is off and white for one that is
/// on without a colour of its own, and a key whose whole face is that colour has no legible label
/// left at either end of the range.
fn outline(domain: &str, entity_id: &str) -> Option<Border> {
    (domain == "light").then(|| Border {
        color: ColorBinding::Reference(format!("$(self:{entity_id}.color)")),
        width: 5,
    })
}

fn press(entity_id: &str, domain: &str, service: &str) -> Action {
    let (action_name, parameters) = match domain {
        "light" => (LIGHT_TOGGLE, json!({ "entity_id": entity_id })),
        _ => (
            CALL_SERVICE,
            json!({
                "domain": domain,
                "service": service,
                "entity_id": entity_id,
            }),
        ),
    };

    Action::InvokeIntegration {
        integration_id: IntegrationId("self".to_string()),
        action_name: action_name.to_string(),
        parameters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogue(entries: &[(&str, Option<&str>)]) -> BTreeMap<String, CatalogueEntry> {
        entries
            .iter()
            .map(|(entity_id, friendly_name)| {
                (
                    (*entity_id).to_string(),
                    CatalogueEntry {
                        friendly_name: friendly_name.map(str::to_string),
                        domain: entity_id.split('.').next().unwrap_or_default().to_string(),
                    },
                )
            })
            .collect()
    }

    fn only(entity_id: &str, friendly_name: &str) -> Preset {
        from_catalogue(&catalogue(&[(entity_id, Some(friendly_name))]))
            .pop()
            .unwrap_or_else(|| panic!("{entity_id} is worth a key"))
    }

    fn pressed(preset: &Preset) -> (&str, &serde_json::Value) {
        let Action::InvokeIntegration {
            integration_id,
            action_name,
            parameters,
        } = &preset.control.action_bindings[0].actions[0]
        else {
            panic!("a preset presses its own instance");
        };
        assert_eq!(
            integration_id.0, "self",
            "an instance id written into a preset would be wrong for every other instance"
        );
        (action_name, parameters)
    }

    /// An installation carries a thousand sensors and a hundred maintenance buttons. Offering a key
    /// for each would leave nothing findable.
    #[test]
    fn an_entity_that_cannot_be_pressed_is_not_offered_a_key() {
        let offered = from_catalogue(&catalogue(&[
            ("sensor.outside_temp", Some("Outside")),
            ("binary_sensor.front_door", Some("Front Door")),
            ("update.esphome_kitchen", Some("Kitchen Update")),
            ("button.kitchen_identify", Some("Identify")),
            ("device_tracker.phone", Some("Phone")),
            ("light.kitchen", Some("Kitchen")),
        ]));

        let ids: Vec<_> = offered
            .iter()
            .map(|preset| preset.preset_id.as_str())
            .collect();
        assert_eq!(ids, ["light.kitchen"]);
    }

    #[test]
    fn a_light_toggles_and_outlines_itself_in_its_own_colour() {
        let preset = only("light.kitchen", "Kitchen");
        let state = &preset.control.default_state;

        assert_eq!(
            state.text.as_deref(),
            Some("Kitchen\n$(self:light.kitchen.state)")
        );
        assert_eq!(
            state.border.as_ref().map(|border| &border.color),
            Some(&ColorBinding::Reference(
                "$(self:light.kitchen.color)".to_string()
            ))
        );

        let (action_name, parameters) = pressed(&preset);
        assert_eq!(action_name, LIGHT_TOGGLE);
        assert_eq!(parameters["entity_id"], json!("light.kitchen"));
    }

    /// Every other domain reports white when on and black when off, which says nothing about the
    /// entity and only makes the label harder to read.
    #[test]
    fn only_a_light_is_outlined_in_a_colour_it_actually_reports() {
        for entity_id in ["switch.desk_lamp", "fan.purifier", "scene.movie"] {
            assert_eq!(
                only(entity_id, "Anything").control.default_state.border,
                None,
                "{entity_id} does not report a colour"
            );
        }
    }

    #[test]
    fn a_domain_without_a_light_shortcut_is_pressed_through_call_service() {
        let (action_name, parameters) = {
            let preset = only("switch.desk_lamp", "Desk Lamp");
            let (action_name, parameters) = pressed(&preset);
            (action_name.to_string(), parameters.clone())
        };

        assert_eq!(action_name, CALL_SERVICE);
        assert_eq!(parameters["domain"], json!("switch"));
        assert_eq!(parameters["service"], json!("toggle"));
        assert_eq!(parameters["entity_id"], json!("switch.desk_lamp"));
    }

    /// A scene's state is the timestamp it was last applied and a script's is whether it happens to
    /// be running, so a key for either shows what it does rather than a reading nobody asked for.
    #[test]
    fn a_scene_and_a_script_run_rather_than_reporting_a_state() {
        for (entity_id, service) in [("scene.movie", "turn_on"), ("script.arm_home", "turn_on")] {
            let preset = only(entity_id, "Named");
            assert_eq!(preset.control.default_state.text.as_deref(), Some("Named"));

            let (_, parameters) = pressed(&preset);
            assert_eq!(parameters["service"], json!(service));
        }
    }

    #[test]
    fn a_media_player_key_plays_and_pauses_and_shows_what_it_is_doing() {
        let preset = only("media_player.study", "Study");
        assert_eq!(
            preset.control.default_state.text.as_deref(),
            Some("Study\n$(self:media_player.study.state)")
        );

        let (_, parameters) = pressed(&preset);
        assert_eq!(parameters["service"], json!("media_play_pause"));
    }

    #[test]
    fn an_entity_with_no_friendly_name_is_labelled_with_its_id() {
        let preset = from_catalogue(&catalogue(&[("light.kitchen", None)]))
            .pop()
            .expect("a light is offered whether or not it is named");
        assert_eq!(preset.name, "light.kitchen");
        assert_eq!(
            preset.control.default_state.text.as_deref(),
            Some("light.kitchen\n$(self:light.kitchen.state)")
        );
    }

    /// The picker keys off `preset_id`, so a republish that renumbered them would move every entry
    /// out from under whatever the cursor was on.
    #[test]
    fn a_preset_is_identified_by_its_entity_so_a_republish_keeps_its_place() {
        let before = from_catalogue(&catalogue(&[
            ("light.kitchen", Some("Kitchen")),
            ("switch.desk_lamp", Some("Desk Lamp")),
        ]));
        let after = from_catalogue(&catalogue(&[
            ("light.hall", Some("Hall")),
            ("light.kitchen", Some("Kitchen")),
            ("switch.desk_lamp", Some("Desk Lamp")),
        ]));

        assert_eq!(
            before
                .iter()
                .map(|preset| preset.preset_id.as_str())
                .collect::<Vec<_>>(),
            ["light.kitchen", "switch.desk_lamp"]
        );
        assert_eq!(
            after
                .iter()
                .map(|preset| preset.preset_id.as_str())
                .collect::<Vec<_>>(),
            ["light.hall", "light.kitchen", "switch.desk_lamp"]
        );
    }
}
