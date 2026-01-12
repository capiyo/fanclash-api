// src/handlers/chat_handlers.rs
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use bson::{doc, oid::ObjectId};
use chrono::Utc;
use futures::TryStreamExt;
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};

use crate::models::chat::{ChatMessage, ChatMessageResponse, CreateChatMessage, UpdateChatMessage, MarkAsSeenRequest};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            message: Some(message),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

// Get collection helper
fn get_chat_collection(db: &Database) -> Collection<ChatMessage> {
    db.collection("chat_messages")
}

// GET /chat/:post_id/messages
pub async fn get_post_messages(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Query(params): Query<PaginationQuery>,
) -> impl IntoResponse {
    let collection = get_chat_collection(&state.db);

    let page = params.page.unwrap_or(1);
    let limit = params.limit.unwrap_or(50);
    let skip = (page - 1) * limit;

    let filter = doc! { "post_id": &post_id };

    match collection.count_documents(filter.clone()).await {
        Ok(total) => {
            let mut cursor = collection
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip as u64)
                .limit(limit as i64)
                .await;

            match cursor {
                Ok(mut cursor) => {
                    let mut messages = Vec::new();
                    while let Ok(Some(message)) = cursor.try_next().await {
                        messages.push(ChatMessageResponse::from(message));
                    }

                    let response = serde_json::json!({
                        "messages": messages,
                        "total": total,
                        "page": page,
                        "limit": limit,
                        "total_pages": (total as f64 / limit as f64).ceil() as i64
                    });

                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                Err(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to fetch messages: {}", err))),
                ),
            }
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Failed to count messages: {}", err))),
        ),
    }
}

// POST /chat/:post_id/messages
pub async fn create_message(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Json(payload): Json<CreateChatMessage>,
) -> impl IntoResponse {
    let collection = get_chat_collection(&state.db);

    // Validate post_id matches
    if payload.post_id != post_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Post ID mismatch".to_string())),
        );
    }

    // Create new message
    let chat_message = ChatMessage {
        id: None,
        post_id: payload.post_id,
        user_id: payload.user_id,
        username: payload.username,
        message: payload.message,
        seen: false,
        created_at: Utc::now(),
    };

    match collection.insert_one(chat_message).await {
        Ok(result) => {
            // Fetch the created message
            let filter = doc! { "_id": result.inserted_id.as_object_id().unwrap() };
            match collection.find_one(filter).await {
                Ok(Some(message)) => {
                    let response = ChatMessageResponse::from(message);
                    (StatusCode::CREATED, Json(ApiResponse::success(response)))
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error("Failed to retrieve created message".to_string())),
                ),
            }
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Failed to create message: {}", err))),
        ),
    }
}

// GET /chat/messages/:message_id
pub async fn get_message(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
) -> impl IntoResponse {
    let collection = get_chat_collection(&state.db);

    match ObjectId::parse_str(&message_id) {
        Ok(object_id) => {
            let filter = doc! { "_id": object_id };

            match collection.find_one(filter).await {
                Ok(Some(message)) => {
                    let response = ChatMessageResponse::from(message);
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                Ok(None) => (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::error("Message not found".to_string())),
                ),
                Err(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to fetch message: {}", err))),
                ),
            }
        }
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid message ID format".to_string())),
        ),
    }
}

// PUT /chat/messages/:message_id
pub async fn update_message(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
    Json(payload): Json<UpdateChatMessage>,
) -> impl IntoResponse {
    let collection = get_chat_collection(&state.db);

    // In real app, get user_id from auth
    let user_id = "current_user_id".to_string();

    match ObjectId::parse_str(&message_id) {
        Ok(object_id) => {
            let filter = doc! {
                "_id": object_id,
                "user_id": &user_id // Users can only update their own messages
            };

            let update = doc! {
                "$set": {
                    "message": &payload.message
                }
            };

            match collection.find_one_and_update(filter, update).await {
                Ok(Some(message)) => {
                    let response = ChatMessageResponse::from(message);
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                Ok(None) => (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::error("Message not found or unauthorized".to_string())),
                ),
                Err(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to update message: {}", err))),
                ),
            }
        }
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid message ID format".to_string())),
        ),
    }
}

