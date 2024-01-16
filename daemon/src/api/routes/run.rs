use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    controllers::Alles,
    scripts::{soundboard::SoundboardScript, Script},
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
    let state = state.clone();
    let controllers = state.controllers.lock().unwrap().clone();
    let first_controller: Option<Arc<Box<dyn Alles>>> = controllers
        .iter()
        .find(|controller| match controller.name() {
            "Launchpad Mini Mk1" => device_id == "launchpad_mini_mk1_0",
            "Launchpad Mini Mk3" => device_id == "launchpad_mini_mk3_0",
            _ => false,
        })
        .cloned();
    drop(controllers);

    if let Some(controller) = first_controller {
        info!("Controller found");

        let mut running_scripts = state.running_scripts.lock().unwrap();

        let running_script = running_scripts.get(&device_id);

        if let Some(running_script) = running_script {
            info!("Script already running, dropping");
            running_script.abort();
        }

        // controller
        let mut script = SoundboardScript::new();

        let x = tokio::spawn(async move {
            controller.run(&mut script).await.unwrap();
        });

        running_scripts.insert(device_id, x);
    }

    Json(ConnectResult {})
}
