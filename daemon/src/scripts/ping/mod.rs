use crate::controllers::Controller;
use cpal::traits::{DeviceTrait, HostTrait};
use rodio::OutputStreamHandle;
use tracing::info;

use super::Script;

pub struct PingScript {
    stream_handle: OutputStreamHandle,
}

impl Script for PingScript {
    fn name(&self) -> &'static str {
        "ping"
    }

    fn on_press(&mut self, x: u8, y: u8, controller: &dyn Controller) {
        info!("Ping! {} {}", x, y);
        controller.set_button_color(x, y, 1).unwrap();

        // info!("Playing sound");
        // let file = File::open("assets/developers.mp3").unwrap();
        // let file = Decoder::new(BufReader::new(file)).unwrap();

        // let sink = Sink::try_new(&self.stream_handle).unwrap();

        // sink.append(file);

        // sink.detach();
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

        Self { stream_handle }
    }
}
