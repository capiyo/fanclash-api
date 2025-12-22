use axum::{
    routing::{delete, get, post, put},
    Router,
};

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(crate::handlers::posta::get_posts))
        .route("/", post(crate::handlers::posta::create_post))
        .route("/:id", get(crate::handlers::posta::get_post_by_id))
        .route("/:id", put(crate::handlers::posta::update_post_caption))
        .route("/:id", delete(crate::handlers::posta::delete_post))
        .route(
            "/user/:user_id",
            get(crate::handlers::posta::get_posts_by_user),
        )
}

pub fn upload_routes() -> Router<AppState> {
    Router::new().route("/upload", post(crate::handlers::posta::create_post))
}
