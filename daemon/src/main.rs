use std::{
    process,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use tokio::select;
use tracing::info;

mod api;
mod controllers;
mod scripts;
mod sound;
mod state;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting daemon");

    let (controller_tx, mut controller_rx) = tokio::sync::mpsc::channel(32);
    let controllers: Arc<Mutex<Vec<Arc<Box<dyn controllers::Alles>>>>> =
        Arc::new(Mutex::new(Vec::new()));

    let state = Arc::new(state::AppState {
        controller_tx,
        controllers,
    });

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
        _ = api::serve(state.clone()) => {},
        _ = add_controller(&mut controller_rx, state) => {},
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down");
        },
    }

    // controller.clear().unwrap();
    // controller2.clear().unwrap();

    thread::sleep(Duration::from_millis(100));

    process::exit(0);
}

pub async fn add_controller(
    controller_rx: &mut tokio::sync::mpsc::Receiver<Arc<Box<dyn controllers::Alles>>>,
    state1: Arc<state::AppState>,
) {
    info!("Starting controller receiver");
    while let Some(controller) = controller_rx.recv().await {
        info!("Received controller");
        controller.initialize().unwrap();

        let mut controllers = state1.controllers.lock().unwrap();
        controllers.push(controller);
        drop(controllers);
    }
}
