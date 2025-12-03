use axum::{
    routing::{get, post, put, delete},
    Router,
};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(crate::handlers::posts::get_posts))
        .route("/", post(crate::handlers::posts::create_post))
        .route("/:id", get(crate::handlers::posts::get_post_by_id))
        .route("/:id", put(crate::handlers::posts::update_post_caption))
        .route("/:id", delete(crate::handlers::posts::delete_post))
        .route("/user/:user_id", get(crate::handlers::posts::get_posts_by_user))
}

pub fn upload_routes() -> Router<AppState> {
    Router::new()
        .route("/upload", post(crate::handlers::posts::create_post))
}