use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use mongodb::bson;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Post {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,

    pub user_id: String,
    pub user_name: String,
    pub caption: String,
    pub image_url: String,
    pub cloudinary_public_id: String,
    pub image_format: String,

    pub likes_count: i32,
    pub comments_count: i32,
    pub shares_count: i32,
    pub liked_by: Vec<String>,
    pub is_saved: bool,

    // FIX: Add MongoDB serde helper for DateTime
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Comment {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,

    pub post_id: String,
    pub user_id: String,
    pub user_name: String,

    #[validate(length(min = 1, message = "Comment cannot be empty"))]
    pub comment: String,

    pub likes_count: i32,
    pub liked_by: Vec<String>,

    // FIX: Add MongoDB serde helper for DateTime
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
    pub cloudinary_public_id: String,
    pub image_format: String,

    pub likes_count: i32,
    pub comments_count: i32,
    pub shares_count: i32,
    pub liked_by: Vec<String>,
    pub is_saved: bool,

    pub created_at: String,
    pub updated_at: String,
}

impl From<Post> for PostResponse {
    fn from(post: Post) -> Self {
        PostResponse {
            id: post._id.unwrap().to_hex(),
            user_id: post.user_id,
            user_name: post.user_name,
            caption: post.caption,
            image_url: post.image_url,
            cloudinary_public_id: post.cloudinary_public_id,
            image_format: post.image_format,
            likes_count: post.likes_count,
            comments_count: post.comments_count,
            shares_count: post.shares_count,
            liked_by: post.liked_by,
            is_saved: post.is_saved,
            created_at: post.created_at.to_rfc3339(),
            updated_at: post.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommentResponse {
    pub id: String,
    pub post_id: String,
    pub user_id: String,
    pub user_name: String,
    pub comment: String,
    pub likes_count: i32,
    pub liked_by: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Comment> for CommentResponse {
    fn from(comment: Comment) -> Self {
        CommentResponse {
            id: comment._id.unwrap().to_hex(),
            post_id: comment.post_id,
            user_id: comment.user_id,
            user_name: comment.user_name,
            comment: comment.comment,
            likes_count: comment.likes_count,
            liked_by: comment.liked_by,
            created_at: comment.created_at.to_rfc3339(),
            updated_at: comment.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LikeRequest {
    pub user_id: String,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct CreateCommentRequest {
    pub user_id: String,
    pub user_name: String,
    #[validate(length(min = 1, message = "Comment cannot be empty"))]
    pub comment: String,
}

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct UpdateCommentRequest {
    pub user_id: String,
    #[validate(length(min = 1, message = "Comment cannot be empty"))]
    pub comment: String,
}
