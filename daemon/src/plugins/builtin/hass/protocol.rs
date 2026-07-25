use serde_json::{json, Map, Value as JsonValue};

/// What one frame from Home Assistant means. Everything the connection reacts to is decided here,
/// so the socket task is left with sequencing rather than parsing.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerMessage {
    AuthRequired,
    AuthOk,
    AuthInvalid {
        message: String,
    },
    /// The new state carried by a `state_changed` event.
    State(JsonValue),
    Result {
        id: u64,
        outcome: Result<JsonValue, String>,
    },
    Ignored,
}

pub fn parse_server_message(payload: &str) -> Result<ServerMessage, String> {
    let message: JsonValue =
        serde_json::from_str(payload).map_err(|error| format!("unreadable frame: {error}"))?;

    let message_type = message
        .get("type")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    Ok(match message_type {
        "auth_required" => ServerMessage::AuthRequired,
        "auth_ok" => ServerMessage::AuthOk,
        "auth_invalid" => ServerMessage::AuthInvalid {
            message: message
                .get("message")
                .and_then(JsonValue::as_str)
                .unwrap_or("the access token was rejected")
                .to_string(),
        },
        "event" => match message.pointer("/event/data/new_state") {
            Some(JsonValue::Object(_)) => ServerMessage::State(
                message
                    .pointer("/event/data/new_state")
                    .cloned()
                    .unwrap_or(JsonValue::Null),
            ),
            _ => ServerMessage::Ignored,
        },
        "result" => {
            let Some(id) = message.get("id").and_then(JsonValue::as_u64) else {
                return Err("a result arrived without an id".to_string());
            };
            let outcome = if message
                .get("success")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false)
            {
                Ok(message.get("result").cloned().unwrap_or(JsonValue::Null))
            } else {
                Err(result_error(&message))
            };
            ServerMessage::Result { id, outcome }
        }
        _ => ServerMessage::Ignored,
    })
}

fn result_error(message: &JsonValue) -> String {
    let reason = message
        .pointer("/error/message")
        .and_then(JsonValue::as_str)
        .unwrap_or("the request was refused");
    match message.pointer("/error/code").and_then(JsonValue::as_str) {
        Some(code) => format!("{reason} ({code})"),
        None => reason.to_string(),
    }
}

pub fn auth(token: &str) -> JsonValue {
    json!({ "type": "auth", "access_token": token })
}

pub fn subscribe_state_changed(id: u64) -> JsonValue {
    json!({ "id": id, "type": "subscribe_events", "event_type": "state_changed" })
}

pub fn get_states(id: u64) -> JsonValue {
    json!({ "id": id, "type": "get_states" })
}

