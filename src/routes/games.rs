use axum::{
    routing::{get, post},
    Router,
};

use crate::handlers::games;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Existing routes
        .route("/", get(games::get_upcoming_games))
        .route("/stats", get(games::get_game_stats))
        .route("/recent", get(games::get_recent_games))
        .route("/:id", get(games::get_game_by_id))
        // Fast count endpoints
        .route(
            "/fixture/:fixture_id/votes/fast",
            get(games::get_fixture_vote_count_fast),
        )
        .route(
            "/fixture/:fixture_id/comments/fast",
            get(games::get_fixture_comment_count_fast),
        )
        .route(
            "/fixture/:fixture_id/counts/fast",
            get(games::get_fixture_counts_fast),
        )
        .route(
            "/fixture/counts/batch",
            post(games::get_batch_fixture_counts_fast),
        )
        // ========== NEW: Voter endpoints ==========
        .route(
            "/fixture/:fixture_id/voters",
            get(games::get_fixture_voters_fast),
        )
        .route(
            "/fixture/:fixture_id/user/:user_id/voted",
            get(games::check_user_voted_fast),
        )
}
