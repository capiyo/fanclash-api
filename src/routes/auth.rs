use axum::{
    routing::{post},
    Router,
};
use mongodb::Database;

use crate::handlers::auth::{register, login, login_with_phone};

pub fn routes() -> Router<Database> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/login-with-phone", post(login_with_phone))
}