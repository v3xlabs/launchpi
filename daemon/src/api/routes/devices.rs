use axum::Json;
use midir::MidiInput;
use serde::{Deserialize, Serialize};

use crate::controllers::{launchpad_mini_mk3::LaunchpadMiniMk3, Controller};

#[derive(Serialize, Deserialize)]
pub struct Device {
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct DevicesResult {
    pub devices: Vec<Device>,
}

pub async fn get() -> Json<DevicesResult> {
    // let input = MidiInput::new("Launchpi").unwrap();

    // let devices = input
    //     .ports()
    //     .iter()
    //     .map(|port| Device {
    //         name: input.port_name(port).unwrap(),
    //     })
    //     .collect();

    let mut devices: Vec<Box<dyn Controller>> = vec![];

    if let Ok(mk3) = LaunchpadMiniMk3::guess() {
        mk3.initialize().unwrap();
        mk3.clear().unwrap();
        devices.push(mk3);
    }

    let devices = devices
        .into_iter()
        .map(|device| Device {
            name: device.name().to_string(),
        })
        .collect();

    Json(DevicesResult { devices })
}
