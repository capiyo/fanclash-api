use axum::{
    routing::{get, post, put},
    Router,
};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", post(crate::handlers::auth::create_user))
        .route("/users", get(crate::handlers::auth::get_all_users))
        .route(
            "/users/:firebase_uid",
            get(crate::handlers::auth::get_user_by_firebase_uid),
        )
        .route(
            "/users/:firebase_uid",
            put(crate::handlers::auth::update_user),
        )
        .route(
            "/user-by-username/:username",
            get(crate::handlers::auth::get_user_by_username),
        )
}
