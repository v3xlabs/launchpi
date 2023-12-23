use super::Controller;


pub struct LaunchpadMiniMk3 {}

impl Controller for LaunchpadMiniMk3 {
    fn from_connection(connection: T) -> Result<Self, ()> {
        unimplemented!()
    }

    async fn clear() -> Result<(), ()> {
        unimplemented!()
    }
}
