use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;
use tracing::info;

use crate::state::AppState;

pub async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let controllers = state.controllers.lock().unwrap();
    let receiver = controllers
        .first()
        .and_then(|x| x.get_event_receiver().ok())
        .ok_or(axum::Error::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "hi",
        )));
    drop(controllers);

    let receiver = match receiver {
        Ok(receiver) => receiver,
        Err(_) => {
            let (_, receiver) = broadcast::channel(1);
            receiver
        }
    };

    info!("Found receiver");
    let stream = BroadcastStream::new(receiver).map(|result| {
        result
            .map(|msg| {
                info!("Forwarding event");
                Event::default().json_data(msg).unwrap()
            })
            .map_err(|_| axum::Error::new(std::io::Error::new(std::io::ErrorKind::Other, "hi")))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
