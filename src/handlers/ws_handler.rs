use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
};
use bson::{doc, oid::ObjectId, DateTime as BsonDateTime};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing;

use crate::{models::vote::Comment, state::AppState};

// ========== QUERY PARAMS ==========
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    #[serde(rename = "fixtureId")]
    pub fixture_id: String,

    #[serde(rename = "userId")]
    pub user_id: String,

    #[serde(rename = "username")]
    pub username: Option<String>,
}

// ========== WEB SOCKET MESSAGE TYPES ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum WSMessage {
    #[serde(rename = "comment.new")]
    CommentNew {
        payload: CommentPayload,
        timestamp: String,
    },
    #[serde(rename = "typing")]
    Typing {
        payload: TypingPayload,
        timestamp: String,
    },
    #[serde(rename = "comment.seen")]
    CommentSeen {
        payload: CommentReadReceipt,
        timestamp: String,
    },
    #[serde(rename = "presence")]
    Presence {
        payload: PresencePayload,
        timestamp: String,
    },
    #[serde(rename = "vote.update")]
    VoteUpdate {
        payload: VoteUpdatePayload,
        timestamp: String,
    },

    // ========== NEW: LIVE MATCH EVENTS ==========
    #[serde(rename = "match.goal")]
    MatchGoal {
        payload: GoalPayload,
        timestamp: String,
    },
    #[serde(rename = "match.score")]
    MatchScore {
        payload: ScorePayload,
        timestamp: String,
    },
    #[serde(rename = "match.card")]
    MatchCard {
        payload: CardPayload,
        timestamp: String,
    },
    #[serde(rename = "match.half_time")]
    MatchHalfTime {
        payload: HalfTimePayload,
        timestamp: String,
    },
    #[serde(rename = "match.full_time")]
    MatchFullTime {
        payload: FullTimePayload,
        timestamp: String,
    },
    #[serde(rename = "match.statistics")]
    MatchStatistics {
        payload: StatisticsPayload,
        timestamp: String,
    },
    #[serde(rename = "match.status")]
    MatchStatus {
        payload: StatusPayload,
        timestamp: String,
    },

    #[serde(rename = "pong")]
    Pong { timestamp: String },
    #[serde(rename = "connected")]
    Connected {
        fixture_id: String,
        timestamp: String,
    },
}

