use std::sync::Arc;

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::{controllers::{launchpad_mini_mk3::LaunchpadMiniMk3, Controller}, state::AppState};

#[derive(Serialize, Deserialize)]
pub struct Device {
    pub name: String,
    pub connected: bool,
}

#[derive(Serialize, Deserialize)]
pub struct DevicesResult {
    pub devices: Vec<Device>,
}

pub async fn get(
    State(state): State<Arc<AppState>>,
) -> Json<DevicesResult> {
    let mut devices: Vec<(&str, bool)> = vec![];

    if let Ok(mk3) = LaunchpadMiniMk3::guess_ok() {
        let controllers = state.controllers.lock().unwrap();
        let connected = controllers.iter().any(|controller| {
            controller.name() == "Launchpad Mini Mk3"
        });
        drop(controllers);
        devices.push(("Launchpad Mini Mk3", connected));
    }

    let devices = devices
        .into_iter()
        .map(|(device, connected)| Device {
            name: device.to_string(),
            connected,
        })
        .collect();

    Json(DevicesResult { devices })
}
