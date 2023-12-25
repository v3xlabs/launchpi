use std::{fs::File, io::BufReader, sync::{Arc, Mutex}};

use cpal::traits::{DeviceTrait, HostTrait};
use lazy_static::lazy_static;
use rodio::{Decoder, OutputStreamHandle, Sink, OutputStream};
use tracing::info;
use crate::controllers::Controller;

use super::Script;

enum PadState {
    Playing(Sink),
    Idle,
}

impl Default for PadState {
    fn default() -> Self {
        Self::Idle
    }
}

pub struct DemoScript {
    stream_handle: OutputStreamHandle,

    board_state: [[PadState; 8]; 8],
    board_audio: [[Option<String>; 8]; 8],
}

impl Script for DemoScript {
    fn name(&self) -> &'static str {
        "demo"
    }

    fn on_press(&mut self, x: u8, y: u8, controller: &dyn Controller) {
        info!("Demo! {} {}", x, y);

        let desired_state = match &self.board_state[x as usize][y as usize] {
            PadState::Idle => {
                if let Some(location) = self.board_audio[x as usize][y as usize].as_ref() {
                    let file = File::open(location).unwrap();
                    let file = Decoder::new(BufReader::new(file)).unwrap();

                    let sink = Sink::try_new(&self.stream_handle).unwrap();

                    sink.append(file);

                    PadState::Playing(sink)
                } else {
                    PadState::Idle
                }
            }
            PadState::Playing(sink) => {
                sink.stop();
                PadState::Idle
            }
        };

        // match desired_state {
        //     PadState::Idle => controller.set_button_color(x, y, 0).unwrap(),
        //     PadState::Playing(_) => controller.set_button_color(x, y, 3).unwrap(),
        // }

        self.board_state[x as usize][y as usize] = desired_state;

        self.update_board(controller);
    }

    fn on_release(&mut self, x: u8, y: u8, controller: &dyn Controller) {
        info!("Demo! {} {}", x, y);

        let desired_state = PadState::Idle;

        // match desired_state {
        //     PadState::Idle => controller.set_button_color(x, y, 0).unwrap(),
        //     PadState::Playing(_) => controller.set_button_color(x, y, 3).unwrap(),
        // }
        self.board_state[x as usize][y as usize] = desired_state;
        
        self.update_board(controller);
    }

    fn initialize(&mut self, controller: &dyn Controller) {
        self.update_board(controller);
    }

    fn new() -> Self {
        let device = cpal::default_host()
            .output_devices()
            .unwrap()
            .find(|device| device.name().unwrap().contains("pipewire"))
            .unwrap();

        let (stream, stream_handle) = rodio::OutputStream::try_from_device(&device).unwrap();

        std::mem::forget(stream);

        let mut board_audio: [[Option<String>; 8]; 8] = Default::default();

        // TODO: load board_audio

        Self {
            stream_handle,
            board_state: Default::default(),
            board_audio,
        }
    }
}

impl DemoScript {
    fn update_board(&mut self, controller: &dyn super::Controller) {
        for x in 0..8 {
            for y in 0..8 {
                let color = match self.board_state[x][y] {
                    PadState::Idle => match self.board_audio[x][y].as_ref() {
                        Some(_) => 1,
                        None => 0,
                    },
                    PadState::Playing(_) => 2,
                };
                controller
                    .set_button_color(x as u8, y as u8, color)
                    .unwrap();
            }
        }
    }
}
