use crate::state::AppState;
use axum::Router;
use tokio::net::TcpListener;

mod routes;

pub async fn serve(state: AppState) -> Result<(), std::io::Error> {
    let app = Router::new()
        .merge(routes::surfaces::router())
        .with_state(state);
    let listener = TcpListener::bind("0.0.0.0:3000").await?;

    axum::serve(listener, app).await
}
