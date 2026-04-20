use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub firebase_uid: String,
    pub username: String,
    pub phone: String,
    pub balance: f64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub firebase_uid: String,
    pub username: String,
    pub phone: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub firebase_uid: String,
    pub username: String,
    pub phone: String,
    pub balance: f64,
}

// Keep these for backward compatibility if needed
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
pub struct AuthResponse {
    pub user: UserResponse,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub username: String,
    pub phone: String,
    pub exp: usize,
}
