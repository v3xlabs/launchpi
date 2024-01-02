use std::sync::{Arc, Mutex};

use crate::scripts::Script;

use super::{Alles, Controller, ControllerEvent, ScriptRunner};
use launchy::{
    launchpad_mini_mk3::PaletteColor, InputDevice, InputDeviceHandlerPolling, MidiError,
    MsgPollingWrapper, OutputDevice,
};
use tracing::info;

pub struct LaunchpadMiniMk3 {
    midi_in: Arc<Mutex<InputDeviceHandlerPolling<launchy::mini_mk3::Message>>>,
    midi_out: Arc<Mutex<launchy::mini_mk3::Output>>,
    event_sender: Arc<Mutex<tokio::sync::broadcast::Sender<ControllerEvent>>>,
    event_receiver: tokio::sync::broadcast::Receiver<ControllerEvent>,
}

#[async_trait::async_trait]
impl Controller for LaunchpadMiniMk3 {
    fn guess() -> Result<Box<Self>, MidiError> {
        let midi_in = Arc::new(Mutex::new(launchy::mini_mk3::Input::guess_polling()?));
        let midi_out = Arc::new(Mutex::new(launchy::mini_mk3::Output::guess()?));
        let (event_sender, event_receiver) = tokio::sync::broadcast::channel(10);

        // Mock receiver magically works lmao
        // tokio::spawn(async move {
        //     loop {
        //         let message = event_receiver.recv().await.unwrap();
        //         info!("Idle Received message: {:?}", message);
        //     }
        // });

        Ok(Box::new(Self {
            midi_in,
            midi_out,
            event_receiver,
            event_sender: Arc::new(Mutex::new(event_sender)),
        }))
    }

    fn guess_ok() -> Result<(), MidiError> {
        launchy::mini_mk3::Input::guess_polling()?;
        launchy::mini_mk3::Output::guess()?;

        Ok(())
    }

    fn initialize(&self) -> Result<(), MidiError> {
        self.clear().unwrap();

        let sender = self.event_sender.clone();
        let midi_in = self.midi_in.clone();

        tokio::spawn(async move {
            info!("Starting midi_in loop");

            let mut sender = sender.lock().unwrap();
            let midi_in = midi_in.lock().unwrap();

            while let message = midi_in.recv() {
                info!("MIDI OPERATION");

                // sender.send("value".to_string()).unwrap();

                match message {
                    launchy::mini_mk3::Message::Press { button } => match button {
                        launchy::mini_mk3::Button::GridButton { x, y } => {
                            info!("Midi -> send press event");
                            if let Err(error) = sender.send(ControllerEvent::Press { x, y: y + 1 })
                            {
                                info!("Error sending event: {}", error);
                            }
                        }
                        launchy::mini_mk3::Button::ControlButton { index } => {
                            info!("Midi -> send control press event {}", index);
                            let (x, y) = match index {
                                0..=7 => (index, 0),
                                8..=u8::MAX => (8, index - 7), // TODO: this is 7 due to the light, adjust later when launchy is updated
                            };
                            if let Err(error) = sender.send(ControllerEvent::Press { x, y }) {
                                info!("Error sending event: {}", error);
                            }
                        }
                    },
                    launchy::launchpad_mini_mk3::Message::Release { button } => match button {
                        launchy::launchpad_mini_mk3::Button::GridButton { x, y } => {
                            info!("Midi -> send release event");
                            if let Err(error) =
                                sender.send(ControllerEvent::Release { x, y: y + 1 })
                            {
                                info!("Error sending event: {}", error);
                            }
                        }
                        launchy::mini_mk3::Button::ControlButton { index } => {
                            info!("Midi -> send control press event {}", index);
                            let (x, y) = match index {
                                0..=7 => (index, 0),
                                8..=u8::MAX => (8, index - 7), // TODO: this is 7 due to the light, adjust later when launchy is updated
                            };
                            if let Err(error) = sender.send(ControllerEvent::Release { x, y }) {
                                info!("Error sending event: {}", error);
                            }
                        }
                    },
                    _ => {}
                }
            }
        });

        Ok(())
    }

    fn clear(&self) -> Result<(), MidiError> {
        let mut midi_out = self.midi_out.lock().unwrap();
        midi_out.clear()
    }

    fn get_event_receiver(&self) -> Result<tokio::sync::broadcast::Receiver<ControllerEvent>, ()> {
        info!("Getting event receiver");

        // let event_sender = self.event_sender.clone();
        // tokio::spawn(async move {
        //     // wait 2 seconds
        //     tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        //     event_sender.send(ControllerEvent::Heartbeat).unwrap();
        // });

        Ok(self.event_receiver.resubscribe())
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
        // script.initialize(self);

        // let midi_in = self.midi_in.lock().unwrap();

        // for message in midi_in.iter() {
        //     match message {
        //         launchy::mini_mk3::Message::Press { button } => match button {
        //             launchy::mini_mk3::Button::GridButton { x, y } => {
        //                 script.on_press(x, y, self);
        //             }
        //             _ => {}
        //         },
        //         launchy::launchpad_mini_mk3::Message::Release { button } => match button {
        //             launchy::launchpad_mini_mk3::Button::GridButton { x, y } => {
        //                 script.on_release(x, y, self);
        //             }
        //             _ => {}
        //         },
        //         _ => {}
        //     }
        // }

        Ok(())
    }
}

impl Alles for LaunchpadMiniMk3 {}
