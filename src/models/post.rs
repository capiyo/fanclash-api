use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;
use chrono::{DateTime, Utc};
use mongodb::bson;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,
    pub user_id: String,
    pub user_name: String,
    pub caption: String,
    pub image_url: String,
    pub image_path: String,

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,  // Changed from Option<DateTime<Utc>>

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,  // Changed from Option<DateTime<Utc>>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostResponse {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub caption: String,
    pub image_url: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl From<Post> for PostResponse {
    fn from(post: Post) -> Self {
        PostResponse {
            id: post._id.unwrap().to_hex(),
            user_id: post.user_id,
            user_name: post.user_name,
            caption: post.caption,
            image_url: post.image_url,
            created_at: Option::from(post.created_at),
            updated_at: Option::from(post.updated_at),
        }
    }
}