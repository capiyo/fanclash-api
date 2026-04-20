use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::Utc;
use futures_util::TryStreamExt;
use jsonwebtoken::{encode, EncodingKey, Header};
use mongodb::{
    bson::{doc, oid::ObjectId},
    Collection,
};
use serde_json::json;

use crate::errors::AppError;
use crate::models::user::{AuthResponse, Claims, CreateUserRequest, LoginUser, User, UserResponse};
use crate::state::AppState;

const BCRYPT_COST: u32 = 4;

// ========== REGISTER ==========
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> impl IntoResponse {
    println!("📝 Registering user: {}", payload.username);

    let collection: Collection<User> = state.db.collection("users");

    // Check if username already exists
    let existing = collection
        .find_one(doc! { "username": &payload.username })
        .await;

    if let Ok(Some(_)) = existing {
        return (
            StatusCode::CONFLICT,
            Json(json!({
                "success": false,
                "message": "Username already taken"
            })),
        )
            .into_response();
    }

    // Hash password
    let password_hash = match hash(&payload.password, BCRYPT_COST) {
        Ok(hash) => hash,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "message": "Failed to hash password"
                })),
            )
                .into_response();
        }
    };

    let user = User {
        id: None,
        username: payload.username.clone(),
        phone: payload.phone.clone(),
        password_hash,
        balance: 0.0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        reset_otp: None,
    };

    let result = collection.insert_one(&user).await;

    match result {
        Ok(inserted) => {
            let inserted_id = inserted.inserted_id.as_object_id().unwrap();
            println!("✅ User created: {}", payload.username);

            let user_response = UserResponse {
                id: inserted_id.to_hex(),
                username: payload.username,
                phone: payload.phone,
                balance: 0.0,
            };

            let token = generate_token(
                &user_response.id,
                &user_response.username,
                &user_response.phone,
            );

            (
                StatusCode::CREATED,
                Json(json!({
                    "success": true,
                    "user": user_response,
                    "token": token
                })),
            )
                .into_response()
        }
        Err(e) => {
            println!("❌ Failed to create user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "message": "Failed to create user"
                })),
            )
                .into_response()
        }
    }
}

// ========== LOGIN ==========
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginUser>,
) -> impl IntoResponse {
    println!("🔐 Login attempt: {}", payload.username);

    let collection: Collection<User> = state.db.collection("users");

    let user = match collection
        .find_one(doc! { "username": &payload.username })
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "success": false,
                    "message": "User not found"
                })),
            )
                .into_response();
        }
        Err(e) => {
            println!("❌ Database error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "message": "Database error"
                })),
            )
                .into_response();
        }
    };

    // Verify password
    match verify(&payload.password, &user.password_hash) {
        Ok(true) => {
            println!("✅ Login successful: {}", payload.username);

            let user_response = UserResponse {
                id: user.id.unwrap().to_hex(),
                username: user.username,
                phone: user.phone,
                balance: user.balance,
            };

            let token = generate_token(
                &user_response.id,
                &user_response.username,
                &user_response.phone,
            );

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "user": user_response,
                    "token": token
                })),
            )
                .into_response()
        }
        Ok(false) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "success": false,
                "message": "Invalid password"
            })),
        )
            .into_response(),
        Err(e) => {
            println!("❌ Password verification error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "message": "Error verifying password"
                })),
            )
                .into_response()
        }
    }
}

// ========== GET ALL USERS ==========
pub async fn get_all_users(State(state): State<AppState>) -> impl IntoResponse {
    println!("📥 Getting all users");

    let collection: Collection<User> = state.db.collection("users");

    match collection.find(doc! {}).await {
        Ok(cursor) => {
            let users: Vec<User> = match cursor.try_collect().await {
                Ok(users) => users,
                Err(e) => {
                    println!("❌ Failed to collect users: {}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "success": false,
                            "message": "Failed to fetch users"
                        })),
                    )
                        .into_response();
                }
            };

            let user_responses: Vec<UserResponse> = users
                .into_iter()
                .filter_map(|user| {
                    Some(UserResponse {
                        id: user.id?.to_hex(),
                        username: user.username,
                        phone: user.phone,
                        balance: user.balance,
                    })
                })
                .collect();

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "users": user_responses
                })),
            )
                .into_response()
        }
        Err(e) => {
            println!("❌ Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "message": "Database error"
                })),
            )
                .into_response()
        }
    }
}

// ========== GET USER BY ID ==========
pub async fn get_user_by_id(
    State(state): State<AppState>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    println!("📥 Getting user by ID: {}", user_id);

    let collection: Collection<User> = state.db.collection("users");

    let object_id = match ObjectId::parse_str(&user_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "success": false,
                    "message": "Invalid user ID"
                })),
            )
                .into_response();
        }
    };

    match collection.find_one(doc! { "_id": object_id }).await {
        Ok(Some(user)) => {
            let user_response = UserResponse {
                id: user.id.unwrap().to_hex(),
                username: user.username,
                phone: user.phone,
                balance: user.balance,
            };

            (
                StatusCode::OK,
                Json(json!({
                    "success": true,
                    "user": user_response
                })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "message": "User not found"
            })),
        )
            .into_response(),
        Err(e) => {
            println!("❌ Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "message": "Database error"
                })),
            )
                .into_response()
        }
    }
}

// ========== HELPER: Generate JWT Token ==========
fn generate_token(user_id: &str, username: &str, phone: &str) -> String {
    let claims = Claims {
        sub: user_id.to_string(),
        username: username.to_string(),
        phone: phone.to_string(),
        exp: (Utc::now().timestamp() + 86400) as usize, // 24 hours
    };

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .unwrap_or_else(|_| "".to_string())
}
