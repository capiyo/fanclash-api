use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(crate::handlers::auth::register))
        .route("/login", post(crate::handlers::auth::login))
        .route("/users", get(crate::handlers::auth::get_all_users))
        .route(
            "/users/:user_id",
            get(crate::handlers::auth::get_user_by_id),
        )
}
