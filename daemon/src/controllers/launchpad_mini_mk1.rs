use std::sync::{Arc, Mutex};

use launchy::{InputDevice, InputDeviceHandlerPolling, MidiError, MsgPollingWrapper, OutputDevice};
use rand::Rng;
use tracing::info;

use crate::scripts::Script;

use super::{Controller, DeviceInfo};

pub struct LaunchpadMiniMk1 {
    midi_in: Arc<Mutex<InputDeviceHandlerPolling<launchy::mini::Message>>>,
    midi_out: Arc<Mutex<launchy::mini::Output>>,
}

#[async_trait::async_trait]
impl Controller for LaunchpadMiniMk1 {
    fn from_connection(_device: &DeviceInfo) -> Result<Box<Self>, ()> {
        todo!()
    }
    fn detect_all() -> Result<Vec<DeviceInfo>, ()> {
        todo!()
    }

    fn guess() -> Result<Box<Self>, MidiError> {
        let input = launchy::mini::Input::guess_polling()?;
        let output = launchy::mini::Output::guess()?;

        let midi_in = Arc::new(Mutex::new(input));
        let midi_out = Arc::new(Mutex::new(output));

        Ok(Box::new(Self { midi_in, midi_out }))
    }

    fn initialize(&self) -> Result<(), MidiError> {
        self.clear().unwrap();

        // Wait for 1 second
        // tokio::time::sleep(Duration::from_millis(10)).await;

        Ok(())
    }

    fn run(&self, script: &impl Script) -> Result<(), MidiError> {
        let midi_in = self.midi_in.lock().unwrap();

        for message in midi_in.iter() {
            match message {
                launchy::mini::Message::Press { button } => match button {
                    launchy::mini::Button::GridButton { x, y } => {
                        script.on_press(x, y, self);
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        Ok(())
    }

    fn clear(&self) -> Result<(), MidiError> {
        let mut midi_out = self.midi_out.lock().unwrap();

        midi_out.light_all(launchy::mini::Color::BLACK)
    }

    fn name(&self) -> &str {
        "Launchpad Mini Mk1"
    }

    fn set_button_color(&self, x: u8, y: u8, color: u8) -> Result<(), MidiError> {
        let mut midi_out = self.midi_out.lock().unwrap();

        midi_out.light(
            launchy::mini::Button::GridButton { x, y },
            launchy::mini::Color::GREEN,
        )
    }
}
