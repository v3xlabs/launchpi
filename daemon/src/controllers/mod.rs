use std::collections::HashMap;

use launchy::MidiError;

use self::{launchpad_mini_mk1::LaunchpadMiniMk1, launchpad_mini_mk3::LaunchpadMiniMk3};

mod launchpad_mini_mk1;
mod launchpad_mini_mk3;

#[async_trait::async_trait]
pub trait Controller: Send + Sync + 'static {
    fn from_connection(device: &DeviceInfo) -> Result<Self, ()>
    where
        Self: Sized;

    fn detect_all() -> Result<Vec<DeviceInfo>, ()>
    where
        Self: Sized;

    async fn guess() -> Result<Self, MidiError>
    where
        Self: Sized;

    // -device specific starts here-

    async fn initialize(&mut self) -> Result<(), MidiError> where Self: Sized {
        Ok(())
    }

    async fn run(&mut self) -> Result<(), MidiError>
    where
        Self: Sized;

    fn clear(&mut self) -> Result<(), MidiError>;

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

pub async fn guess() -> Result<Box<dyn Controller>, ()> {
    if let Ok(mut t) = LaunchpadMiniMk3::guess().await {
        t.initialize().await.unwrap();
        return Ok(Box::new(t));
    }
    if let Ok(mut t) = LaunchpadMiniMk1::guess().await {
        t.initialize().await.unwrap();
        return Ok(Box::new(t));
    }

    Err(())
}
