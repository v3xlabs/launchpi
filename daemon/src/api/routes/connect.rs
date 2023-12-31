use std::sync::Arc;

use axum::{extract::State, Json};
use midir::MidiInput;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    controllers::{launchpad_mini_mk3::LaunchpadMiniMk3, Controller},
    state::AppState,
};

#[derive(Serialize, Deserialize)]
pub struct Device {
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConnectResult {
    // pub devices: Vec<Device>,
}

pub async fn post(State(state): State<Arc<AppState>>) -> Json<ConnectResult> {
    if let Ok(mk3) = LaunchpadMiniMk3::guess() {
        let connected = state
            .controllers
            .lock()
            .unwrap()
            .iter()
            .any(|controller| controller.name() == mk3.name());
        if !connected {
            info!("Adding controller");
            state.controller_tx.send(Arc::new(mk3)).await.unwrap();
            // state.controllers.lock().unwrap().push(Arc::new(mk3));
        }
    }

    Json(ConnectResult {})
}
