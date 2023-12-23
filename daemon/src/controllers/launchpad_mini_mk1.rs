use super::Controller;


pub struct LaunchpadMiniMk1 {}

impl Controller for LaunchpadMiniMk1 {
    fn from_connection(connection: T) -> Result<Self, ()> {
        unimplemented!()
    }

    async fn clear() -> Result<(), ()> {
        unimplemented!()
    }

    async fn detect_all() -> Result<Vec<DeviceInfo>, ()> {
        unimplemented!()
    }
}
