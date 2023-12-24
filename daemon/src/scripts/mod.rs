use crate::controllers::Controller;

pub mod ping;

pub trait Script: Send {
    fn name(&self) -> &'static str;

    fn new() -> Self;

    fn on_press(&self, x: u8, y: u8, controller: &impl Controller);
}
