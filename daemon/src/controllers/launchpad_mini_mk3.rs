use super::{Controller, DeviceInfo};

pub struct LaunchpadMiniMk3 {}

#[async_trait::async_trait]
impl Controller for LaunchpadMiniMk3 {
    fn from_connection(_device: &DeviceInfo) -> Result<Self, ()> {
        todo!()
    }

    fn detect_all() -> Result<Vec<DeviceInfo>, ()> {
        todo!()
    }

    async fn clear(&self) -> Result<(), ()> {
        todo!()
    }
}
