use axum::{routing::get, Router};
use tokio::net::TcpListener;

use crate::state::AppState;

mod routes;

pub async fn serve(state: AppState) -> Result<(), axum::Error> {
    let app = Router::new()
        .route("/", get(root))
        .route("/devices", get(routes::devices::get))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn root() -> &'static str {
    "Hello, World!"
}
