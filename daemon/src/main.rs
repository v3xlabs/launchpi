use std::{
    process,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use scripts::Script;
use tokio::select;
use tracing::info;

use crate::controllers::{
    launchpad_mini_mk1::LaunchpadMiniMk1, launchpad_mini_mk3::LaunchpadMiniMk3, Alles, Controller,
    ScriptRunner,
};

mod api;
mod controllers;
mod scripts;
mod state;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting daemon");

    let state = state::AppState::default();

    // let mut controllers: Vec<Arc<Box<dyn Alles>>> = Vec::new();

    // let controller: Arc<Box<dyn Alles>> = Arc::new(LaunchpadMiniMk1::guess().unwrap());
    // controllers.push(controller.clone());
    // let controller2: Arc<Box<dyn Alles>> = Arc::new(LaunchpadMiniMk3::guess().unwrap());
    // controllers.push(controller2.clone());

    // controller.initialize().unwrap();
    // controller2.initialize().unwrap();

    // let mut script = scripts::ping::PingScript::new();

    // let controller1 = controller.clone();
    // tokio::spawn(async move { controller1.run(&mut script).unwrap() });

    // let mut script2 = scripts::soundboard::SoundboardScript::new();

    // let controller21 = controller2.clone();
    // tokio::spawn(async move { controller21.run(&mut script2).unwrap() });

    select! {
        _ = api::serve(state) => {},
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down");
        },
    }

    // controller.clear().unwrap();
    // controller2.clear().unwrap();

    thread::sleep(Duration::from_millis(100));

    process::exit(0);
}