// DELETE /chat/messages/:message_id
pub async fn delete_message(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
) -> impl IntoResponse {
    let collection = get_chat_collection(&state.db);

    // In real app, get user_id from auth
    let user_id = "current_user_id".to_string();

    match ObjectId::parse_str(&message_id) {
        Ok(object_id) => {
            let filter = doc! {
                "_id": object_id,
                "user_id": &user_id // Users can only delete their own messages
            };

            match collection.delete_one(filter).await {
                Ok(result) if result.deleted_count > 0 => (
                    StatusCode::OK,
                    Json(ApiResponse::success("Message deleted successfully".to_string())),
                ),
                Ok(_) => (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::error("Message not found or unauthorized".to_string())),
                ),
                Err(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to delete message: {}", err))),
                ),
            }
        }
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid message ID format".to_string())),
        ),
    }
}

// POST /chat/messages/mark-seen
pub async fn mark_messages_as_seen(
    State(state): State<AppState>,
    Json(payload): Json<MarkAsSeenRequest>,
) -> impl IntoResponse {
    let collection = get_chat_collection(&state.db);

    // In real app, get user_id from auth
    let user_id = "current_user_id".to_string();

    // Convert string IDs to ObjectId
    let object_ids: Result<Vec<ObjectId>, _> = payload.message_ids
        .iter()
        .map(|id| ObjectId::parse_str(id))
        .collect();

    match object_ids {
        Ok(object_ids) => {
            // User can't mark their own messages as seen
            let filter = doc! {
                "_id": { "$in": object_ids },
                "user_id": { "$ne": &user_id }
            };

            let update = doc! {
                "$set": {
                    "seen": true
                }
            };

            match collection.update_many(filter, update).await {
                Ok(result) => {
                    let response = serde_json::json!({
                        "marked_count": result.modified_count
                    });
                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                Err(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to mark messages: {}", err))),
                ),
            }
        }
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Invalid message ID format".to_string())),
        ),
    }
}

// GET /chat/:post_id/unread-count
pub async fn get_unread_count(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
) -> impl IntoResponse {
    let collection = get_chat_collection(&state.db);

    // In real app, get user_id from auth
    let user_id = "current_user_id".to_string();

    let filter = doc! {
        "post_id": &post_id,
        "user_id": { "$ne": &user_id }, // Messages from other users
        "seen": false
    };

    match collection.count_documents(filter).await {
        Ok(count) => (
            StatusCode::OK,
            Json(ApiResponse::success(count)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Failed to count unread messages: {}", err))),
        ),
    }
}

// GET /chat/users/:user_id/messages
pub async fn get_user_messages(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(params): Query<PaginationQuery>,
) -> impl IntoResponse {
    let collection = get_chat_collection(&state.db);

    let page = params.page.unwrap_or(1);
    let limit = params.limit.unwrap_or(50);
    let skip = (page - 1) * limit;

    let filter = doc! { "user_id": &user_id };

    match collection.count_documents(filter.clone()).await {
        Ok(total) => {
            let mut cursor = collection
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .skip(skip as u64)
                .limit(limit as i64)
                .await;

            match cursor {
                Ok(mut cursor) => {
                    let mut messages = Vec::new();
                    while let Ok(Some(message)) = cursor.try_next().await {
                        messages.push(ChatMessageResponse::from(message));
                    }

                    let response = serde_json::json!({
                        "messages": messages,
                        "total": total,
                        "page": page,
                        "limit": limit
                    });

                    (StatusCode::OK, Json(ApiResponse::success(response)))
                }
                Err(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(format!("Failed to fetch user messages: {}", err))),
                ),
            }
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Failed to count user messages: {}", err))),
        ),
    }
}
