use std::{
    process,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use futures::select;
use scripts::Script;
use tracing::info;

use crate::controllers::{
    launchpad_mini_mk1::LaunchpadMiniMk1, launchpad_mini_mk3::LaunchpadMiniMk3, Controller, ScriptRunner, Alles
};

mod api;
mod controllers;
mod scripts;
mod state;

#[async_std::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting daemon");

    let state = state::AppState::default();

    let mut controllers: Vec<Arc<Box<dyn Alles>>> = Vec::new();

    // let controller: Arc<Box<dyn Alles>> = Arc::new(LaunchpadMiniMk1::guess().unwrap());
    // controllers.push(controller.clone());
    let controller2: Arc<Box<dyn Alles>> = Arc::new(LaunchpadMiniMk3::guess().unwrap());
    controllers.push(controller2.clone());

    // controller.initialize().unwrap();
    controller2.initialize().unwrap();

    // info!("Successfully started controller: {}", controller.name());

    // state.controllers.push(controller);

    // let mut script = scripts::soundboard::SoundboardScript::new();

    // let controller1 = controller.clone();
    // tokio::spawn(async move { controller1.run(&mut script).unwrap() });

    let mut script2 = scripts::soundboard::SoundboardScript::new();

    let controller21 = controller2.clone();

    controller21.run(&mut script2).unwrap();

    // tokio::spawn();

    // async move { controller21.run(&mut script2).unwrap() };
    // _ = api::serve(state) => {},

    // controller.clear().unwrap();
    controller2.clear().unwrap();

    thread::sleep(Duration::from_millis(100));

    process::exit(0);
}
