use chrono::{DateTime, Utc};
use mongodb::bson;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,
    pub user_id: String,
    pub user_name: String,
    pub caption: String,
    pub image_url: String,
    pub cloudinary_public_id: String,
    pub image_format: String,

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostResponse {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub caption: String,
    pub image_url: String,
    pub created_at: String, // Changed to String
    pub updated_at: String, // Changed to String
}

impl From<Post> for PostResponse {
    fn from(post: Post) -> Self {
        PostResponse {
            id: post._id.unwrap().to_hex(),
            user_id: post.user_id,
            user_name: post.user_name,
            caption: post.caption,
            image_url: post.image_url,
            created_at: post.created_at.to_rfc3339(), // Convert to ISO 8601 string
            updated_at: post.updated_at.to_rfc3339(), // Convert to ISO 8601 string
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PaginationParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateCaptionRequest {
    pub caption: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdatePostRequest {
    pub caption: Option<String>,
    pub image: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub user_id: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PostStats {
    pub total_posts: u64,
    pub posts_last_week: u64,
    pub top_users: Vec<serde_json::Value>,
    pub posts_by_hour: Vec<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UserPostStats {
    pub user_id: String,
    pub total_posts: u64,
    pub latest_post: Option<String>,
    pub first_post: Option<String>,
    pub posts_by_month: Vec<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
}
