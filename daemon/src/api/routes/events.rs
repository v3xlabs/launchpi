use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};

use std::{convert::Infallible, sync::Arc};
use tokio::{
    stream,
    sync::{broadcast::Receiver, mpsc},
};
use tracing::info;

use crate::{controllers::ControllerEvent, state::AppState};

pub async fn sse_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    tokio::spawn(write(sender, state));
    tokio::spawn(read(receiver));
}

async fn read(receiver: SplitStream<WebSocket>) {
    // ...
}

async fn write(
    mut sender: SplitSink<WebSocket, Message>,
    state: Arc<AppState>,
) {

    let state = state.clone();
    let controllers = state.controllers.lock().unwrap().clone();
    let first_controller = controllers
        .first()
        .and_then(|controller| controller.get_event_receiver().ok());
    drop(controllers);

    // ...
    info!("Apec");

    if let Some(mut controller) = first_controller {
        tokio::spawn(async move {
            info!("Apecz");
            loop {
                while let Ok(message) = controller.try_recv() {
                    info!("Received messagezzz: {:?}", message);
                    sender
                        .send(Message::Text(serde_json::to_string(&message).unwrap()))
                        .await
                        .unwrap();
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        });
    }
}
