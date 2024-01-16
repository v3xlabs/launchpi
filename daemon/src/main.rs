use std::{
    process,
    sync::{Arc, Mutex}, collections::HashMap,
};

use tokio::select;
use tracing::info;

mod api;
mod controllers;
mod scripts;
mod sound;
mod bootstrap;
mod state;

type ArcMutexVec<T> = Arc<Mutex<Vec<T>>>;
type ArcBox<T> = Arc<Box<T>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting daemon");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(1);

    let (controller_tx, mut controller_rx) = tokio::sync::mpsc::channel(32);
    let controllers: ArcMutexVec<ArcBox<dyn controllers::Alles>> =
        Arc::new(Mutex::new(Vec::new()));

    let state = Arc::new(state::AppState {
        controller_tx,
        controllers,
        shutdown_tx,
        running_scripts: Arc::new(Mutex::new(HashMap::new())),
    });

    select! {
        _ = api::serve(state.clone()) => {},
        _ = add_controller(&mut controller_rx, state.clone()) => {},
        _ = bootstrap::bootstrap(state.clone()) => {},
        _ = shutdown_rx.recv() => {
            info!("Received shutdown signal, shutting down");
        },
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down");
        },
    }

    drop(state);

    info!("Dropping state");

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
