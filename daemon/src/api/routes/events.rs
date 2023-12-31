use async_stream::stream;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{stream::Stream, StreamExt};
use std::{convert::Infallible, sync::Arc};
use tokio::{stream, sync::mpsc};
use tokio_stream::wrappers::{BroadcastStream, ReceiverStream};
use tracing::info;

use crate::state::AppState;

pub async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let state = state.clone();
    let controllers = state.controllers.lock().unwrap().clone();
    let first_controller = controllers
        .first()
        .and_then(|controller| controller.get_event_receiver().ok());
    // drop(controllers);

    info!("SSE handler");

    // if let Some(controller) = &first_controller {
    //     let mut receiver = controller.resubscribe();
    //     tokio::spawn(async move {
    //         loop {
    //             let message = receiver.recv().await.unwrap();
    //             info!("Received messagezzz: {:?}", message);
    //         }
    //     });
    // }

    let (tx, rx) = mpsc::channel(10);

    if let Some(controller) = &first_controller {
        let mut receiver = controller.resubscribe();
        tokio::spawn(async move {
            while let Ok(message) = receiver.recv().await {
                info!("Received messagezzz: {:?}", message);
                tx.send(message).await.unwrap();
            }
        });
    }

    let stream = ReceiverStream::new(rx).map(|_| Ok(Event::default().data("HELLOOO")));

    Sse::new(stream).keep_alive(KeepAlive::default())
}
