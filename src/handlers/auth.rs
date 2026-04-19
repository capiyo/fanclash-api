use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::Utc;
use futures_util::TryStreamExt;
use jsonwebtoken::{encode, EncodingKey, Header};
use mongodb::{
    bson::{doc, oid::ObjectId, DateTime},
    Collection,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use validator::Validate;

use crate::errors::{AppError, Result};
use crate::models::user::{
    AuthResponse, Claims, CreateUser, LoginUser, LoginWithPhone, User, UserResponse,
};
use crate::services::otp_service::OTPService;
use crate::state::AppState;

// ========== CONSTANTS ==========
const BCRYPT_COST: u32 = 4;

// ========== OTP STORAGE FOR REGISTRATION ==========
lazy_static::lazy_static! {
    static ref REGISTRATION_OTP_STORE: Mutex<HashMap<String, RegistrationOtpSession>> = Mutex::new(HashMap::new());
}

#[derive(Debug, Clone)]
struct RegistrationOtpSession {
    phone: String,
    otp_code: String,
    expires_at: i64,
    verified: bool,
}

fn generate_otp() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1000000))
}

// ========== REQUEST/RESPONSE DTOs ==========

// Registration OTP DTOs
#[derive(Debug, Deserialize)]
pub struct SendOtpRequest {
    pub phone: String,
}

#[derive(Debug, Serialize)]
pub struct SendOtpResponse {
    pub success: bool,
    pub message: String,
    pub temp_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyOtpRequest {
    pub temp_id: String,
    pub otp_code: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyOtpResponse {
    pub success: bool,
    pub message: String,
    pub verification_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterWithOtpRequest {
    pub username: String,
    pub phone: String,
    pub password: String,
    pub verification_token: String,
}

// Forgot Password DTOs
#[derive(Debug, Deserialize, Validate)]
pub struct ForgotPasswordRequest {
    #[validate(length(min = 3, message = "Username must be at least 3 characters"))]
    pub username: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct VerifyResetOtpRequest {
    pub user_id: String,
    #[validate(length(min = 6, max = 6, message = "OTP must be 6 digits"))]
    pub otp: String,
    pub reset_token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordRequest {
    pub user_id: String,
    pub reset_token: String,
    #[validate(length(min = 6, message = "Password must be at least 6 characters"))]
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct ForgotPasswordResponse {
    pub success: bool,
    pub message: String,
    pub user_id: Option<String>,
    pub reset_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VerifyResetOtpResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ResetPasswordResponse {
    pub success: bool,
    pub message: String,
}

// ========== EXISTING AUTH HANDLERS ==========

pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<CreateUser>,
) -> Result<Json<AuthResponse>> {
    let collection: Collection<User> = state.db.collection("users");

    let filter = doc! {
        "$or": [
            { "username": &payload.username },
            { "phone": &payload.phone }
        ]
    };

    let existing_user = collection.find_one(filter).await?;

    if existing_user.is_some() {
        return Err(AppError::UserAlreadyExists);
    }

    let password = payload.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || hash(&password, BCRYPT_COST))
        .await
        .map_err(|_| AppError::InternalServerError("Thread join error".to_string()))?
        .map_err(|_| AppError::InternalServerError("Failed to hash password".to_string()))?;

    let user = User {
        _id: None,
        username: payload.username.clone(),
        phone: payload.phone.clone(),
        password_hash,
        balance: 0.0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        reset_otp: None,
    };

    let _insert_result = collection.insert_one(&user).await?;

    let inserted_id = user._id.unwrap();

    let user_response = UserResponse {
        id: inserted_id.to_hex(),
        username: payload.username.clone(),
        phone: payload.phone.clone(),
        balance: 0.0,
    };

    let claims = Claims {
        sub: inserted_id.to_hex(),
        username: payload.username.clone(),
        phone: payload.phone.clone(),
        exp: (Utc::now().timestamp() + 86400) as usize,
    };

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|_| AppError::InternalServerError("Failed to generate token".to_string()))?;

    Ok(Json(AuthResponse {
        user: user_response,
        token,
    }))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginUser>,
) -> Result<Json<AuthResponse>> {
    let collection: Collection<User> = state.db.collection("users");

    let filter = doc! { "username": &payload.username };
    let user = collection
        .find_one(filter)
        .await?
        .ok_or(AppError::UserNotFound)?;

    let password = payload.password.clone();
    let hash_clone = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify(&password, &hash_clone))
        .await
        .map_err(|_| AppError::InternalServerError("Thread join error".to_string()))?
        .map_err(|_| AppError::InternalServerError("Password verification failed".to_string()))?;

    if !valid {
        return Err(AppError::InvalidPassword);
    }

    let user_response = UserResponse {
        id: user._id.unwrap().to_hex(),
        username: user.username.clone(),
        phone: user.phone.clone(),
        balance: user.balance,
    };

    let claims = Claims {
        sub: user._id.unwrap().to_hex(),
        username: user.username.clone(),
        phone: user.phone.clone(),
        exp: (Utc::now().timestamp() + 86400) as usize,
    };

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|_| AppError::InternalServerError("Failed to generate token".to_string()))?;

    Ok(Json(AuthResponse {
        user: user_response,
        token,
    }))
}

pub async fn login_with_phone(
    State(state): State<AppState>,
    Json(payload): Json<LoginWithPhone>,
) -> Result<Json<AuthResponse>> {
    let collection: Collection<User> = state.db.collection("users");

    let filter = doc! { "phone": &payload.phone };
    let user = collection
        .find_one(filter)
        .await?
        .ok_or(AppError::UserNotFound)?;

    let password = payload.password.clone();
    let hash_clone = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify(&password, &hash_clone))
        .await
        .map_err(|_| AppError::InternalServerError("Thread join error".to_string()))?
        .map_err(|_| AppError::InternalServerError("Password verification failed".to_string()))?;

    if !valid {
        return Err(AppError::InvalidPassword);
    }

    let user_response = UserResponse {
        id: user._id.unwrap().to_hex(),
        username: user.username.clone(),
        phone: user.phone.clone(),
        balance: user.balance,
    };

    let claims = Claims {
        sub: user._id.unwrap().to_hex(),
        username: user.username.clone(),
        phone: user.phone.clone(),
        exp: (Utc::now().timestamp() + 86400) as usize,
    };

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|_| AppError::InternalServerError("Failed to generate token".to_string()))?;

