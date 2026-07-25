use serde_json::Value as JsonValue;

use crate::{panels::rendered_state::RgbaColor, variables::VariableValue};

/// Full brightness in Home Assistant's `brightness` attribute.
const MAX_BRIGHTNESS: f64 = 255.0;

/// One subscribed value name, split into the entity it reads and the part of that entity's state
/// it publishes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueBinding {
    pub name: String,
    pub entity_id: String,
    pub field: String,
}

/// Reads `light.kitchen.color` as "the colour of `light.kitchen`". An entity id is exactly two
/// segments, so everything after the second dot is the field, and a bare entity id reads its state.
pub fn parse_binding(name: &str) -> Option<ValueBinding> {
    let (domain, rest) = name.split_once('.')?;
    if domain.is_empty() {
        return None;
    }
    let (object_id, field) = match rest.split_once('.') {
        Some((object_id, field)) => (object_id, field),
        None => (rest, "state"),
    };
    if object_id.is_empty() || field.is_empty() {
        return None;
    }

    Some(ValueBinding {
        name: name.to_string(),
        entity_id: format!("{domain}.{object_id}"),
        field: field.to_string(),
    })
}

pub fn entity_id_of(state: &JsonValue) -> Option<&str> {
    state.get("entity_id").and_then(JsonValue::as_str)
}

/// What a field of one entity state publishes. `None` leaves the previous value in place, which is
/// what an attribute an entity simply does not carry should do.
pub fn value_for_field(state: &JsonValue, field: &str) -> Option<VariableValue> {
    match field {
        "state" => Some(VariableValue::Text(state_text(state)?.to_string())),
        "on" => Some(VariableValue::Boolean(state_text(state)? == "on")),
        "color" => color_of(state).map(VariableValue::Color),
        "brightness_pct" => attribute(state, "brightness")
            .and_then(JsonValue::as_f64)
            .map(|brightness| VariableValue::Number((brightness * 100.0 / MAX_BRIGHTNESS).round())),
        path => lookup(state, path).map(scalar),
    }
}

fn state_text(state: &JsonValue) -> Option<&str> {
    state.get("state").and_then(JsonValue::as_str)
}

/// Attributes win over the top level so that `friendly_name` and `unit_of_measurement` read the way
/// they are written in Home Assistant, while `last_changed` still resolves.
fn lookup<'a>(state: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    let path = path.strip_prefix("attributes.").unwrap_or(path);
    walk(state.get("attributes")?, path).or_else(|| walk(state, path))
}

fn walk<'a>(document: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    let mut current = document;
    for segment in path.split('.') {
        current = match current {
            JsonValue::Object(fields) => fields.get(segment)?,
            JsonValue::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

/// A light that reports no colour is still asked for one by whatever key draws it, so an entity
/// that is on reads as white and anything else as black rather than keeping a stale colour.
fn color_of(state: &JsonValue) -> Option<RgbaColor> {
    if let Some(color) = attribute(state, "rgb_color").and_then(rgb_from_array) {
        return Some(color);
    }
    if let Some(color) = attribute(state, "hs_color")
        .and_then(JsonValue::as_array)
        .and_then(|components| {
            hue_saturation(components.first()?.as_f64()?, components.get(1)?.as_f64()?)
        })
    {
        return Some(color);
    }

    match state_text(state)? {
        "on" => Some(RgbaColor::opaque(255, 255, 255)),
        _ => Some(RgbaColor::opaque(0, 0, 0)),
    }
}

fn attribute<'a>(state: &'a JsonValue, name: &str) -> Option<&'a JsonValue> {
    state.get("attributes")?.get(name)
}

fn rgb_from_array(value: &JsonValue) -> Option<RgbaColor> {
    let components = value.as_array()?;
    let component = |at: usize| -> Option<u8> {
        Some(components.get(at)?.as_f64()?.clamp(0.0, 255.0).round() as u8)
    };
    Some(RgbaColor::opaque(
        component(0)?,
        component(1)?,
        component(2)?,
    ))
}

fn hue_saturation(hue: f64, saturation: f64) -> Option<RgbaColor> {
    let saturation = saturation / 100.0;
    let sector = hue.rem_euclid(360.0) / 60.0;
    let secondary = saturation * (1.0 - (sector % 2.0 - 1.0).abs());
    let (red, green, blue) = match sector as u8 {
        0 => (saturation, secondary, 0.0),
        1 => (secondary, saturation, 0.0),
        2 => (0.0, saturation, secondary),
        3 => (0.0, secondary, saturation),
        4 => (secondary, 0.0, saturation),
        _ => (saturation, 0.0, secondary),
    };
    let offset = 1.0 - saturation;
    let byte = |component: f64| ((component + offset) * 255.0).round() as u8;
    Some(RgbaColor::opaque(byte(red), byte(green), byte(blue)))
}

