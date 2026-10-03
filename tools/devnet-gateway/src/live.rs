use std::time::Duration;

use axum::{
    extract::{
        Query, State,
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::broadcast,
    time::{Instant, MissedTickBehavior},
};

use crate::{AppState, DevnetError, model::LiveEvent};

const PROTOCOL: &str = "payrail.live.v1";
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
pub(crate) struct LiveQuery {
    address: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Connected<'a> {
    r#type: &'static str,
    protocol: &'static str,
    address: &'a str,
    finalized_height: String,
}

pub(crate) async fn upgrade(
    websocket: WebSocketUpgrade,
    Query(query): Query<LiveQuery>,
    State(state): State<AppState>,
) -> Result<Response, DevnetError> {
    let status = {
        let service = state
            .service
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?;
        service.account(&query.address)?;
        service.status()
    };
    let receiver = state.events.subscribe();
    Ok(websocket
        .protocols([PROTOCOL])
        .on_upgrade(move |socket| serve(socket, receiver, query.address, status.finalized_height)))
}

async fn serve(
    mut socket: WebSocket,
    mut events: broadcast::Receiver<LiveEvent>,
    address: String,
    finalized_height: String,
) {
    let connected = Connected {
        r#type: "connected",
        protocol: PROTOCOL,
        address: &address,
        finalized_height,
    };
    if send_json(&mut socket, &connected).await.is_err() {
        return;
    }
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
    heartbeat.tick().await;
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            message = socket.recv() => match message {
                Some(Ok(Message::Pong(_))) => last_pong = Instant::now(),
                Some(Ok(Message::Ping(payload))) => {
                    if socket.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                    last_pong = Instant::now();
                }
                Some(Ok(Message::Text(text))) if text.as_str() == "ping" => {
                    if socket.send(Message::Text("{\"type\":\"pong\"}".into())).await.is_err() {
                        break;
                    }
                    last_pong = Instant::now();
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            event = events.recv() => match event {
                Ok(event) if event.concerns(&address) => {
                    if send_json(&mut socket, &event).await.is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if socket.send(Message::Text("{\"type\":\"resyncRequired\"}".into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = heartbeat.tick() => {
                if last_pong.elapsed() > CLIENT_TIMEOUT {
                    let _ = socket.send(Message::Close(Some(CloseFrame {
                        code: 1001,
                        reason: "heartbeat timeout".into(),
                    }))).await;
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn send_json(socket: &mut WebSocket, value: &impl Serialize) -> Result<(), axum::Error> {
    let Ok(json) = serde_json::to_string(value) else {
        return Ok(());
    };
    socket.send(Message::Text(json.into())).await
}
