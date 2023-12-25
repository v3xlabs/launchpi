use std::sync::{Arc, Mutex};

use crate::scripts::Script;

use super::{Alles, Controller, DeviceInfo, ScriptRunner};
use launchy::{
    launchpad_mini_mk3::PaletteColor, InputDevice, InputDeviceHandlerPolling, MidiError,
    MsgPollingWrapper, OutputDevice,
};

pub struct LaunchpadMiniMk3 {
    midi_in: Arc<Mutex<InputDeviceHandlerPolling<launchy::mini_mk3::Message>>>,
    midi_out: Arc<Mutex<launchy::mini_mk3::Output>>,
}

#[async_trait::async_trait]
impl Controller for LaunchpadMiniMk3 {
    fn from_connection(_device: &DeviceInfo) -> Result<Box<Self>, ()> {
        todo!()
    }

    fn detect_all() -> Result<Vec<DeviceInfo>, ()> {
        todo!()
    }

    fn guess() -> Result<Box<Self>, MidiError> {
        let midi_in = Arc::new(Mutex::new(launchy::mini_mk3::Input::guess_polling()?));
        let midi_out = Arc::new(Mutex::new(launchy::mini_mk3::Output::guess()?));

        Ok(Box::new(Self { midi_in, midi_out }))
    }

    fn initialize(&self) -> Result<(), MidiError> {
        self.clear().unwrap();

        Ok(())
    }

    fn clear(&self) -> Result<(), MidiError> {
        let mut midi_out = self.midi_out.lock().unwrap();
        midi_out.clear()
    }

    fn name(&self) -> &str {
        "Launchpad Mini Mk3"
    }

    fn set_button_color(&self, x: u8, y: u8, color: u8) -> Result<(), MidiError> {
        let mut midi_out: std::sync::MutexGuard<'_, launchy::launchpad_mini_mk3::Output> =
            self.midi_out.lock().unwrap();

        let color = match color {
            0 => PaletteColor::BLACK,
            // 1 => PaletteColor::DARK_GRAY,
            // 2 => PaletteColor::LIGHT_GRAY,
            1 => PaletteColor::WHITE,
            2 => PaletteColor::RED,
            3 => PaletteColor::YELLOW,
            4 => PaletteColor::BLUE,
            5 => PaletteColor::MAGENTA,
            6 => PaletteColor::BROWN,
            7 => PaletteColor::CYAN,
            8 => PaletteColor::GREEN,
            _ => PaletteColor::BLACK,
        };

        midi_out.light(launchy::mini_mk3::Button::GridButton { x, y }, color)
    }
}

impl ScriptRunner for LaunchpadMiniMk3 {
    fn run(&self, script: &mut dyn Script) -> Result<(), MidiError> {
        script.initialize(self);

        let midi_in = self.midi_in.lock().unwrap();

        for message in midi_in.iter() {
            match message {
                launchy::mini_mk3::Message::Press { button } => match button {
                    launchy::mini_mk3::Button::GridButton { x, y } => {
                        script.on_press(x, y, self);
                    }
                    _ => {}
                },
                launchy::launchpad_mini_mk3::Message::Release { button } => match button {
                    launchy::launchpad_mini_mk3::Button::GridButton { x, y } => {
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

impl Alles for LaunchpadMiniMk3 {}
