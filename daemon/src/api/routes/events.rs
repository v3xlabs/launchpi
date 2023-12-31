use async_stream::try_stream;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use std::sync::Arc;
use tokio::sync::broadcast;
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

    let mut receiver = match receiver {
        Ok(receiver) => receiver,
        Err(_) => {
            let (_, receiver) = broadcast::channel(1);
            receiver
        }
    };

    Sse::new(try_stream! {
        loop {
            match receiver.recv().await {
                Ok(msg) => {
                    info!("Forwarding event");
                    yield Event::default().json_data(msg).unwrap();
                }
                Err(_) => {
                    info!("Event receiver closed");
                    break;
                },
            }
        }
    })
    .keep_alive(KeepAlive::default())
}
