use axum::{
    routing::{get, post, put},
    Router,
};

use crate::handlers::user_profile::{
    get_user_profiles, get_user_profile_by_id, get_user_profile_by_phone,
    save_user_profile, update_user_balance, get_user_stats,
    get_recent_users, create_user_profile
};
use crate::state::AppState;

pub fn user_profile_routes() -> Router<AppState> {
    Router::new()
        // GET routes
        .route("/api/users/profiles", get(get_user_profiles))
        .route("/api/users/profiles/:id", get(get_user_profile_by_id))
        .route("/api/users/phone/:phone", get(get_user_profile_by_phone))
        .route("/api/users/stats", get(get_user_stats))
        .route("/api/users/recent", get(get_recent_users))

        // POST routes
        .route("/api/users/save-profile", post(save_user_profile))
        .route("/api/users/create", post(create_user_profile))
        .route("/api/users/update-balance", post(update_user_balance))

        // PUT routes (for updates)
        .route("/api/users/profiles/:id", put(save_user_profile))
}