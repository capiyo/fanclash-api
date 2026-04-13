use axum::{routing::get, Router};

use crate::handlers::games;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(games::get_upcoming_games))
        .route("/stats", get(games::get_game_stats))
        .route("/recent", get(games::get_recent_games))
        .route("/:id", get(games::get_game_by_id))
}
