use crate::state::AppState;
use axum::Router;
use tokio::net::TcpListener;

mod error;
mod routes;
mod web;

fn router(state: AppState) -> Router {
    Router::new()
        .merge(routes::surfaces::router())
        .merge(routes::plugins::router())
        .merge(routes::fonts::router())
        .fallback(web::serve)
        .with_state(state)
}

pub async fn serve(state: AppState) -> Result<(), std::io::Error> {
    let host = std::env::var("LAUNCHPI_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("LAUNCHPI_PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = TcpListener::bind(format!("{host}:{port}")).await?;

    axum::serve(listener, router(state)).await
}
