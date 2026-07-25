use serde_json::{Map, Value as JsonValue};

use crate::{
    panels::rendered_state::RgbaColor,
    plugins::{
        builtin::hass::protocol::ServiceCall,
        manifest::{ActionDefinition, ConfigField},
        plugin::PluginError,
    },
};

pub const CALL_SERVICE: &str = "call_service";
pub const LIGHT_TOGGLE: &str = "light.toggle";
pub const LIGHT_TURN_ON: &str = "light.turn_on";
pub const LIGHT_TURN_OFF: &str = "light.turn_off";
pub const LIGHT_SET_COLOR: &str = "light.set_color";

pub fn definitions() -> Vec<ActionDefinition> {
    vec![
        ActionDefinition::new(CALL_SERVICE)
            .label("Call a service")
            .description(
                "Calls any Home Assistant service. Every field resolves $(...) references.",
            )
            .parameters(vec![
                ConfigField::text("domain")
                    .label("Domain")
                    .placeholder("light")
                    .required(),
                ConfigField::text("service")
                    .label("Service")
                    .placeholder("turn_on")
                    .required(),
                ConfigField::text("entity_id")
                    .label("Entity")
                    .placeholder("light.kitchen"),
                ConfigField::text("service_data")
                    .label("Service data")
                    .placeholder("{ \"brightness_pct\": 40 }")
                    .help("JSON object sent as the service data."),
            ]),
        ActionDefinition::new(LIGHT_TOGGLE)
            .label("Toggle a light")
            .parameters(vec![entity_field()]),
        ActionDefinition::new(LIGHT_TURN_ON)
            .label("Turn a light on")
            .parameters(vec![
                entity_field(),
                brightness_field(),
                color_field().help("Optional. #rrggbb or r,g,b."),
            ]),
        ActionDefinition::new(LIGHT_TURN_OFF)
            .label("Turn a light off")
            .parameters(vec![entity_field()]),
        ActionDefinition::new(LIGHT_SET_COLOR)
            .label("Set a light's colour")
            .parameters(vec![
                entity_field(),
                color_field().required(),
                brightness_field(),
            ]),
    ]
}

fn entity_field() -> ConfigField {
    ConfigField::text("entity_id")
        .label("Entity")
        .placeholder("light.kitchen")
        .required()
}

fn brightness_field() -> ConfigField {
    ConfigField::number("brightness_pct").label("Brightness (%)")
}

fn color_field() -> ConfigField {
    ConfigField::text("color")
        .label("Colour")
        .placeholder("#e8b923")
}

/// Turns an invocation into the one service call it means. The convenience actions exist so a
/// binding reads as what it does; they all end up as `call_service`.
pub fn service_call(
    action_name: &str,
    parameters: &JsonValue,
    interpolate: &dyn Fn(&str) -> String,
) -> Result<ServiceCall, PluginError> {
    let text = |key: &str| optional_string(parameters, key).map(|value| interpolate(&value));
    let required = |key: &str| {
        text(key).ok_or_else(|| PluginError::Configuration(format!("{key} is required")))
    };

    match action_name {
        CALL_SERVICE => {
            let mut call = ServiceCall::new(required("domain")?, required("service")?);
            call.entity_id = text("entity_id");
            call.service_data = service_data(parameters, interpolate)?;
            Ok(call)
        }
        LIGHT_TOGGLE | LIGHT_TURN_OFF => {
            let service = match action_name {
                LIGHT_TOGGLE => "toggle",
                _ => "turn_off",
            };
            let mut call = ServiceCall::new("light", service);
            call.entity_id = Some(required("entity_id")?);
            Ok(call)
        }
        LIGHT_TURN_ON | LIGHT_SET_COLOR => {
            let mut call = ServiceCall::new("light", "turn_on");
            call.entity_id = Some(required("entity_id")?);

            let color = match action_name {
                LIGHT_SET_COLOR => Some(required("color")?),
                _ => text("color"),
            };
            if let Some(color) = color {
                let parsed = parse_color(&color).ok_or_else(|| {
                    PluginError::Configuration(format!("{color} is not a colour; use #rrggbb"))
                })?;
                call.service_data.insert(
                    "rgb_color".to_string(),
                    serde_json::json!([parsed.red, parsed.green, parsed.blue]),
                );
            }
            if let Some(brightness) = number(parameters, "brightness_pct") {
                call.service_data.insert(
                    "brightness_pct".to_string(),
                    serde_json::json!(brightness.clamp(0.0, 100.0).round()),
                );
            }
            Ok(call)
        }
        unknown => Err(PluginError::UnknownAction(unknown.to_string())),
    }
}

