use tracing::info;

mod api;
mod state;
mod controllers;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting daemon");

    let state = state::AppState::default();

    api::serve(state).await.unwrap();
}
