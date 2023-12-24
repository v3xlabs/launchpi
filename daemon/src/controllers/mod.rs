use std::collections::HashMap;

use launchy::MidiError;

use crate::scripts::Script;

pub mod launchpad_mini_mk1;
pub mod launchpad_mini_mk3;

#[async_trait::async_trait]
pub trait Controller: Send {
    fn from_connection(device: &DeviceInfo) -> Result<Box<Self>, ()>;

    fn detect_all() -> Result<Vec<DeviceInfo>, ()>;

    fn guess() -> Result<Box<Self>, MidiError>;

    // -device specific starts here-

    fn initialize(&self) -> Result<(), MidiError> {
        Ok(())
    }

    fn run(&self, script: &impl Script) -> Result<(), MidiError>;

    fn clear(&self) -> Result<(), MidiError>;

    fn set_button_color(&self, x: u8, y: u8, color: u8) -> Result<(), MidiError>;

    fn name(&self) -> &str;
}

#[allow(dead_code)]
pub struct DeviceInfo {
    name: String,
    port: u16,

    extra_data: HashMap<String, String>,
}

#[allow(dead_code)]
pub fn list_devices() -> Result<Vec<DeviceInfo>, ()> {
    let list = vec![];

    Ok(list)
}

