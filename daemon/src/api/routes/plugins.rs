use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    api::error::ApiError,
    config::values::UserValue,
    identifiers::IntegrationId,
    plugins::{
        instance::PluginInstance,
        manifest::{ConfigField, PluginManifest},
        plugin::LookupOption,
    },
    state::AppState,
    variables::{VariableRef, VariableValue},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/plugins", get(list_plugins).post(create_instance))
        .route(
            "/api/plugins/:integration_id",
            axum::routing::patch(update_instance).delete(delete_instance),
        )
        .route("/api/plugins/:integration_id/config", get(export_instance))
        .route(
            "/api/plugins/:integration_id/lookup/:source",
            get(lookup_options),
        )
        .route(
            "/api/plugins/:integration_id/variables",
            get(list_variables),
        )
        .route(
            "/api/plugins/:integration_id/actions/:action_name",
            post(run_action),
        )
        .route("/api/devices/:surface_id/config", get(export_device))
        .route("/api/config/export", get(export_configuration))
        .route("/api/values", get(list_all_values).post(upsert_user_value))
        .route(
            "/api/values/:name",
            axum::routing::delete(delete_user_value),
        )
}

/// Everything the daemon currently knows, from every source, plus the catalogue of what can be
/// done with it. One request backs the whole Values page.
#[derive(Serialize)]
struct ValueCatalogue {
    values: Vec<VariableEntry>,
    user_values: Vec<UserValue>,
    actions: Vec<AvailableAction>,
}

#[derive(Serialize)]
struct AvailableAction {
    integration_id: IntegrationId,
    instance_name: String,
    name: String,
    label: String,
    description: Option<String>,
    parameters: Vec<ConfigField>,
}

async fn list_all_values(State(state): State<AppState>) -> Json<ValueCatalogue> {
    let manifests = state.plugins.manifests();
    let actions = state
        .plugins
        .instances()
        .into_iter()
        .flat_map(|instance| {
            let manifest = manifests
                .iter()
                .find(|manifest| manifest.plugin_type == instance.plugin_type);
            manifest
                .map(|manifest| manifest.actions.clone())
                .unwrap_or_default()
                .into_iter()
                .map(move |action| AvailableAction {
                    integration_id: instance.integration_id.clone(),
                    instance_name: instance.display_name.clone(),
                    name: action.name,
                    label: action.label,
                    description: action.description,
                    parameters: action.parameters,
                })
        })
        .collect();

    Json(ValueCatalogue {
        values: state
            .plugins
            .variable_snapshot()
            .into_iter()
            .map(entry_of)
            .collect(),
        user_values: state.plugins.user_values(),
        actions,
    })
}

async fn upsert_user_value(
    State(state): State<AppState>,
    Json(value): Json<UserValue>,
) -> Result<StatusCode, ApiError> {
    state
        .plugins
        .set_user_value(value)
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(ApiError::bad_request)
}

async fn delete_user_value(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .plugins
        .remove_user_value(&name)
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(|reason| ApiError {
            status: StatusCode::NOT_FOUND,
            message: reason,
        })
}

#[derive(Serialize)]
struct PluginCatalogue {
    types: Vec<PluginManifest>,
    instances: Vec<PluginInstance>,
}

#[derive(Deserialize)]
struct CreateInstanceRequest {
    plugin_type: String,
    name: String,
    display_name: Option<String>,
    #[serde(default)]
    config: JsonValue,
}

#[derive(Deserialize)]
struct UpdateInstanceRequest {
    is_enabled: Option<bool>,
    display_name: Option<String>,
    config: Option<JsonValue>,
}

#[derive(Serialize)]
struct VariableEntry {
    integration_id: IntegrationId,
    name: String,
    value: VariableValue,
    rendered: String,
}

async fn list_plugins(State(state): State<AppState>) -> Json<PluginCatalogue> {
    Json(PluginCatalogue {
        types: state.plugins.manifests(),
        instances: state.plugins.instances(),
    })
}

