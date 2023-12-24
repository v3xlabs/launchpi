use std::collections::HashMap;

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

    // -device specific starts here-

    async fn clear(&self) -> Result<(), ()>;
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
