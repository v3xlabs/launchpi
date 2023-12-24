use tracing::info;

mod api;
mod controllers;
mod state;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting daemon");

    let mut state = state::AppState::default();

    let controller = controllers::guess().await.unwrap();

    // controller.initialize().await.unwrap();

    info!("Successfully started controller: {}", controller.name());

    state.controllers.push(controller);

    api::serve(state).await.unwrap();
}