// ========== NEW: LIVE MATCH PAYLOADS ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GoalPayload {
    pub fixture_id: String,
    pub scorer: String,      // "home_team" or "away_team"
    pub scored_team: String, // Team name that scored
    pub home_score: i32,
    pub away_score: i32,
    pub minute: i32,
    pub player: Option<String>,
    pub score_display: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScorePayload {
    pub fixture_id: String,
    pub home_score: i32,
    pub away_score: i32,
    pub minute: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CardPayload {
    pub fixture_id: String,
    pub card_type: String, // "yellow" or "red"
    pub team: String,      // Team name
    pub player: String,
    pub minute: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HalfTimePayload {
    pub fixture_id: String,
    pub home_score: i32,
    pub away_score: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FullTimePayload {
    pub fixture_id: String,
    pub home_score: i32,
    pub away_score: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatisticsPayload {
    pub fixture_id: String,
    pub minute: i32,
    pub stats: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatusPayload {
    pub fixture_id: String,
    pub status: String, // "live", "half_time", "completed"
    pub time_elapsed: i32,
}

// ========== EXISTING PAYLOAD STRUCTURES ==========
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommentPayload {
    pub comment_id: String,
    pub voter_id: String,
    pub username: String,
    pub fixture_id: String,
    pub selection: String,
    pub comment: String,
    pub timestamp: String,
    pub likes: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TypingPayload {
    pub user_id: String,
    pub username: String,
    pub fixture_id: String,
    pub is_typing: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommentReadReceipt {
    pub comment_id: String,
    pub user_id: String,
    pub username: String,
    pub fixture_id: String,
    pub seen_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PresencePayload {
    pub user_id: String,
    pub username: String,
    pub status: String,
    pub fixture_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VoteUpdatePayload {
    pub fixture_id: String,
    pub user_id: String,
    pub selection: String,
    pub home_votes: i64,
    pub away_votes: i64,
    pub draw_votes: i64,
}

// ========== UPGRADE HANDLER ==========
pub async fn ws_comments_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let fixture_id = params.fixture_id.clone();
    let user_id = params.user_id.clone();
    let username = params.username.unwrap_or_else(|| "Anonymous".to_string());

    tracing::info!(
        "🔌 WS upgrade request for fixture: {}, user: {}",
        fixture_id,
        user_id
    );

    ws.on_upgrade(move |socket| handle_socket(socket, fixture_id, user_id, username, state))
}

// ========== PER-CONNECTION LOGIC ==========
// ========== PER-CONNECTION LOGIC ==========

// ========== PER-CONNECTION LOGIC ==========
async fn handle_socket(
    socket: WebSocket,
    fixture_id: String,
    user_id: String,
    username: String,
    state: AppState,
) {
    let tx = state.get_or_create_broadcaster(&fixture_id);
    let mut rx = tx.subscribe();

    let (sender, mut receiver) = socket.split();

    // Wrap sender in Arc<Mutex> to share between tasks
    let sender = Arc::new(Mutex::new(sender));
    let sender_clone = sender.clone();

    // Send welcome message
    let welcome = WSMessage::Connected {
        fixture_id: fixture_id.clone(),
        timestamp: Utc::now().to_rfc3339(),
    };

    if let Ok(welcome_json) = serde_json::to_string(&welcome) {
        let mut sender_guard = sender.lock().await;
        if sender_guard
            .send(Message::Text(welcome_json))
            .await
            .is_err()
        {
            return;
        }
    }

    // Send current match state immediately on connection
    send_current_match_state(&state, &fixture_id, &sender).await;

    // Broadcast user online presence
    let presence = WSMessage::Presence {
        payload: PresencePayload {
            user_id: user_id.clone(),
            username: username.clone(),
            status: "online".to_string(),
            fixture_id: fixture_id.clone(),
        },
        timestamp: Utc::now().to_rfc3339(),
    };

    if let Ok(presence_json) = serde_json::to_string(&presence) {
        let _ = tx.send(presence_json);
    }

    tracing::info!(
        "✅ WS connected: user {} to fixture {}",
        user_id,
        fixture_id
    );

    let fixture_id_for_send = fixture_id.clone();
    let fixture_id_for_recv = fixture_id.clone();
    let user_id_for_recv = user_id.clone();
    let username_for_recv = username.clone();

    // Task 1: Forward broadcast messages to this client
    let mut send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    let mut sender_guard = sender.lock().await;
                    if sender_guard.send(Message::Text(msg)).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("WS client lagged, skipped {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Task 2: Handle incoming messages
    let state_clone = state.clone();
    let tx_clone = tx.clone();

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    handle_incoming_message(
                        text,
                        &state_clone,
                        &fixture_id_for_recv,
                        &user_id_for_recv,
                        &username_for_recv,
                        &tx_clone,
                    )
                    .await;
                }
                Message::Close(_) => break,
                Message::Ping(_) => {
                    // Send pong response using sender_clone
                    let pong = WSMessage::Pong {
                        timestamp: Utc::now().to_rfc3339(),
                    };
                    if let Ok(pong_json) = serde_json::to_string(&pong) {
                        let mut sender_guard = sender_clone.lock().await;
                        let _ = sender_guard.send(Message::Text(pong_json)).await;
                    }
                }
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    // Broadcast user offline presence
    let offline_presence = WSMessage::Presence {
        payload: PresencePayload {
            user_id,
            username,
            status: "offline".to_string(),
            fixture_id: fixture_id_for_send,
        },
        timestamp: Utc::now().to_rfc3339(),
    };

    if let Ok(offline_json) = serde_json::to_string(&offline_presence) {
        let _ = tx.send(offline_json);
    }

    tracing::info!("🔌 WS disconnected for fixture: {}", fixture_id);
}

// ========== SEND CURRENT MATCH STATE ==========
async fn send_current_match_state(
    state: &AppState,
    fixture_id: &str,
    sender: &Arc<Mutex<futures_util::stream::SplitSink<WebSocket, Message>>>,
) {
    let collection = state.db.collection::<crate::models::game::Game>("games");
    let filter = doc! { "match_id": fixture_id };

    if let Ok(Some(game)) = collection.find_one(filter).await {
        // Send current score
        let score_msg = WSMessage::MatchScore {
            payload: ScorePayload {
                fixture_id: fixture_id.to_string(),
                home_score: game.home_score.unwrap_or(0),
                away_score: game.away_score.unwrap_or(0),
                minute: game.time_elapsed,
            },
            timestamp: Utc::now().to_rfc3339(),
        };

        if let Ok(score_json) = serde_json::to_string(&score_msg) {
            let mut sender_guard = sender.lock().await;
            let _ = sender_guard.send(Message::Text(score_json)).await;
        }

        // Send current status
        let status_msg = WSMessage::MatchStatus {
            payload: StatusPayload {
                fixture_id: fixture_id.to_string(),
                status: game.status,
                time_elapsed: game.time_elapsed,
            },
            timestamp: Utc::now().to_rfc3339(),
        };

        if let Ok(status_json) = serde_json::to_string(&status_msg) {
            let mut sender_guard = sender.lock().await;
            let _ = sender_guard.send(Message::Text(status_json)).await;
        }
    }
}
// ========== SEND CURRENT MATCH STATE ==========

// ========== HANDLE INCOMING MESSAGES ==========
async fn handle_incoming_message(
    text: String,
    state: &AppState,
    fixture_id: &str,
    user_id: &str,
    username: &str,
    broadcaster: &tokio::sync::broadcast::Sender<String>,
) {
    if let Ok(json_msg) = serde_json::from_str::<Value>(&text) {
        let message_type = json_msg.get("type").and_then(|t| t.as_str());

        match message_type {
            Some("comment.new") => {
                if let Some(payload) = json_msg.get("payload") {
                    if let Ok(comment_payload) =
                        serde_json::from_value::<CommentPayload>(payload.clone())
                    {
                        let broadcast_msg = WSMessage::CommentNew {
                            payload: comment_payload,
                            timestamp: Utc::now().to_rfc3339(),
                        };
                        if let Ok(broadcast_json) = serde_json::to_string(&broadcast_msg) {
                            let _ = broadcaster.send(broadcast_json);
                        }
                    }
                }
            }
            Some("typing") => {
                if let Some(payload) = json_msg.get("payload") {
                    if let Ok(typing_payload) =
                        serde_json::from_value::<TypingPayload>(payload.clone())
                    {
                        let broadcast_msg = WSMessage::Typing {
                            payload: typing_payload,
                            timestamp: Utc::now().to_rfc3339(),
                        };
                        if let Ok(broadcast_json) = serde_json::to_string(&broadcast_msg) {
                            let _ = broadcaster.send(broadcast_json);
                        }
                    }
                }
            }
            Some("comment.seen") => {
                if let Some(payload) = json_msg.get("payload") {
                    if let Ok(receipt) =
                        serde_json::from_value::<CommentReadReceipt>(payload.clone())
                    {
                        mark_comment_as_seen(state, &receipt).await;

                        let broadcast_msg = WSMessage::CommentSeen {
                            payload: receipt,
                            timestamp: Utc::now().to_rfc3339(),
                        };
                        if let Ok(broadcast_json) = serde_json::to_string(&broadcast_msg) {
                            let _ = broadcaster.send(broadcast_json);
                        }
                    }
                }
            }
            Some("presence") => {
                if let Some(payload) = json_msg.get("payload") {
                    if let Ok(presence_payload) =
                        serde_json::from_value::<PresencePayload>(payload.clone())
                    {
                        let broadcast_msg = WSMessage::Presence {
                            payload: presence_payload,
                            timestamp: Utc::now().to_rfc3339(),
                        };
                        if let Ok(broadcast_json) = serde_json::to_string(&broadcast_msg) {
                            let _ = broadcaster.send(broadcast_json);
                        }
                    }
                }
            }
            Some("ping") => {
                let pong = WSMessage::Pong {
                    timestamp: Utc::now().to_rfc3339(),
                };
                if let Ok(pong_json) = serde_json::to_string(&pong) {
                    let _ = broadcaster.send(pong_json);
                }
            }
            _ => {
                tracing::debug!("Unknown message type: {:?}", message_type);
            }
        }
    }
}

// ========== PUBLIC BROADCAST FUNCTION (Called by Poller) ==========
pub async fn broadcast_live_match_update(
    state: &AppState,
    fixture_id: &str,
    event_type: &str,
    data: serde_json::Value,
) {
    let ws_message = match event_type {
        "goal" => {
            if let Ok(payload) = serde_json::from_value::<GoalPayload>(data) {
                Some(WSMessage::MatchGoal {
                    payload,
                    timestamp: Utc::now().to_rfc3339(),
                })
            } else {
                None
            }
        }
        "score" => {
            if let Ok(payload) = serde_json::from_value::<ScorePayload>(data) {
                Some(WSMessage::MatchScore {
                    payload,
                    timestamp: Utc::now().to_rfc3339(),
                })
            } else {
                None
            }
        }
        "yellow_card" | "red_card" => {
            if let Ok(payload) = serde_json::from_value::<CardPayload>(data) {
                Some(WSMessage::MatchCard {
                    payload,
                    timestamp: Utc::now().to_rfc3339(),
                })
            } else {
                None
            }
        }
        "half_time" => {
            if let Ok(payload) = serde_json::from_value::<HalfTimePayload>(data) {
                Some(WSMessage::MatchHalfTime {
                    payload,
                    timestamp: Utc::now().to_rfc3339(),
                })
            } else {
                None
            }
        }
        "full_time" => {
            if let Ok(payload) = serde_json::from_value::<FullTimePayload>(data) {
                Some(WSMessage::MatchFullTime {
                    payload,
                    timestamp: Utc::now().to_rfc3339(),
                })
            } else {
                None
            }
        }
        "statistics" => {
            if let Ok(payload) = serde_json::from_value::<StatisticsPayload>(data) {
                Some(WSMessage::MatchStatistics {
                    payload,
                    timestamp: Utc::now().to_rfc3339(),
                })
            } else {
                None
            }
        }
        "status" => {
            if let Ok(payload) = serde_json::from_value::<StatusPayload>(data) {
                Some(WSMessage::MatchStatus {
                    payload,
                    timestamp: Utc::now().to_rfc3339(),
                })
            } else {
                None
            }
        }
        _ => None,
    };

    if let Some(message) = ws_message {
        if let Ok(json) = serde_json::to_string(&message) {
            let tx = state.get_or_create_broadcaster(fixture_id);
            let _ = tx.send(json);
            tracing::info!(
                "📡 Broadcasted {} event for fixture {}",
                event_type,
                fixture_id
            );
        }
    }
}

// ========== DATABASE OPERATIONS ==========
async fn mark_comment_as_seen(state: &AppState, receipt: &CommentReadReceipt) {
    let collection = state.db.collection::<Comment>("room");

    let object_id = match ObjectId::parse_str(&receipt.comment_id) {
        Ok(oid) => oid,
        Err(e) => {
            tracing::error!("Invalid comment ID: {}", e);
            return;
        }
    };

    let filter = doc! {
        "_id": object_id,
        "seenBy": { "$ne": &receipt.user_id }
    };

    let update = doc! {
        "$addToSet": { "seenBy": &receipt.user_id }
    };

    if let Err(e) = collection.update_one(filter, update).await {
        tracing::error!("Failed to mark comment as seen: {}", e);
    } else {
        tracing::info!(
            "✅ Comment {} marked as seen by user: {}",
            receipt.comment_id,
            receipt.user_id
        );
    }
}
