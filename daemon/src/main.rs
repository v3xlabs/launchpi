use launchy::MidiError;
use scripts::Script;
use tokio::{join, select};
use tracing::{error, info};

use crate::controllers::{
    launchpad_mini_mk1::LaunchpadMiniMk1, launchpad_mini_mk3::LaunchpadMiniMk3, Controller,
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

    let controller = LaunchpadMiniMk3::guess().unwrap();
    let controller2 = LaunchpadMiniMk1::guess().unwrap();

    controller.initialize().unwrap();
    controller2.initialize().unwrap();

    // info!("Successfully started controller: {}", controller.name());

    // state.controllers.push(controller);

    let script = scripts::ping::PingScript::new();

    tokio::spawn(async move { controller.run(&script).unwrap() });

    let script2 = scripts::ping::PingScript::new();

    tokio::spawn(async move { controller2.run(&script2).unwrap() });

    api::serve(state).await.unwrap();
}
