use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::state::AppState;

// ── Query params ──────────────────────────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    #[serde(rename = "fixtureId")]
    pub fixture_id: String,
}

// ── Upgrade handler ───────────────────────────────────────────────────────────
pub async fn ws_comments_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let fixture_id = params.fixture_id.clone();
    tracing::info!("🔌 WS upgrade request for fixture: {}", fixture_id);
    ws.on_upgrade(move |socket| handle_socket(socket, fixture_id, state))
}

// ── Per-connection logic ──────────────────────────────────────────────────────
async fn handle_socket(socket: WebSocket, fixture_id: String, state: AppState) {
    let tx = state.get_or_create_broadcaster(&fixture_id);
    let mut rx = tx.subscribe();

    let (mut sender, mut receiver) = socket.split();

    // Send a welcome ping so the Flutter client knows the connection is live
    let welcome = json!({
        "type": "connected",
        "fixtureId": fixture_id,
    })
    .to_string();

    if sender.send(Message::Text(welcome)).await.is_err() {
        return; // client already gone
    }

    tracing::info!("✅ WS connected for fixture: {}", fixture_id);

    // Task 1: forward broadcast messages → this client
    let mut send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if sender.send(Message::Text(msg)).await.is_err() {
                        break; // client disconnected
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("WS client lagged, skipped {} messages", n);
                    // keep going — just dropped some messages
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Task 2: drain incoming frames (ping/pong keepalive + close detection)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                Message::Ping(_) => {} // axum auto-replies with Pong
                _ => {}                // clients don't send anything else
            }
        }
    });

    // When either task exits, abort the other and clean up
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    tracing::info!("🔌 WS disconnected for fixture: {}", fixture_id);
}
