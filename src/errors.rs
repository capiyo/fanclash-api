use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("MongoDB error: {0}")]
    MongoDB(#[from] mongodb::error::Error),

    #[error("Multipart error: {0}")]
    Multipart(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid image format")]
    InvalidImageFormat,

    #[error("Image too large")]
    ImageTooLarge,

    #[error("No image provided")]
    NoImageProvided,

    #[error("Invalid user data")]
    InvalidUserData,

    #[error("Post not found")]
    PostNotFound,

    #[error("Invalid ObjectId: {0}")]
    InvalidObjectId(String),

    #[error("Document not found")]
    DocumentNotFound,

    #[error("Duplicate key error")]
    DuplicateKey,

    #[error("M-Pesa error: {0}")]
    MpesaError(String),

    #[error("Authentication error")]
    AuthError,

    #[error("Unauthorized access")]
    Unauthorized,

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("External API error: {0}")]
    ExternalApi(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::MongoDB(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
            AppError::Multipart(_) => (StatusCode::BAD_REQUEST, "Invalid multipart data"),
            AppError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IO error"),
            AppError::InvalidImageFormat => (StatusCode::BAD_REQUEST, "Invalid image format"),
            AppError::ImageTooLarge => (StatusCode::BAD_REQUEST, "Image too large"),
            AppError::NoImageProvided => (StatusCode::BAD_REQUEST, "No image provided"),
            AppError::InvalidUserData => (StatusCode::BAD_REQUEST, "Invalid user data"),
            AppError::PostNotFound => (StatusCode::NOT_FOUND, "Post not found"),
            AppError::DocumentNotFound => (StatusCode::NOT_FOUND, "Document not found"),
            AppError::InvalidObjectId(_) => (StatusCode::BAD_REQUEST, "Invalid ID format"),
            AppError::DuplicateKey => (StatusCode::CONFLICT, "Duplicate entry"),
            AppError::MpesaError(_) => (StatusCode::BAD_GATEWAY, "M-Pesa error"),
            AppError::AuthError => (StatusCode::UNAUTHORIZED, "Authentication failed"),
            AppError::Unauthorized => (StatusCode::FORBIDDEN, "Unauthorized access"),
            AppError::ValidationError(_) => (StatusCode::BAD_REQUEST, "Validation failed"),
            AppError::RateLimitExceeded => (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded"),
            AppError::ServiceUnavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "Service unavailable"),
            AppError::ExternalApi(_) => (StatusCode::BAD_GATEWAY, "External API error"),
        };

        let body = Json(json!({
            "error": error_message,
            "message": self.to_string(),
            "success": false,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        (status, body).into_response()
    }
}

// Manual From implementations
impl From<axum_extra::extract::multipart::MultipartError> for AppError {
    fn from(err: axum_extra::extract::multipart::MultipartError) -> Self {
        AppError::Multipart(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::ValidationError(format!("JSON parsing error: {}", err))
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::ExternalApi(format!("HTTP request failed: {}", err))
    }
}

impl From<mongodb::bson::oid::Error> for AppError {
    fn from(err: mongodb::bson::oid::Error) -> Self {
        AppError::InvalidObjectId(err.to_string())
    }
}

impl From<std::num::ParseFloatError> for AppError {
    fn from(err: std::num::ParseFloatError) -> Self {
        AppError::ValidationError(format!("Number parsing error: {}", err))
    }
}

impl From<std::num::ParseIntError> for AppError {
    fn from(err: std::num::ParseIntError) -> Self {
        AppError::ValidationError(format!("Integer parsing error: {}", err))
    }
}

// Helper conversion functions (not From traits)
pub trait IntoAppError {
    fn into_app_error(self) -> AppError;
}

impl IntoAppError for chrono::format::ParseError {
    fn into_app_error(self) -> AppError {
        AppError::ValidationError(format!("Date parsing error: {}", self))
    }
}

// Helper functions
impl AppError {
    pub fn invalid_data(msg: impl Into<String>) -> Self {
        AppError::ValidationError(msg.into())
    }

    pub fn mpesa(msg: impl Into<String>) -> Self {
        AppError::MpesaError(msg.into())
    }

    pub fn external_api(msg: impl Into<String>) -> Self {
        AppError::ExternalApi(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;