    Ok(Json(AuthResponse {
        user: user_response,
        token,
    }))
}

pub async fn get_user_profile(
    State(state): State<AppState>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
) -> Result<Json<UserResponse>> {
    let collection: Collection<User> = state.db.collection("users");

    let object_id =
        ObjectId::parse_str(&user_id).map_err(|_| AppError::invalid_data("Invalid user ID"))?;

    let filter = doc! { "_id": object_id };
    let user = collection
        .find_one(filter)
        .await?
        .ok_or(AppError::DocumentNotFound)?;

    let user_response = UserResponse {
        id: user._id.unwrap().to_hex(),
        username: user.username,
        phone: user.phone,
        balance: user.balance,
    };

    Ok(Json(user_response))
}

pub async fn get_all_users(State(state): State<AppState>) -> Result<Json<Vec<UserResponse>>> {
    let collection: Collection<User> = state.db.collection("users");

    let cursor = collection.find(doc! {}).await?;
    let users: Vec<User> = cursor.try_collect().await?;

    let user_responses: Vec<UserResponse> = users
        .into_iter()
        .map(|user| UserResponse {
            id: user._id.unwrap().to_hex(),
            username: user.username,
            phone: user.phone,
            balance: user.balance,
        })
        .collect();

    Ok(Json(user_responses))
}

// ========== FORGOT PASSWORD HANDLERS ==========

pub async fn forgot_password(
    State(state): State<AppState>,
    Json(req): Json<ForgotPasswordRequest>,
) -> impl IntoResponse {
    if let Err(errors) = req.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ForgotPasswordResponse {
                success: false,
                message: format!("Validation error: {}", errors),
                user_id: None,
                reset_token: None,
            }),
        )
            .into_response();
    }

    let users: Collection<User> = state.db.collection("users");

    let user = match users.find_one(doc! { "username": &req.username }).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ForgotPasswordResponse {
                    success: false,
                    message: "User not found".to_string(),
                    user_id: None,
                    reset_token: None,
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ForgotPasswordResponse {
                    success: false,
                    message: "Database error".to_string(),
                    user_id: None,
                    reset_token: None,
                }),
            )
                .into_response();
        }
    };

    let user_id = match user._id {
        Some(id) => id,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ForgotPasswordResponse {
                    success: false,
                    message: "User ID not found".to_string(),
                    user_id: None,
                    reset_token: None,
                }),
            )
                .into_response();
        }
    };

    let otp_code = OTPService::generate_otp();
    let reset_token = match state.otp_service.generate_reset_token(&user_id.to_hex()) {
        Ok(token) => token,
        Err(e) => {
            tracing::error!("Token generation error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ForgotPasswordResponse {
                    success: false,
                    message: "Failed to generate reset token".to_string(),
                    user_id: None,
                    reset_token: None,
                }),
            )
                .into_response();
        }
    };

    if let Err(e) = state
        .otp_service
        .store_otp_in_user(&user_id, &otp_code, &reset_token)
        .await
    {
        tracing::error!("Failed to store OTP: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ForgotPasswordResponse {
                success: false,
                message: "Failed to store OTP".to_string(),
                user_id: None,
                reset_token: None,
            }),
        )
            .into_response();
    }

    if let Err(e) = state.sms_service.send_otp(&user.phone, &otp_code).await {
        tracing::error!("Failed to send SMS: {}", e);
    }

    (
        StatusCode::OK,
        Json(ForgotPasswordResponse {
            success: true,
            message: "OTP sent to your phone".to_string(),
            user_id: Some(user_id.to_hex()),
            reset_token: Some(reset_token),
        }),
    )
        .into_response()
}

