use std::{fs::File, io::BufReader, time::Duration};

use rodio::{source::SineWave, Decoder, Sink, Source};
use cpal::traits::{HostTrait, DeviceTrait};
use tracing::info;

use crate::controllers::Controller;

use super::Script;

pub struct PingScript {}

impl Script for PingScript {
    fn name(&self) -> &'static str {
        "ping"
    }

    fn on_press(&self, x: u8, y: u8, controller: &impl super::Controller) {
        info!("Ping! {} {}", x, y);
        controller.set_button_color(x, y, 0).unwrap();

        tokio::spawn(async {
            // play sound
            let thing = cpal::default_host().output_devices().unwrap().find(|x| {
                // x.name().unwrap().contains("pipewire")
                info!("Device: {:?}", x.name().unwrap());
                false
            }).unwrap();

            let (_stream, stream_handle) = rodio::OutputStream::try_from_device(&thing).unwrap();
            let sink = Sink::try_new(&stream_handle).unwrap();

            let file = BufReader::new(File::open("assets/ping.wav").unwrap());
            let source = Decoder::new(file).unwrap();

            sink.append(source);

            sink.sleep_until_end();
        });
    }

    fn new() -> Self {
        Self {}
    }
}

pub struct Ping2Script {}

impl Script for Ping2Script {
    fn name(&self) -> &'static str {
        "ping2"
    }

    fn on_press(&self, x: u8, y: u8, controller: &impl Controller) {}

    fn new() -> Self {
        Self {}
    }
}
