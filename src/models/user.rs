use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;
use mongodb::bson;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,
    pub username: String,
    pub phone: String,
    pub password_hash: String,
    pub balance: f64,

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,  // Changed from Option<DateTime<Utc>>

    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: DateTime<Utc>,  // Changed from Option<DateTime<Utc>>
}

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub phone: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginUser {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginWithPhone {
    pub phone: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,  // Changed from i32 to String (ObjectId hex)
    pub username: String,
    pub phone: String,
    pub balance: f64,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub user: UserResponse,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,  // Changed from i32 to String
    pub username: String,
    pub phone: String,
    pub exp: usize,
}