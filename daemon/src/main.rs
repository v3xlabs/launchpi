use tracing::{error, info};

mod api;
mod controllers;
mod state;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting daemon");

    let mut state = state::AppState::default();

    let controller = controllers::guess().await;

    let Ok(controller) = controller else {
        error!("Couldn't find a controller");

        return;
    };

    // controller.initialize().await.unwrap();

    info!("Successfully started controller: {}", controller.name());

    state.controllers.push(controller);

    api::serve(state).await.unwrap();
}
