use axum::{extract::State, routing::get, Json, Router};
use serde::Deserialize;

use crate::{
    api::error::ApiError,
    config::{changes::ConfigurationChanges, ExportFormat},
    state::AppState,
};

#[derive(Deserialize)]
pub struct ExportQuery {
    #[serde(default)]
    pub format: ExportFormat,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/config/changes", get(configuration_changes))
}

async fn configuration_changes(
    State(state): State<AppState>,
) -> Result<Json<ConfigurationChanges>, ApiError> {
    state
        .configuration_changes()
        .map(Json)
        .map_err(ApiError::internal)
}
