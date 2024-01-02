use std::sync::{Arc, Mutex};

use launchy::{InputDevice, InputDeviceHandlerPolling, MidiError, MsgPollingWrapper, OutputDevice};
use tokio::sync::broadcast::error::TryRecvError;
use tracing::info;

use crate::scripts::Script;

use super::{Alles, Controller, ControllerEvent, ScriptRunner};

pub struct LaunchpadMiniMk1 {
    midi_in: Arc<Mutex<InputDeviceHandlerPolling<launchy::mini::Message>>>,
    midi_out: Arc<Mutex<launchy::mini::Output>>,
    event_sender: Arc<Mutex<tokio::sync::broadcast::Sender<ControllerEvent>>>,
    event_receiver: tokio::sync::broadcast::Receiver<ControllerEvent>,
}

#[async_trait::async_trait]
impl Controller for LaunchpadMiniMk1 {
    fn guess() -> Result<Box<Self>, MidiError> {
        let input = launchy::mini::Input::guess_polling()?;
        let output = launchy::mini::Output::guess()?;

        let midi_in = Arc::new(Mutex::new(input));
        let midi_out = Arc::new(Mutex::new(output));

        let (event_sender, event_receiver) = tokio::sync::broadcast::channel(10);

        Ok(Box::new(Self {
            midi_in,
            midi_out,
            event_receiver,
            event_sender: Arc::new(Mutex::new(event_sender)),
        }))
    }

    fn guess_ok() -> Result<(), MidiError> {
        launchy::mini::Input::guess_polling()?;
        launchy::mini::Output::guess()?;

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
                    launchy::mini::Message::Press { button } => match button {
                        launchy::mini::Button::GridButton { x, y } => {
                            info!("Midi -> send press event");
                            if let Err(error) = sender.send(ControllerEvent::Press { x, y: y + 1 })
                            {
                                info!("Error sending event: {}", error);
                            }
                        }
                        launchy::mini::Button::ControlButton { index } => {
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
                    launchy::launchpad_mini::Message::Release { button } => match button {
                        launchy::launchpad_mini::Button::GridButton { x, y } => {
                            info!("Midi -> send release event");
                            if let Err(error) =
                                sender.send(ControllerEvent::Release { x, y: y + 1 })
                            {
                                info!("Error sending event: {}", error);
                            }
                        }
                        launchy::mini::Button::ControlButton { index } => {
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

    fn get_event_receiver(&self) -> Result<tokio::sync::broadcast::Receiver<ControllerEvent>, ()> {
        info!("Getting event receiver mk1");

        Ok(self.event_receiver.resubscribe())
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

#[async_trait::async_trait]
impl ScriptRunner for LaunchpadMiniMk1 {
    async fn run(&self, script: &mut dyn Script) -> Result<(), MidiError> {
        script.initialize(self);

        let mut receiver = self.get_event_receiver().unwrap();


        loop {
            match receiver.try_recv() {
                Ok(message) => {
                    info!("HJIIIIzzzz");
                    match message {
                        ControllerEvent::Press { x, y } => {
                            info!("Received press event: {} {}", x, y);
                            script.on_press(x, y, self);
                        }
                        ControllerEvent::Release { x, y } => {
                            info!("Received release event: {} {}", x, y);
                            script.on_release(x, y, self);
                        }
                        _ => {
                            info!("Received message: {:?}", message)
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                Err(error) => match error {
                    TryRecvError::Empty => {}
                    TryRecvError::Closed => {
                        info!("Closed");
                        break;
                    }
                    TryRecvError::Lagged(_) => {
                        info!("Lagged");
                        break;
                    }
                },
            }
        }

        Ok(())
    }
}


impl Alles for LaunchpadMiniMk1 {}