/// Accepts an object as well as the JSON text a generated form produces, because a single-line
/// field cannot hand over anything else.
fn service_data(
    parameters: &JsonValue,
    interpolate: &dyn Fn(&str) -> String,
) -> Result<Map<String, JsonValue>, PluginError> {
    let data = match parameters.get("service_data") {
        None | Some(JsonValue::Null) => return Ok(Map::new()),
        Some(JsonValue::String(text)) if text.trim().is_empty() => return Ok(Map::new()),
        Some(JsonValue::String(text)) => serde_json::from_str(&interpolate(text))
            .map_err(|error| PluginError::Configuration(format!("service_data: {error}")))?,
        Some(other) => interpolated(other, interpolate),
    };
    match data {
        JsonValue::Object(fields) => Ok(fields),
        _ => Err(PluginError::Configuration(
            "service_data must be a JSON object".to_string(),
        )),
    }
}

fn interpolated(value: &JsonValue, interpolate: &dyn Fn(&str) -> String) -> JsonValue {
    match value {
        JsonValue::String(text) => JsonValue::String(interpolate(text)),
        JsonValue::Array(items) => JsonValue::Array(
            items
                .iter()
                .map(|item| interpolated(item, interpolate))
                .collect(),
        ),
        JsonValue::Object(fields) => JsonValue::Object(
            fields
                .iter()
                .map(|(key, item)| (key.clone(), interpolated(item, interpolate)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn parse_color(value: &str) -> Option<RgbaColor> {
    if let Some(color) = RgbaColor::from_hex(value) {
        return Some(color);
    }
    let mut components = value
        .split(',')
        .map(|component| component.trim().parse::<u8>().ok());
    let mut next = || components.next().flatten();
    let color = RgbaColor::opaque(next()?, next()?, next()?);

    components.next().is_none().then_some(color)
}

fn number(parameters: &JsonValue, key: &str) -> Option<f64> {
    match parameters.get(key)? {
        JsonValue::Number(value) => value.as_f64(),
        JsonValue::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}

/// A generated form and a hand-written TOML disagree about how `40` should be typed, so a number
/// is accepted where a string is expected.
fn optional_string(parameters: &JsonValue, key: &str) -> Option<String> {
    match parameters.get(key)? {
        JsonValue::String(value) if value.trim().is_empty() => None,
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Null => None,
        other => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn verbatim(value: &str) -> String {
        value.to_string()
    }

    fn call(action_name: &str, parameters: JsonValue) -> Result<ServiceCall, PluginError> {
        service_call(action_name, &parameters, &verbatim)
    }

    #[test]
    fn a_toggle_targets_the_named_light() {
        let call = call(LIGHT_TOGGLE, json!({ "entity_id": "light.kitchen" })).expect("valid");
        assert_eq!(call.domain, "light");
        assert_eq!(call.service, "toggle");
        assert_eq!(call.entity_id.as_deref(), Some("light.kitchen"));
        assert!(call.service_data.is_empty());
    }

    #[test]
    fn turning_on_carries_the_colour_and_brightness_it_was_given() {
        let call = call(
            LIGHT_TURN_ON,
            json!({ "entity_id": "light.kitchen", "color": "#e8b923", "brightness_pct": 40 }),
        )
        .expect("valid");
        assert_eq!(call.service, "turn_on");
        assert_eq!(call.service_data["rgb_color"], json!([232, 185, 35]));
        assert_eq!(call.service_data["brightness_pct"], json!(40.0));
    }

    #[test]
    fn turning_on_without_a_colour_leaves_the_light_as_it_was() {
        let call = call(LIGHT_TURN_ON, json!({ "entity_id": "light.kitchen" })).expect("valid");
        assert!(call.service_data.is_empty());
    }

    #[test]
    fn setting_a_colour_requires_one() {
        assert_eq!(
            call(LIGHT_SET_COLOR, json!({ "entity_id": "light.kitchen" })),
            Err(PluginError::Configuration("color is required".to_string()))
        );
    }

    #[test]
    fn a_colour_is_read_from_either_notation() {
        let hex = call(
            LIGHT_SET_COLOR,
            json!({ "entity_id": "light.kitchen", "color": "e8b923" }),
        )
        .expect("valid");
        let components = call(
            LIGHT_SET_COLOR,
            json!({ "entity_id": "light.kitchen", "color": "232, 185, 35" }),
        )
        .expect("valid");
        assert_eq!(hex.service_data["rgb_color"], json!([232, 185, 35]));
        assert_eq!(components.service_data, hex.service_data);
    }

    #[test]
    fn an_unreadable_colour_is_reported_rather_than_sent() {
        let error = call(
            LIGHT_SET_COLOR,
            json!({ "entity_id": "light.kitchen", "color": "burgundy" }),
        )
        .expect_err("burgundy is not a colour");
        assert!(matches!(error, PluginError::Configuration(_)));
    }

    #[test]
    fn a_service_call_passes_its_data_through() {
        let call = call(
            CALL_SERVICE,
            json!({
                "domain": "media_player",
                "service": "volume_set",
                "entity_id": "media_player.study",
                "service_data": { "volume_level": 0.4 },
            }),
        )
        .expect("valid");
        assert_eq!(call.domain, "media_player");
        assert_eq!(call.service, "volume_set");
        assert_eq!(call.service_data["volume_level"], json!(0.4));
    }

    #[test]
    fn service_data_written_as_text_is_parsed() {
        let call = call(
            CALL_SERVICE,
            json!({
                "domain": "light",
                "service": "turn_on",
                "service_data": "{\"brightness_pct\": 60}",
            }),
        )
        .expect("valid");
        assert_eq!(call.service_data["brightness_pct"], json!(60));
        assert_eq!(call.entity_id, None);
    }

    #[test]
    fn unreadable_service_data_is_a_configuration_error() {
        let error = call(
            CALL_SERVICE,
            json!({ "domain": "light", "service": "turn_on", "service_data": "brightness=60" }),
        )
        .expect_err("that is not JSON");
        assert!(matches!(error, PluginError::Configuration(_)));
    }

    #[test]
    fn references_are_resolved_in_every_string_a_call_carries() {
        let resolved = service_call(
            CALL_SERVICE,
            &json!({
                "domain": "light",
                "service": "turn_on",
                "entity_id": "$(user:target)",
                "service_data": { "effect": "$(user:effect)" },
            }),
            &|value| match value {
                "$(user:target)" => "light.hall".to_string(),
                "$(user:effect)" => "rainbow".to_string(),
                other => other.to_string(),
            },
        )
        .expect("valid");
        assert_eq!(resolved.entity_id.as_deref(), Some("light.hall"));
        assert_eq!(resolved.service_data["effect"], json!("rainbow"));
    }

    #[test]
    fn a_missing_domain_is_reported_by_name() {
        assert_eq!(
            call(CALL_SERVICE, json!({ "service": "turn_on" })),
            Err(PluginError::Configuration("domain is required".to_string()))
        );
    }

    #[test]
    fn an_unknown_action_is_reported_by_name() {
        assert_eq!(
            call("light.explode", json!({})),
            Err(PluginError::UnknownAction("light.explode".to_string()))
        );
    }

    #[test]
    fn every_declared_action_is_dispatchable() {
        for definition in definitions() {
            let parameters = json!({
                "domain": "light",
                "service": "turn_on",
                "entity_id": "light.kitchen",
                "color": "#ffffff",
            });
            assert!(
                service_call(&definition.name, &parameters, &verbatim).is_ok(),
                "{} is declared but not handled",
                definition.name
            );
        }
    }
}
