use std::cmp::max;
use std::thread::sleep;
use std::time::Duration;
use std::{fs::File, io::BufReader};

use crate::controllers::Controller;
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::{Decoder, OutputStreamHandle, Sink};
use tracing::info;

use super::Script;

pub struct PingScript {
    stream_handle: OutputStreamHandle,
    curent_color: u8,
}

impl Script for PingScript {
    fn name(&self) -> &'static str {
        "ping"
    }

    fn on_press(&mut self, x: u8, y: u8, controller: &dyn Controller) {
        info!("Ping! {} {}", x, y);

        if x == 0 && y == 0 {
            controller.clear().unwrap();
            self.initialize(controller);
            return;
        }

        if x == 1 && y == 0 {
            self.curent_color = 0;
            self.initialize(controller);
            return;
        }

        if x == 8 {
            self.curent_color = y;
            self.initialize(controller);
            return;
        }

        controller
            .set_button_color(x, y, self.curent_color)
            .unwrap();
    }

    fn initialize(&mut self, controller: &dyn Controller) {
        let mut updates: Vec<(u8, u8, u8)> = Vec::new();
        for color in 1..=8 {
            updates.push((8, color, color));
        }
        for i in 0..=7 {
            updates.push((i, 0, self.curent_color));
        }
        controller.set_button_color_multi(&updates).unwrap();
    }

    fn new() -> Self {
        let device = cpal::default_host()
            .output_devices()
            .unwrap()
            .find(|device| {
                info!("--- {}", device.name().unwrap());
                device.name().unwrap().contains("pipewire")
            })
            .unwrap();

        let (stream, stream_handle) = rodio::OutputStream::try_from_device(&device).unwrap();

        std::mem::forget(stream);

        Self {
            stream_handle,
            curent_color: 1,
        }
    }
}
