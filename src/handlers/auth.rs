use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::Utc;
use mongodb::{
    bson::{doc, oid::ObjectId},
    Collection,
};

use crate::models::user::{CreateUserRequest, User, UserResponse};
use crate::state::AppState;
use futures_util::TryStreamExt;

// Create user profile after Firebase registration
pub async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> impl IntoResponse {
    println!(
        "📝 Creating user profile for Firebase UID: {}",
        payload.firebase_uid
    );

    let collection: Collection<User> = state.db.collection("users");

    // Check if user already exists
    let existing = collection
        .find_one(doc! { "firebase_uid": &payload.firebase_uid })
        .await;

    if let Ok(Some(_)) = existing {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "success": false,
                "message": "User already exists"
            })),
        )
            .into_response();
    }

    // Check if username is taken
    let username_exists = collection
        .find_one(doc! { "username": &payload.username })
        .await;

    if let Ok(Some(_)) = username_exists {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "success": false,
                "message": "Username already taken"
            })),
        )
            .into_response();
    }

    let user = User {
        id: None,
        firebase_uid: payload.firebase_uid,
        username: payload.username,
        phone: payload.phone,
        balance: 0.0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let result = collection.insert_one(&user).await;

    match result {
        Ok(inserted) => {
            let inserted_id = inserted.inserted_id.as_object_id().unwrap();
            println!("✅ User created with ID: {}", inserted_id);

            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "success": true,
                    "user": {
                        "id": inserted_id.to_hex(),
                        "firebase_uid": user.firebase_uid,
                        "username": user.username,
                        "phone": user.phone,
                        "balance": user.balance,
                    }
                })),
            )
                .into_response()
        }
        Err(e) => {
            println!("❌ Failed to create user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "message": "Failed to create user"
                })),
            )
                .into_response()
        }
    }
}

// Get user by username (for login check)
pub async fn get_user_by_username(
    State(state): State<AppState>,
    username: String,
) -> impl IntoResponse {
    println!("🔍 Looking for user by username: {}", username);

    let collection: Collection<User> = state.db.collection("users");

    let user = collection.find_one(doc! { "username": &username }).await;

    match user {
        Ok(Some(user)) => {
            let response = UserResponse {
                id: user.id.unwrap().to_hex(),
                firebase_uid: user.firebase_uid,
                username: user.username,
                phone: user.phone,
                balance: user.balance,
            };

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "user": response
                })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "success": false,
                "message": "User not found"
            })),
        )
            .into_response(),
        Err(e) => {
            println!("❌ Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "message": "Database error"
                })),
            )
                .into_response()
        }
    }
}

// Get user profile by Firebase UID
pub async fn get_user_by_firebase_uid(
    State(state): State<AppState>,
    axum::extract::Path(firebase_uid): axum::extract::Path<String>,
) -> impl IntoResponse {
    println!("📥 Fetching user for Firebase UID: {}", firebase_uid);

    let collection: Collection<User> = state.db.collection("users");

    let user = collection
        .find_one(doc! { "firebase_uid": &firebase_uid })
        .await;

    match user {
        Ok(Some(user)) => {
            let response = UserResponse {
                id: user.id.unwrap().to_hex(),
                firebase_uid: user.firebase_uid,
                username: user.username,
                phone: user.phone,
                balance: user.balance,
            };

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "user": response
                })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "success": false,
                "message": "User not found"
            })),
        )
            .into_response(),
        Err(e) => {
            println!("❌ Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "message": "Database error"
                })),
            )
                .into_response()
        }
    }
}

// Update user profile
pub async fn update_user(
    State(state): State<AppState>,
    axum::extract::Path(firebase_uid): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    println!("📝 Updating user for Firebase UID: {}", firebase_uid);

    let collection: Collection<User> = state.db.collection("users");

    let mut set_doc = doc! {
        "updated_at": Utc::now(),
    };

    if let Some(username) = payload.get("username").and_then(|v| v.as_str()) {
        set_doc.insert("username", username);
    }
    if let Some(phone) = payload.get("phone").and_then(|v| v.as_str()) {
        set_doc.insert("phone", phone);
    }
    if let Some(balance) = payload.get("balance").and_then(|v| v.as_f64()) {
        set_doc.insert("balance", balance);
    }

    let update = doc! { "$set": set_doc };

    let result = collection
        .update_one(doc! { "firebase_uid": &firebase_uid }, update)
        .await;

    match result {
        Ok(update_result) if update_result.matched_count > 0 => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "message": "User updated successfully"
            })),
        )
            .into_response(),
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "success": false,
                "message": "User not found"
            })),
        )
            .into_response(),
        Err(e) => {
            println!("❌ Update failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "message": "Failed to update user"
                })),
            )
                .into_response()
        }
    }
}

// Get all users (admin only - keep if needed)
pub async fn get_all_users(State(state): State<AppState>) -> impl IntoResponse {
    let collection: Collection<User> = state.db.collection("users");

    match collection.find(doc! {}).await {
        Ok(cursor) => {
            let users: Vec<User> = cursor.try_collect().await.unwrap_or_default();
            let responses: Vec<UserResponse> = users
                .into_iter()
                .filter_map(|user| {
                    Some(UserResponse {
                        id: user.id?.to_hex(),
                        firebase_uid: user.firebase_uid,
                        username: user.username,
                        phone: user.phone,
                        balance: user.balance,
                    })
                })
                .collect();

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "users": responses
                })),
            )
                .into_response()
        }
        Err(e) => {
            println!("❌ Failed to fetch users: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "message": "Failed to fetch users"
                })),
            )
                .into_response()
        }
    }
}
