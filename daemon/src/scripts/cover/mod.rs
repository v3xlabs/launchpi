use std::{fs::File, io::BufReader};

use crate::controllers::Controller;
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, OutputStreamHandle, Sink};
use tracing::info;

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

pub struct CoverScript {
    stream_handle: OutputStreamHandle,

    board_state: [[PadState; 8]; 8],
    board_audio: [[Option<String>; 8]; 8],
    board_colors: [[u8; 8]; 8],

    size: [u8; 2],
    offset: [u8; 2],
}

impl Script for CoverScript {
    fn name(&self) -> &'static str {
        "demo"
    }

    fn on_press(&mut self, x: u8, y: u8, controller: &dyn Controller) {
        let (x, y) = (x - self.offset[0], y - self.offset[1]);
        
        if x >= self.size[0] || y >= self.size[1] {
            info!("Out of board button press");
            return;
        }

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
        let (x, y) = (x - self.offset[0], y - self.offset[1]);
        
        info!("Demo! {} {}", x, y);
        
        if x >= self.size[0] || y >= self.size[1] {
            info!("Out of board button press");
            return;
        }
        
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
        let mut board_colors: [[u8; 8]; 8] = Default::default();

        // TODO: load board_audio
        // load `assets/soundboard.toml`
        let board_config = std::fs::read_to_string("assets/soundboard.toml").unwrap();
        let board_config: toml::Value = board_config.parse().unwrap();
        let board_config = board_config.get("pad").unwrap().as_array().unwrap();

        for pad in board_config {
            let x = pad.get("x").unwrap().as_integer().unwrap() as usize;
            let y = pad.get("y").unwrap().as_integer().unwrap() as usize;
            let file = pad.get("path").unwrap().as_str().unwrap().to_string();

            board_audio[x][y] = Some(file);
            board_colors[x][y] = pad.get("color").unwrap().as_integer().unwrap() as u8;
        }

        Self {
            size: [8, 8],
            offset: [0, 1],
            stream_handle,
            board_state: Default::default(),
            board_colors,
            board_audio,
        }
    }
}

impl CoverScript {
    fn update_board(&mut self, controller: &dyn super::Controller) {
        for x in 0..8 {
            for y in 0..8 {
                let color = match self.board_state[x][y] {
                    PadState::Idle => match self.board_audio[x][y].as_ref() {
                        Some(_) => self.board_colors[x][y],
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
