use axum::{routing::get, Router};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::state::AppState;

mod routes;

pub async fn serve(state: Arc<AppState>) -> Result<(), axum::Error> {
    let cors = CorsLayer::very_permissive();

    let app = Router::new()
        .route("/", get(routes::root::root))
        .route("/devices", get(routes::devices::get))
        .route("/connect/:device_id", get(routes::connect::post))
        .route("/events/:device_id", get(routes::events::sse_handler))
        .route("/run/:device_id", get(routes::run::post))
        .layer(cors)
        .with_state(state);

    let mut port = 3000;

    let listener = loop {
        match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
            Ok(listener) => break listener,
            Err(_) => {
                if port > 3010 {
                    panic!("Could not bind to any port between 3000 and 3010")
                }
                info!("Port {} is already in use, trying {}", port, port + 1);
                port += 1
            }
        }
    };

    info!("Listening on http://0.0.0.0:{}", port);

    axum::serve(listener, app).await.unwrap();

    Ok(())
}
