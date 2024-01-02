use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    controllers::{
        launchpad_mini_mk1::LaunchpadMiniMk1, launchpad_mini_mk3::LaunchpadMiniMk3, Controller,
    },
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

pub async fn post(
    Path(device_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Json<ConnectResult> {
    match device_id.as_str() {
        "launchpad_mini_mk1_0" => {
            if let Ok(mk1) = LaunchpadMiniMk1::guess() {
                let connected = state
                    .controllers
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|controller| controller.name() == mk1.name());
                if !connected {
                    info!("Adding controller");
                    state.controller_tx.send(Arc::new(mk1)).await.unwrap();
                    // state.controllers.lock().unwrap().push(Arc::new(mk3));
                }
            }
        }
        "launchpad_mini_mk3_0" => {
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
        }
        _ => {
            return Json(ConnectResult {});
        }
    }

    Json(ConnectResult {})
}
