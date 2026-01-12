// src/models/chat.rs
use bson::{oid::ObjectId, DateTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub post_id: String,
    pub user_id: String,
    pub username: String,
    pub message: String,
    pub seen: bool,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateChatMessage {
    pub post_id: String,
    pub user_id: String,
    pub username: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateChatMessage {
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MarkAsSeenRequest {
    pub message_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessageResponse {
    pub id: String,
    pub post_id: String,
    pub user_id: String,
    pub username: String,
    pub message: String,
    pub seen: bool,
    pub created_at: String,
}

impl From<ChatMessage> for ChatMessageResponse {
    fn from(message: ChatMessage) -> Self {
        Self {
            id: message.id.unwrap().to_hex(),
            post_id: message.post_id,
            user_id: message.user_id,
            username: message.username,
            message: message.message,
            seen: message.seen,
            created_at: message.created_at.to_rfc3339(),
        }
    }
}
