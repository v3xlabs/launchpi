use std::time::Duration;

use launchy::{InputDevice, InputDeviceHandlerPolling, MidiError, OutputDevice};
use rand::Rng;

use super::{Controller, DeviceInfo};

pub struct LaunchpadMiniMk1 {
    midi_in: InputDeviceHandlerPolling<launchy::mini::Message>,
    midi_out: launchy::mini::Output,
}

unsafe impl Send for LaunchpadMiniMk1 {}
unsafe impl Sync for LaunchpadMiniMk1 {}

#[async_trait::async_trait]
impl Controller for LaunchpadMiniMk1 {
    fn from_connection(_device: &DeviceInfo) -> Result<Self, ()> {
        todo!()
    }
    fn detect_all() -> Result<Vec<DeviceInfo>, ()> {
        todo!()
    }

    async fn guess() -> Result<Self, MidiError> {
        let midi_in = launchy::mini::Input::guess_polling()?;
        let midi_out = launchy::mini::Output::guess()?;

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
            launchy::mini::Color::ORANGE,
        )?;

        Ok(())
    }

    async fn run(&mut self) -> Result<(), MidiError> {
        println!("Device found");

        Ok(())
    }

    fn clear(&mut self) -> Result<(), MidiError> {
        self.midi_out.light_all(launchy::mini::Color::BLACK)
    }

    fn name(&self) -> &str {
        "Launchpad Mini Mk1"
    }
}