pub async fn verify_reset_otp(
    State(state): State<AppState>,
    Json(req): Json<VerifyResetOtpRequest>,
) -> impl IntoResponse {
    let user_id = match ObjectId::parse_str(&req.user_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyResetOtpResponse {
                    success: false,
                    message: "Invalid user ID".to_string(),
                }),
            )
                .into_response();
        }
    };

    match state
        .otp_service
        .verify_user_otp(&user_id, &req.otp, &req.reset_token)
        .await
    {
        Ok(true) => (
            StatusCode::OK,
            Json(VerifyResetOtpResponse {
                success: true,
                message: "OTP verified successfully".to_string(),
            }),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::BAD_REQUEST,
            Json(VerifyResetOtpResponse {
                success: false,
                message: "Invalid or expired OTP".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("OTP verification error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(VerifyResetOtpResponse {
                    success: false,
                    message: "Failed to verify OTP".to_string(),
                }),
            )
                .into_response()
        }
    }
}

pub async fn reset_password(
    State(state): State<AppState>,
    Json(req): Json<ResetPasswordRequest>,
) -> impl IntoResponse {
    let user_id = match ObjectId::parse_str(&req.user_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ResetPasswordResponse {
                    success: false,
                    message: "Invalid user ID".to_string(),
                }),
            )
                .into_response();
        }
    };

    let users: Collection<User> = state.db.collection("users");

    let hashed_password = match hash(&req.new_password, DEFAULT_COST) {
        Ok(pw) => pw,
        Err(e) => {
            tracing::error!("Password hashing error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ResetPasswordResponse {
                    success: false,
                    message: "Failed to hash password".to_string(),
                }),
            )
                .into_response();
        }
    };

    let now = chrono::Utc::now();
    let now_bson = mongodb::bson::DateTime::from_millis(now.timestamp_millis());

    let filter = doc! { "_id": user_id };
    let update = doc! {
        "$set": {
            "password_hash": hashed_password,
            "updated_at": now_bson
        },
        "$unset": { "reset_otp": "" }
    };

    match users.update_one(filter, update).await {
        Ok(result) if result.matched_count > 0 => (
            StatusCode::OK,
            Json(ResetPasswordResponse {
                success: true,
                message: "Password reset successful".to_string(),
            }),
        )
            .into_response(),
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(ResetPasswordResponse {
                success: false,
                message: "User not found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Password update error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ResetPasswordResponse {
                    success: false,
                    message: "Failed to reset password".to_string(),
                }),
            )
                .into_response()
        }
    }
}

// ========== REGISTRATION OTP HANDLERS (NEW) ==========

pub async fn send_registration_otp(
    State(state): State<AppState>,
    Json(req): Json<SendOtpRequest>,
) -> impl IntoResponse {
    // Check if phone already registered
    let users: Collection<User> = state.db.collection("users");
    let existing = users.find_one(doc! { "phone": &req.phone }).await;

    if let Ok(Some(_)) = existing {
        return (
            StatusCode::CONFLICT,
            Json(SendOtpResponse {
                success: false,
                message: "Phone number already registered".to_string(),
                temp_id: None,
            }),
        )
            .into_response();
    }

    // Generate OTP
    let otp_code = generate_otp();
    let expires_at = Utc::now().timestamp() + 300; // 5 minutes
    let temp_id = uuid::Uuid::new_v4().to_string();

    // Store OTP session
    let session = RegistrationOtpSession {
        phone: req.phone.clone(),
        otp_code: otp_code.clone(),
        expires_at,
        verified: false,
    };

    {
        let mut map = REGISTRATION_OTP_STORE.lock().unwrap();
        map.insert(temp_id.clone(), session);
    }

    // Send SMS
    match state.sms_service.send_otp(&req.phone, &otp_code).await {
        Ok(_) => {
            tracing::info!("Registration OTP sent to {}: {}", req.phone, otp_code);
            (
                StatusCode::OK,
                Json(SendOtpResponse {
                    success: true,
                    message: "OTP sent successfully".to_string(),
                    temp_id: Some(temp_id),
                }),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to send registration SMS: {}", e);
            let mut map = REGISTRATION_OTP_STORE.lock().unwrap();
            map.remove(&temp_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SendOtpResponse {
                    success: false,
                    message: "Failed to send OTP".to_string(),
                    temp_id: None,
                }),
            )
                .into_response()
        }
    }
}