fn scalar(value: &JsonValue) -> VariableValue {
    match value {
        JsonValue::Bool(value) => VariableValue::Boolean(*value),
        JsonValue::Number(value) => VariableValue::Number(value.as_f64().unwrap_or_default()),
        JsonValue::String(value) => VariableValue::Text(value.clone()),
        JsonValue::Null => VariableValue::Text(String::new()),
        other => VariableValue::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kitchen_light() -> JsonValue {
        serde_json::json!({
            "entity_id": "light.kitchen",
            "state": "on",
            "attributes": {
                "friendly_name": "Kitchen",
                "brightness": 128,
                "rgb_color": [232, 185, 35],
                "hs_color": [45.4, 84.9],
                "supported_color_modes": ["hs"],
            },
            "last_changed": "2026-07-25T10:00:00.000000+00:00",
        })
    }

    #[test]
    fn a_name_splits_into_an_entity_and_a_field() {
        assert_eq!(
            parse_binding("light.kitchen.color"),
            Some(ValueBinding {
                name: "light.kitchen.color".to_string(),
                entity_id: "light.kitchen".to_string(),
                field: "color".to_string(),
            })
        );
    }

    #[test]
    fn a_bare_entity_id_reads_its_state() {
        let binding = parse_binding("sensor.outside_temp").expect("an entity id is a value name");
        assert_eq!(binding.entity_id, "sensor.outside_temp");
        assert_eq!(binding.field, "state");
    }

    #[test]
    fn a_field_keeps_every_segment_past_the_entity_id() {
        let binding =
            parse_binding("sensor.outside_temp.attributes.unit_of_measurement").expect("parses");
        assert_eq!(binding.entity_id, "sensor.outside_temp");
        assert_eq!(binding.field, "attributes.unit_of_measurement");
    }

    #[test]
    fn a_name_that_is_not_an_entity_reference_is_ignored() {
        assert_eq!(parse_binding("kitchen"), None);
        assert_eq!(parse_binding("light."), None);
        assert_eq!(parse_binding(".kitchen"), None);
    }

    #[test]
    fn the_state_field_publishes_the_state_string() {
        assert_eq!(
            value_for_field(&kitchen_light(), "state"),
            Some(VariableValue::Text("on".to_string()))
        );
        assert_eq!(
            value_for_field(&kitchen_light(), "on"),
            Some(VariableValue::Boolean(true))
        );
    }

    #[test]
    fn the_color_field_publishes_the_reported_colour() {
        assert_eq!(
            value_for_field(&kitchen_light(), "color"),
            Some(VariableValue::Color(RgbaColor::opaque(232, 185, 35)))
        );
    }

    #[test]
    fn a_light_without_an_rgb_colour_falls_back_to_hue_and_saturation() {
        let state = serde_json::json!({
            "entity_id": "light.hall",
            "state": "on",
            "attributes": { "hs_color": [0.0, 100.0] },
        });
        assert_eq!(
            value_for_field(&state, "color"),
            Some(VariableValue::Color(RgbaColor::opaque(255, 0, 0)))
        );
    }

    #[test]
    fn an_entity_with_no_colour_at_all_reads_white_when_on_and_black_otherwise() {
        let on = serde_json::json!({ "entity_id": "switch.fan", "state": "on" });
        let off = serde_json::json!({ "entity_id": "switch.fan", "state": "off" });
        assert_eq!(
            value_for_field(&on, "color"),
            Some(VariableValue::Color(RgbaColor::opaque(255, 255, 255)))
        );
        assert_eq!(
            value_for_field(&off, "color"),
            Some(VariableValue::Color(RgbaColor::opaque(0, 0, 0)))
        );
    }

    #[test]
    fn brightness_reads_raw_and_as_a_percentage() {
        assert_eq!(
            value_for_field(&kitchen_light(), "brightness"),
            Some(VariableValue::Number(128.0))
        );
        assert_eq!(
            value_for_field(&kitchen_light(), "brightness_pct"),
            Some(VariableValue::Number(50.0))
        );
    }

    #[test]
    fn an_attribute_resolves_with_or_without_its_prefix() {
        assert_eq!(
            value_for_field(&kitchen_light(), "friendly_name"),
            Some(VariableValue::Text("Kitchen".to_string()))
        );
        assert_eq!(
            value_for_field(&kitchen_light(), "attributes.supported_color_modes.0"),
            Some(VariableValue::Text("hs".to_string()))
        );
    }

    #[test]
    fn a_top_level_field_resolves_when_no_attribute_shadows_it() {
        assert_eq!(
            value_for_field(&kitchen_light(), "last_changed"),
            Some(VariableValue::Text(
                "2026-07-25T10:00:00.000000+00:00".to_string()
            ))
        );
    }

    #[test]
    fn an_absent_field_publishes_nothing_rather_than_an_empty_value() {
        assert_eq!(value_for_field(&kitchen_light(), "nonsense"), None);
    }

    #[test]
    fn an_unavailable_entity_still_publishes_its_state() {
        let state = serde_json::json!({
            "entity_id": "sensor.outside_temp",
            "state": "unavailable",
            "attributes": {},
        });
        assert_eq!(
            value_for_field(&state, "state"),
            Some(VariableValue::Text("unavailable".to_string()))
        );
    }
}
