use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // ========== EXISTING AUTH ROUTES ==========
        .route("/register", post(crate::handlers::auth::register))
        .route("/login", post(crate::handlers::auth::login))
        .route(
            "/login/phone",
            post(crate::handlers::auth::login_with_phone),
        )
        .route("/users", get(crate::handlers::auth::get_all_users))
        .route("/profile/:id", get(crate::handlers::auth::get_user_profile))
        // ========== FORGOT PASSWORD ROUTES ==========
        .route(
            "/forgot-password",
            post(crate::handlers::auth::forgot_password),
        )
        .route(
            "/verify-reset-otp",
            post(crate::handlers::auth::verify_reset_otp),
        )
        .route(
            "/reset-password",
            post(crate::handlers::auth::reset_password),
        )
        // ========== REGISTRATION OTP ROUTES ==========
        .route(
            "/send-otp",
            post(crate::handlers::auth::send_registration_otp),
        )
        .route(
            "/verify-otp",
            post(crate::handlers::auth::verify_registration_otp),
        )
        .route(
            "/register-with-otp",
            post(crate::handlers::auth::register_with_otp),
        )
}
