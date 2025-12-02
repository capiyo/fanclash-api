use axum::{
    routing::{get, post, delete, put},
    Router,
};
use mongodb::Database;

pub fn routes() -> Router<Database> {
    Router::new()
        .route("/", get(crate::handlers::posts::get_posts))
        .route("/", post(crate::handlers::posts::create_post))
        .route("/:id", get(crate::handlers::posts::get_post_by_id))
        .route("/user/:user_id", get(crate::handlers::posts::get_posts_by_user))
        .route("/:id", delete(crate::handlers::posts::delete_post))
        .route("/:id/caption", put(crate::handlers::posts::update_post_caption))
}

pub fn upload_routes() -> Router<Database> {
    Router::new()
        .route("/uploads/:filename", get(crate::handlers::upload::serve_image))
}