use tracing::{error, info, warn, Level};

mod api;
mod controllers;
mod models;
mod persistence;
mod scripts;
mod state;
mod streamdeck;

/// `fmt::init()` pins INFO and ignores the environment unless tracing-subscriber is built with
/// `env-filter`, which we do not depend on. Read a bare level instead: `RUST_LOG=debug just dev`.
fn install_tracing() -> Level {
    let requested = std::env::var("RUST_LOG").ok();
    let parsed = requested
        .as_deref()
        .and_then(|value| value.trim().parse::<Level>().ok());
    let level = parsed.unwrap_or(Level::INFO);
    tracing_subscriber::fmt().with_max_level(level).init();
    if let (Some(value), None) = (requested.as_deref(), parsed) {
        warn!(
            value,
            "RUST_LOG is not a bare level (error, warn, info, debug, trace); using info"
        );
    }
    level
}

#[tokio::main]
async fn main() {
    let level = install_tracing();

    info!(%level, "Starting daemon");

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
