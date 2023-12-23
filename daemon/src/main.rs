use tracing::info;

mod api;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting daemon");

    api::serve().await.unwrap();
}
