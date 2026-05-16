use axum::{
    routing::{get, post, put},
    Router,
};

use crate::handlers::games;
use crate::handlers::lineup_handler; // Add this import
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        // ========== GAME ROUTES ==========
        .route("/", get(games::get_games))
        .route("/upcoming", get(games::get_upcoming_games))
        .route("/live", get(games::get_live_games))
        .route("/stats", get(games::get_game_stats))
        .route("/recent", get(games::get_recent_games))
        .route("/:id", get(games::get_game_by_id))
        .route("/match/:match_id", get(games::get_game_by_match_id))
        .route("/:match_id/score", put(games::update_game_score))
        .route("/:match_id/status", put(games::update_game_status))
        // ========== FAST COUNT ENDPOINTS ==========
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
        // ========== VOTER ENDPOINTS ==========
        .route(
            "/fixture/:fixture_id/voters",
            get(games::get_fixture_voters_fast),
        )
        .route(
            "/fixture/:fixture_id/user/:user_id/voted",
            get(games::check_user_voted_fast),
        )
        // ========== TIMELINE ENDPOINTS ==========
        .route("/:match_id/timeline", get(games::get_match_timeline))
        .route(
            "/:match_id/timeline/:event_type",
            get(games::get_match_timeline_by_type),
        )
        .route("/:match_id/latest-goal", get(games::get_latest_goal))
        .route("/timeline", post(games::add_timeline_event))
        .route("/timeline/bulk", post(games::bulk_add_timeline_events))
        // ========== STATISTICS ENDPOINTS ==========
        .route("/:match_id/statistics", get(games::get_match_statistics))
        .route(
            "/:match_id/statistics/latest",
            get(games::get_latest_statistics),
        )
        .route(
            "/:match_id/statistics/:minute",
            get(games::get_statistics_at_minute),
        )
        .route("/statistics", post(games::add_statistics_snapshot))
        .route("/statistics/bulk", post(games::bulk_update_statistics))
        // ========== LINEUPS ENDPOINTS ==========
        .route("/lineups", post(lineup_handler::receive_lineups_update))
        .route("/:match_id/lineups", get(lineup_handler::get_lineups))
        .route(
            "/:match_id/lineups/simplified",
            get(lineup_handler::get_simplified_lineups),
        )
        .route(
            "/:match_id/lineups/available",
            get(lineup_handler::check_lineups_available),
        )
        // ========== LIVE UPDATE ENDPOINT (Called by Python Poller) ==========
        .route("/live-update", post(games::receive_live_update))
}
