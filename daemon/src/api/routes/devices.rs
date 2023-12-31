use std::sync::Arc;

use axum::{Json, extract::State};
use midir::MidiInput;
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
    // let input = MidiInput::new("Launchpi").unwrap();

    // let devices = input
    //     .ports()
    //     .iter()
    //     .map(|port| Device {
    //         name: input.port_name(port).unwrap(),
    //     })
    //     .collect();

    let mut devices: Vec<(Box<dyn Controller>, bool)> = vec![];

    if let Ok(mk3) = LaunchpadMiniMk3::guess() {
        let connected = state.controllers.lock().unwrap().iter().any(|controller| {
            controller.name() == mk3.name()
        });
        devices.push((mk3, connected));
    }

    let devices = devices
        .into_iter()
        .map(|(device, connected)| Device {
            name: device.name().to_string(),
            connected,
        })
        .collect();

    Json(DevicesResult { devices })
}
