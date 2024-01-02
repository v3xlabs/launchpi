use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
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

use crate::state::AppState;

pub async fn sse_handler(
    ws: WebSocketUpgrade,
    Path(device_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, device_id, state))
}

async fn handle_socket(socket: WebSocket, device_id: String, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    tokio::spawn(write(sender, device_id, state));
    tokio::spawn(read(receiver));
}

async fn read(receiver: SplitStream<WebSocket>) {
    // ...
}

async fn write(mut sender: SplitSink<WebSocket, Message>, device_id: String, state: Arc<AppState>) {
    let state = state.clone();
    let controllers = state.controllers.lock().unwrap().clone();
    let first_controller = controllers
        .iter()
        .find(|controller| match controller.name() {
            "Launchpad Mini Mk1" => device_id == "launchpad_mini_mk1_0",
            "Launchpad Mini Mk3" => device_id == "launchpad_mini_mk3_0",
            _ => false,
        })
        .and_then(|controller| controller.get_event_receiver().ok());
    drop(controllers);

    if let Some(mut controller) = first_controller {
        info!("Controller found");
        tokio::spawn(async move {
            loop {
                while let Ok(message) = controller.try_recv() {
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
