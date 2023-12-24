use std::time::Duration;

use super::{Controller, DeviceInfo};
use launchy::{InputDevice, InputDeviceHandlerPolling, MidiError, OutputDevice};
use rand::Rng;

pub struct LaunchpadMiniMk3 {
    midi_in: InputDeviceHandlerPolling<launchy::mini_mk3::Message>,
    midi_out: launchy::mini_mk3::Output,
}

unsafe impl Send for LaunchpadMiniMk3 {}

unsafe impl Sync for LaunchpadMiniMk3 {}

#[async_trait::async_trait]
impl Controller for LaunchpadMiniMk3 {
    fn from_connection(_device: &DeviceInfo) -> Result<Self, ()> {
        todo!()
    }

    fn detect_all() -> Result<Vec<DeviceInfo>, ()> {
        todo!()
    }

    async fn guess() -> Result<Self, MidiError> {
        let midi_in = launchy::mini_mk3::Input::guess_polling()?;
        let midi_out = launchy::mini_mk3::Output::guess()?;

        Ok(Self { midi_in, midi_out })
    }

    async fn initialize(&mut self) -> Result<(), MidiError>
    where
        Self: Sized,
    {
        self.clear().unwrap();

        // Wait for 1 second
        tokio::time::sleep(Duration::from_millis(10)).await;

        let mut rng = rand::thread_rng();
        let random_x = rng.gen_range(0..8);
        let random_y = rng.gen_range(0..8);

        self.midi_out.light(
            launchy::mini::Button::GridButton {
                x: random_x,
                y: random_y,
            },
            launchy::mini_mk3::PaletteColor::MAGENTA,
        )?;

        Ok(())
    }

    async fn run(&mut self) -> Result<(), MidiError> {
        println!("Device found");

        Ok(())
    }

    fn clear(&mut self) -> Result<(), MidiError> {
        self.midi_out.clear()
    }

    fn name(&self) -> &str {
        "Launchpad Mini Mk3"
    }
}
