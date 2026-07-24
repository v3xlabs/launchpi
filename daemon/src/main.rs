use tracing::{error, info};

mod api;
mod controllers;
mod models;
mod persistence;
mod scripts;
mod state;
mod streamdeck;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting daemon");

    let state = match state::AppState::load().await {
        Ok(state) => state,
        Err(error) => {
            error!(%error, "unable to load persistent state");
            return;
        }
    };

    for surface in state
        .surfaces
        .managed_surfaces()
        .into_iter()
        .filter(|surface| surface.is_enabled)
    {
        streamdeck::studio::start_connection_monitor(state.clone(), surface);
    }

    if let Err(error) = streamdeck::studio::start_discovery(state.clone()) {
        error!(%error, "unable to start Stream Deck Studio discovery");
    }

    if let Err(error) = api::serve(state).await {
        error!(%error, "API server stopped unexpectedly");
    }
}