/// The states carried by a `get_states` result.
pub fn states_of(result: &JsonValue) -> Vec<JsonValue> {
    result
        .as_array()
        .map(|states| {
            states
                .iter()
                .filter(|state| state.is_object())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCall {
    pub domain: String,
    pub service: String,
    pub entity_id: Option<String>,
    pub service_data: Map<String, JsonValue>,
}

impl ServiceCall {
    pub fn new(domain: impl Into<String>, service: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            service: service.into(),
            entity_id: None,
            service_data: Map::new(),
        }
    }

    pub fn message(&self, id: u64) -> JsonValue {
        let mut message = json!({
            "id": id,
            "type": "call_service",
            "domain": self.domain,
            "service": self.service,
        });
        let fields = message
            .as_object_mut()
            .expect("the message was built as an object");
        if !self.service_data.is_empty() {
            fields.insert(
                "service_data".to_string(),
                JsonValue::Object(self.service_data.clone()),
            );
        }
        if let Some(entity_id) = &self.entity_id {
            fields.insert("target".to_string(), json!({ "entity_id": entity_id }));
        }
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handshake_frames_are_recognised() {
        assert_eq!(
            parse_server_message(r#"{"type":"auth_required","ha_version":"2026.7.1"}"#),
            Ok(ServerMessage::AuthRequired)
        );
        assert_eq!(
            parse_server_message(r#"{"type":"auth_ok","ha_version":"2026.7.1"}"#),
            Ok(ServerMessage::AuthOk)
        );
        assert_eq!(
            parse_server_message(r#"{"type":"auth_invalid","message":"Invalid access token"}"#),
            Ok(ServerMessage::AuthInvalid {
                message: "Invalid access token".to_string()
            })
        );
    }

    #[test]
    fn a_state_changed_event_yields_the_new_state() {
        let payload = r#"{
            "id": 1,
            "type": "event",
            "event": {
                "event_type": "state_changed",
                "data": {
                    "entity_id": "light.kitchen",
                    "old_state": { "entity_id": "light.kitchen", "state": "off", "attributes": {} },
                    "new_state": {
                        "entity_id": "light.kitchen",
                        "state": "on",
                        "attributes": { "rgb_color": [232, 185, 35], "brightness": 200 }
                    }
                }
            }
        }"#;
        let ServerMessage::State(state) = parse_server_message(payload).expect("parses") else {
            panic!("a state_changed event carries a state");
        };
        assert_eq!(state["entity_id"], json!("light.kitchen"));
        assert_eq!(state["attributes"]["brightness"], json!(200));
    }

    #[test]
    fn a_removed_entity_carries_no_state_and_is_ignored() {
        let payload = r#"{
            "id": 1,
            "type": "event",
            "event": {
                "event_type": "state_changed",
                "data": { "entity_id": "light.kitchen", "new_state": null }
            }
        }"#;
        assert_eq!(parse_server_message(payload), Ok(ServerMessage::Ignored));
    }

    #[test]
    fn a_successful_result_carries_its_payload() {
        let payload = r#"{"id":2,"type":"result","success":true,"result":[{"entity_id":"light.kitchen","state":"on"}]}"#;
        let ServerMessage::Result { id, outcome } = parse_server_message(payload).expect("parses")
        else {
            panic!("a result frame parses as a result");
        };
        assert_eq!(id, 2);
        assert_eq!(states_of(&outcome.expect("succeeded")).len(), 1);
    }

    #[test]
    fn a_failed_result_reads_as_the_reason_home_assistant_gave() {
        let payload = r#"{"id":3,"type":"result","success":false,"error":{"code":"not_found","message":"Service light.explode not found"}}"#;
        assert_eq!(
            parse_server_message(payload),
            Ok(ServerMessage::Result {
                id: 3,
                outcome: Err("Service light.explode not found (not_found)".to_string()),
            })
        );
    }

    #[test]
    fn an_unknown_frame_is_ignored_rather_than_failing_the_connection() {
        assert_eq!(
            parse_server_message(r#"{"id":4,"type":"pong"}"#),
            Ok(ServerMessage::Ignored)
        );
        assert!(parse_server_message("not json").is_err());
    }

    #[test]
    fn the_commands_carry_the_shape_home_assistant_expects() {
        assert_eq!(
            auth("hunter2"),
            json!({ "type": "auth", "access_token": "hunter2" })
        );
        assert_eq!(
            subscribe_state_changed(1),
            json!({ "id": 1, "type": "subscribe_events", "event_type": "state_changed" })
        );
        assert_eq!(get_states(2), json!({ "id": 2, "type": "get_states" }));
    }

    #[test]
    fn a_service_call_targets_its_entity() {
        let mut call = ServiceCall::new("light", "turn_on");
        call.entity_id = Some("light.kitchen".to_string());
        call.service_data
            .insert("brightness_pct".to_string(), json!(40));

        assert_eq!(
            call.message(7),
            json!({
                "id": 7,
                "type": "call_service",
                "domain": "light",
                "service": "turn_on",
                "service_data": { "brightness_pct": 40 },
                "target": { "entity_id": "light.kitchen" },
            })
        );
    }

    #[test]
    fn a_service_call_without_a_target_omits_both_optional_fields() {
        assert_eq!(
            ServiceCall::new("script", "goodnight").message(8),
            json!({ "id": 8, "type": "call_service", "domain": "script", "service": "goodnight" })
        );
    }

    #[test]
    fn only_state_objects_survive_a_get_states_result() {
        let result = json!([{ "entity_id": "light.kitchen" }, "nonsense", null]);
        assert_eq!(states_of(&result).len(), 1);
        assert!(states_of(&json!(null)).is_empty());
    }
}