pub async fn verify_registration_otp(
    State(_state): State<AppState>,
    Json(req): Json<VerifyOtpRequest>,
) -> impl IntoResponse {
    let session = {
        let map = REGISTRATION_OTP_STORE.lock().unwrap();
        map.get(&req.temp_id).cloned()
    };

    let mut session = match session {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyOtpResponse {
                    success: false,
                    message: "Invalid or expired session".to_string(),
                    verification_token: None,
                }),
            )
                .into_response();
        }
    };

    // Check expiry
    if Utc::now().timestamp() > session.expires_at {
        let mut map = REGISTRATION_OTP_STORE.lock().unwrap();
        map.remove(&req.temp_id);
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyOtpResponse {
                success: false,
                message: "OTP has expired. Please request a new one".to_string(),
                verification_token: None,
            }),
        )
            .into_response();
    }

    // Verify OTP
    if session.otp_code != req.otp_code {
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyOtpResponse {
                success: false,
                message: "Invalid OTP code".to_string(),
                verification_token: None,
            }),
        )
            .into_response();
    }

    // Mark as verified
    {
        let mut map = REGISTRATION_OTP_STORE.lock().unwrap();
        if let Some(s) = map.get_mut(&req.temp_id) {
            s.verified = true;
            session = s.clone();
        }
    }

    (
        StatusCode::OK,
        Json(VerifyOtpResponse {
            success: true,
            message: "OTP verified successfully".to_string(),
            verification_token: Some(req.temp_id),
        }),
    )
        .into_response()
}

pub async fn register_with_otp(
    State(state): State<AppState>,
    Json(payload): Json<RegisterWithOtpRequest>,
) -> Result<Json<AuthResponse>> {
    let collection: Collection<User> = state.db.collection("users");

    // Check if username or phone already exists
    let existing = collection
        .find_one(doc! {
            "$or": [
                { "username": &payload.username },
                { "phone": &payload.phone }
            ]
        })
        .await?;

    if existing.is_some() {
        return Err(AppError::UserAlreadyExists);
    }

    // Verify OTP session exists and is verified
    let session = {
        let map = REGISTRATION_OTP_STORE.lock().unwrap();
        map.get(&payload.verification_token).cloned()
    };

    match session {
        Some(s) if s.verified && s.phone == payload.phone => {
            // OTP verified, proceed
        }
        Some(_) => {
            return Err(AppError::PhoneNotVerified);
        }
        None => {
            return Err(AppError::InvalidOtp);
        }
    }

    // Hash password
    let password = payload.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || hash(&password, BCRYPT_COST))
        .await
        .map_err(|_| AppError::InternalServerError("Thread join error".to_string()))?
        .map_err(|_| AppError::InternalServerError("Failed to hash password".to_string()))?;

    let user = User {
        _id: None,
        username: payload.username.clone(),
        phone: payload.phone.clone(),
        password_hash,
        balance: 0.0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        reset_otp: None,
    };

    collection.insert_one(&user).await?;

    let inserted_id = user._id.unwrap();

    // Clean up OTP session
    {
        let mut map = REGISTRATION_OTP_STORE.lock().unwrap();
        map.remove(&payload.verification_token);
    }

    let user_response = UserResponse {
        id: inserted_id.to_hex(),
        username: payload.username,
        phone: payload.phone,
        balance: 0.0,
    };

    let claims = Claims {
        sub: inserted_id.to_hex(),
        username: user_response.username.clone(),
        phone: user_response.phone.clone(),
        exp: (Utc::now().timestamp() + 86400) as usize,
    };

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|_| AppError::InternalServerError("Failed to generate token".to_string()))?;

    Ok(Json(AuthResponse {
        user: user_response,
        token,
    }))
}
