use std::sync::{Arc, Mutex};

use launchy::{InputDevice, InputDeviceHandlerPolling, MidiError, MsgPollingWrapper, OutputDevice};

use crate::scripts::Script;

use super::{Alles, Controller, ScriptRunner};

pub struct LaunchpadMiniMk1 {
    midi_in: Arc<Mutex<InputDeviceHandlerPolling<launchy::mini::Message>>>,
    midi_out: Arc<Mutex<launchy::mini::Output>>,
}

#[async_trait::async_trait]
impl Controller for LaunchpadMiniMk1 {
    fn guess() -> Result<Box<Self>, MidiError> {
        let input = launchy::mini::Input::guess_polling()?;
        let output = launchy::mini::Output::guess()?;

        let midi_in = Arc::new(Mutex::new(input));
        let midi_out = Arc::new(Mutex::new(output));

        Ok(Box::new(Self { midi_in, midi_out }))
    }

    fn guess_ok() -> Result<(), MidiError> {
        launchy::mini::Input::guess_polling()?;
        launchy::mini::Output::guess()?;

        Ok(())
    }

    fn initialize(&self) -> Result<(), MidiError> {
        self.clear().unwrap();

        // Wait for 1 second
        // tokio::time::sleep(Duration::from_millis(10)).await;

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

        let color = match color {
            0 => launchy::mini::Color::BLACK,
            1 => launchy::mini::Color::RED,
            2 => launchy::mini::Color::GREEN,
            3 => launchy::mini::Color::AMBER,
            4 => launchy::mini::Color::DIM_GREEN,
            5 => launchy::mini::Color::ORANGE,
            6 => launchy::mini::Color::YELLOW,
            _ => launchy::mini::Color::AMBER,
        };

        midi_out.light(launchy::mini::Button::GridButton { x, y }, color)
    }
}

impl ScriptRunner for LaunchpadMiniMk1 {
    fn run(&self, script: &mut dyn Script) -> Result<(), MidiError> {
        script.initialize(self);

        let midi_in = self.midi_in.lock().unwrap();

        for message in midi_in.iter() {
            match message {
                launchy::mini::Message::Press { button } => match button {
                    launchy::mini::Button::GridButton { x, y } => {
                        script.on_press(x, y, self);
                    }
                    _ => {}
                },
                launchy::mini::Message::Release { button } => match button {
                    launchy::mini::Button::GridButton { x, y } => {
                        script.on_release(x, y, self);
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        Ok(())
    }
}

impl Alles for LaunchpadMiniMk1 {}
