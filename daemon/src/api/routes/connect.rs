use axum::Json;
use midir::MidiInput;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Device {
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConnectResult {
    // pub devices: Vec<Device>,
}

pub async fn post() -> Json<ConnectResult> {
    let input = MidiInput::new("Launchpi").unwrap();

    Json(ConnectResult { })
}
