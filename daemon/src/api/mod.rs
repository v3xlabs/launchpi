use axum::{
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;

mod routes;

pub async fn serve() -> Result<(), axum::Error> {
    let app = Router::new()
        .route("/", get(root))
        .route("/devices", get(routes::devices::get));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn root() -> &'static str {
    "Hello, World!"
}