async fn create_instance(
    State(state): State<AppState>,
    Json(request): Json<CreateInstanceRequest>,
) -> Result<Json<PluginInstance>, ApiError> {
    let config = json_to_table(&request.config)?;
    state
        .plugins
        .create_instance(
            request.plugin_type.trim(),
            request.name.trim(),
            request.display_name,
            config,
        )
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn update_instance(
    State(state): State<AppState>,
    Path(integration_id): Path<String>,
    Json(request): Json<UpdateInstanceRequest>,
) -> Result<Json<PluginInstance>, ApiError> {
    let config = request.config.as_ref().map(json_to_table).transpose()?;
    state
        .plugins
        .update_instance(
            &IntegrationId(integration_id),
            request.is_enabled,
            request.display_name,
            config,
        )
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

async fn delete_instance(
    State(state): State<AppState>,
    Path(integration_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .plugins
        .delete_instance(&IntegrationId(integration_id))
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(|reason| ApiError {
            status: StatusCode::NOT_FOUND,
            message: reason,
        })
}

async fn export_instance(
    State(state): State<AppState>,
    Path(integration_id): Path<String>,
) -> Result<String, ApiError> {
    state
        .plugins
        .export_instance(&IntegrationId(integration_id))
        .ok_or_else(|| ApiError::not_found("plugin instance"))?
        .map_err(ApiError::bad_request)
}

/// Backs the combobox on a lookup field: real choices from the running instance, with free text
/// still accepted so a raw identifier keeps working.
async fn lookup_options(
    State(state): State<AppState>,
    Path((integration_id, source)): Path<(String, String)>,
) -> Result<Json<Vec<LookupOption>>, ApiError> {
    state
        .plugins
        .lookup(&IntegrationId(integration_id), &source)
        .await
        .map(Json)
        .map_err(|error| ApiError::bad_request(error.to_string()))
}

async fn list_variables(
    State(state): State<AppState>,
    Path(integration_id): Path<String>,
) -> Json<Vec<VariableEntry>> {
    let integration_id = IntegrationId(integration_id);
    Json(
        state
            .plugins
            .variable_snapshot()
            .into_iter()
            .filter(|(reference, _)| reference.integration_id == integration_id)
            .map(entry_of)
            .collect(),
    )
}

fn entry_of((reference, value): (VariableRef, VariableValue)) -> VariableEntry {
    VariableEntry {
        integration_id: reference.integration_id,
        name: reference.name,
        rendered: value.to_string(),
        value,
    }
}

/// Fires one action from the UI so a binding can be checked without pressing a physical key.
async fn run_action(
    State(state): State<AppState>,
    Path((integration_id, action_name)): Path<(String, String)>,
    Json(parameters): Json<JsonValue>,
) -> Result<StatusCode, ApiError> {
    state
        .plugins
        .invoke(&IntegrationId(integration_id), &action_name, &parameters)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(|error| ApiError::bad_request(error.to_string()))
}

async fn export_device(
    State(state): State<AppState>,
    Path(surface_id): Path<String>,
) -> Result<String, ApiError> {
    state
        .export_device_configuration(&surface_id)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("device"))
}

async fn export_configuration(State(state): State<AppState>) -> Result<String, ApiError> {
    state
        .export_configuration()
        .map_err(ApiError::internal)
        .map(|document| {
            let instances: Vec<_> = state
                .plugins
                .instances()
                .into_iter()
                .filter_map(|instance| {
                    let exported = state
                        .plugins
                        .export_instance(&instance.integration_id)?
                        .ok()?;
                    Some(format!(
                        "# plugins/{}.{}.toml\n{exported}",
                        instance.plugin_type, instance.name
                    ))
                })
                .collect();
            if instances.is_empty() {
                document
            } else {
                format!("{document}\n{}", instances.join("\n"))
            }
        })
}

/// TOML has no null, so a form that left a field blank sends one and it is dropped rather than
/// rejected. Everything else round-trips through serde.
fn json_to_table(value: &JsonValue) -> Result<toml::Table, ApiError> {
    let JsonValue::Object(fields) = value else {
        if value.is_null() {
            return Ok(toml::Table::new());
        }
        return Err(ApiError::bad_request(
            "configuration must be an object".to_string(),
        ));
    };
    let mut table = toml::Table::new();
    for (key, field) in fields {
        if field.is_null() {
            continue;
        }
        let converted = toml::Value::try_from(field).map_err(|error| {
            ApiError::bad_request(format!("{key} cannot be stored in TOML: {error}"))
        })?;
        table.insert(key.clone(), converted);
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_form_field_is_dropped_rather_than_rejected() {
        let table = json_to_table(&serde_json::json!({
            "base_url": "http://local",
            "authorization": null,
        }))
        .map_err(|error| error.message)
        .expect("converts");
        assert!(table.contains_key("base_url"));
        assert!(!table.contains_key("authorization"));
    }

    #[test]
    fn nested_configuration_survives_the_conversion() {
        let table = json_to_table(&serde_json::json!({
            "poll": [{ "name": "value", "path": "/data.json", "interval_ms": 1000 }],
        }))
        .map_err(|error| error.message)
        .expect("converts");
        let rendered = toml::to_string_pretty(&table).expect("serializes");
        assert!(rendered.contains("[[poll]]"));
        assert!(rendered.contains("interval_ms = 1000"));
    }

    #[test]
    fn a_non_object_configuration_is_rejected() {
        assert!(json_to_table(&serde_json::json!("nope")).is_err());
    }

    #[test]
    fn an_absent_configuration_is_an_empty_table() {
        assert!(json_to_table(&JsonValue::Null)
            .map_err(|error| error.message)
            .expect("converts")
            .is_empty());
    }
}
