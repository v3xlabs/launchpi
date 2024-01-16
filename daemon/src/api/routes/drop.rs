use std::sync::Arc;

use axum::{extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Serialize, Deserialize)]
pub struct Device {
    pub name: String,
}

pub async fn get(State(state): State<Arc<AppState>>) -> StatusCode {
    state.shutdown_tx.send(()).await.ok();

    StatusCode::OK
}
