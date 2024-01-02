use std::sync::Arc;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{
    controllers::{
        launchpad_mini_mk1::LaunchpadMiniMk1, launchpad_mini_mk3::LaunchpadMiniMk3, Controller,
    },
    state::AppState,
};

#[derive(Serialize, Deserialize)]
pub struct Device {
    pub name: String,
    pub id: String,
    pub connected: bool,
}

#[derive(Serialize, Deserialize)]
pub struct DevicesResult {
    pub devices: Vec<Device>,
}

pub async fn get(State(state): State<Arc<AppState>>) -> Json<DevicesResult> {
    let mut devices: Vec<(&str, &str, bool)> = vec![];

    if let Ok(mk3) = LaunchpadMiniMk3::guess_ok() {
        let controllers = state.controllers.lock().unwrap();
        let connected = controllers
            .iter()
            .any(|controller| controller.name() == "Launchpad Mini Mk3");
        drop(controllers);
        devices.push(("Launchpad Mini Mk3", "launchpad_mini_mk3_0", connected));
    }

    if let Ok(mk1) = LaunchpadMiniMk1::guess_ok() {
        let controllers = state.controllers.lock().unwrap();
        let connected = controllers
            .iter()
            .any(|controller| controller.name() == "Launchpad Mini Mk1");
        drop(controllers);
        devices.push(("Launchpad Mini Mk1", "launchpad_mini_mk1_0", connected));
    }

    let devices = devices
        .into_iter()
        .map(|(name, id, connected)| Device {
            name: name.into(),
            id: id.into(),
            connected,
        })
        .collect();

    Json(DevicesResult { devices })
}
