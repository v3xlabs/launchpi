use async_stream::stream;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{stream::Stream, StreamExt};
use std::{convert::Infallible, sync::Arc};
use tokio::stream;
use tokio_stream::wrappers::BroadcastStream;
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
    let stream = stream! {
        if let Some(controller) = &first_controller {
            info!("SSE handler 2");
            let mut receiver = controller.resubscribe();
                info!("SSE handler 3");
                loop {
                    info!("SSE handler 4");
                    let message = receiver.recv().await.unwrap();
                    info!("Received messagezzz: {:?}", message);
                    yield Ok(Event::default().json_data(message).unwrap());
                }
         }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
