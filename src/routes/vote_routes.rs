use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::state::AppState;

pub fn vote_routes() -> Router<AppState> {
    Router::new()
        // ========== VOTE ROUTES ==========
        .route("/vote", post(crate::handlers::vote_handlers::create_vote))
        .route("/votes", get(crate::handlers::vote_handlers::get_votes))
        .route(
            "/votes/bulk",
            post(crate::handlers::vote_handlers::bulk_create_votes),
        )
        .route(
            "/votes/user/:voter_id",
            get(crate::handlers::vote_handlers::get_user_votes),
        )
        .route(
            "/votes/fixture/:fixture_id",
            get(crate::handlers::vote_handlers::get_fixture_votes),
        )
        .route(
            "/votes/fixture/:fixture_id/total",
            get(crate::handlers::vote_handlers::get_total_votes_for_fixture),
        )
        .route(
            "/votes/fixture/:fixture_id/selection",
            get(crate::handlers::vote_handlers::get_vote_counts_by_selection),
        )
        .route(
            "/votes/fixture/:fixture_id/user/:voter_id",
            get(crate::handlers::vote_handlers::get_user_vote_for_fixture),
        )
        .route(
            "/votes/:vote_id",
            delete(crate::handlers::vote_handlers::delete_vote),
        )
        // ========== LIKE ROUTES ==========
        .route("/like", post(crate::handlers::vote_handlers::create_like))
        .route(
            "/likes/fixture/:fixture_id",
            get(crate::handlers::vote_handlers::get_fixture_likes),
        )
        .route(
            "/likes/fixture/:fixture_id/total",
            get(crate::handlers::vote_handlers::get_total_likes_for_fixture),
        )
        .route(
            "/likes/fixture/:fixture_id/user/:voter_id",
            get(crate::handlers::vote_handlers::get_user_like_for_fixture),
        )
        .route(
            "/likes/:like_id",
            delete(crate::handlers::vote_handlers::delete_like),
        )
        // ========== COMMENT ROUTES ==========
        .route(
            "/comment",
            post(crate::handlers::vote_handlers::create_comment),
        )
        .route(
            "/comments",
            get(crate::handlers::vote_handlers::get_comments),
        )
        .route(
            "/comments/fixture/:fixture_id",
            get(crate::handlers::vote_handlers::get_fixture_comments),
        )
        .route(
            "/comments/fixture/:fixture_id/total",
            get(crate::handlers::vote_handlers::get_total_comments_for_fixture),
        )
        .route(
            "/comments/user/:voter_id",
            get(crate::handlers::vote_handlers::get_user_comments),
        )
        .route(
            "/comments/:comment_id",
            delete(crate::handlers::vote_handlers::delete_comment),
        )
        .route(
            "/comments/:comment_id/like",
            post(crate::handlers::vote_handlers::like_comment),
        )
        // ========== 🆕 SUB-FIXTURE (PROP BETS) ROUTES ==========
        // Get all sub-fixtures for a fixture
        .route(
            "/sub-fixtures",
            get(crate::handlers::sub_fixture_handler::get_sub_fixtures),
        )
        // Get a single sub-fixture by ID
        .route(
            "/sub-fixture/:id",
            get(crate::handlers::sub_fixture_handler::get_sub_fixture_by_id),
        )
        // Submit a vote on a sub-fixture
        .route(
            "/sub-fixture",
            post(crate::handlers::sub_fixture_handler::submit_sub_fixture_vote),
        )
        // Get statistics for a sub-fixture
        .route(
            "/sub-fixture/:id/stats",
            get(crate::handlers::sub_fixture_handler::get_sub_fixture_stats),
        )
        // Get voters for a sub-fixture (optionally filtered by selection)
        .route(
            "/sub-fixture/:id/voters",
            get(crate::handlers::sub_fixture_handler::get_sub_fixture_voters),
        )
        // Get user's votes for a specific fixture (all sub-fixtures)
        .route(
            "/user/:user_id/fixture/:fixture_id/sub-votes",
            get(crate::handlers::sub_fixture_handler::get_user_sub_fixture_votes),
        )
        // Get all votes for a sub-fixture (admin)
        .route(
            "/sub-fixture/:id/all-votes",
            get(crate::handlers::sub_fixture_handler::get_all_sub_fixture_votes),
        )
        // Get vote counts for a sub-fixture (for chart display)
        .route(
            "/sub-fixture/:id/counts",
            get(crate::handlers::sub_fixture_handler::get_sub_fixture_vote_counts),
        )
        // Check if user has voted on a specific sub-fixture
        .route(
            "/sub-fixture/:id/user/:user_id",
            get(crate::handlers::sub_fixture_handler::check_user_sub_fixture_vote),
        )
        // Get all sub-fixtures for a fixture with user votes included
        .route(
            "/sub-fixtures/fixture/:fixture_id/user/:user_id",
            get(crate::handlers::sub_fixture_handler::get_sub_fixtures_with_user_votes),
        )
}

pub fn vote_stats_routes() -> Router<AppState> {
    Router::new()
        // ========== STATISTICS ROUTES ==========
        .route(
            "/stats/votes/:fixture_id",
            get(crate::handlers::vote_handlers::get_vote_stats),
        )
        .route(
            "/stats/likes/:fixture_id",
            get(crate::handlers::vote_handlers::get_like_stats),
        )
        .route(
            "/stats/comments/:fixture_id",
            get(crate::handlers::vote_handlers::get_comment_stats),
        )
        .route(
            "/stats/fixture/:fixture_id",
            get(crate::handlers::vote_handlers::get_fixture_stats),
        )
        .route(
            "/stats/fixture/:fixture_id/all",
            get(crate::handlers::vote_handlers::get_all_counts_for_fixture),
        )
        .route(
            "/stats/fixture/:fixture_id/engagement",
            get(crate::handlers::vote_handlers::get_fixture_engagement_summary),
        )
        .route(
            "/stats/user/:voter_id",
            get(crate::handlers::vote_handlers::get_user_stats),
        )
        .route(
            "/stats/total",
            get(crate::handlers::vote_handlers::get_total_counts),
        )
        .route(
            "/stats/batch",
            post(crate::handlers::vote_handlers::get_batch_fixture_counts),
        )
        .route(
            "/stats/comments/bulk",
            post(crate::handlers::vote_handlers::get_comment_counts_for_multiple_fixtures),
        )
        .route(
            "/stats/likes/bulk",
            post(crate::handlers::vote_handlers::get_total_likes_for_multiple_fixtures),
        )
        .route(
            "/stats/combined/bulk",
            post(crate::handlers::vote_handlers::get_combined_stats_for_multiple_fixtures),
        )
        // ========== 🆕 SUB-FIXTURE STATS ROUTES ==========
        // Get stats for multiple sub-fixtures (bulk)
        .route(
            "/stats/sub-fixtures/bulk",
            post(crate::handlers::sub_fixture_handler::get_bulk_sub_fixture_stats),
        )
        // Get trending sub-fixtures (most votes)
        .route(
            "/stats/sub-fixtures/trending",
            get(crate::handlers::sub_fixture_handler::get_trending_sub_fixtures),
        )
        // ========== REAL-TIME & TRENDING ROUTES ==========
        .route(
            "/realtime/:fixture_id",
            get(crate::handlers::vote_handlers::get_realtime_vote_updates),
        )
}

pub fn vote_admin_routes() -> Router<AppState> {
    Router::new()
        // ========== ADMIN ROUTES ==========
        .route(
            "/admin/votes/cleanup",
            post(crate::handlers::vote_handlers::cleanup_old_votes),
        )
        .route(
            "/admin/stats/overview",
            get(crate::handlers::vote_handlers::get_overview_stats),
        )
        // ========== 🆕 SUB-FIXTURE ADMIN ROUTES ==========
        .route(
            "/admin/sub-fixtures",
            post(crate::handlers::sub_fixture_handler::create_sub_fixture),
        )
        .route(
            "/admin/sub-fixture/:id",
            delete(crate::handlers::sub_fixture_handler::delete_sub_fixture),
        )
}

// ========== 🔥 NEW FCM NOTIFICATION ROUTES ==========
pub fn notification_routes() -> Router<AppState> {
    Router::new()
        // Token registration
        .route(
            "/register-token",
            post(crate::handlers::notification_handler::register_token),
        )
        // Sending notifications
        .route(
            "/send",
            post(crate::handlers::notification_handler::send_notification),
        )
        .route(
            "/send-bulk",
            post(crate::handlers::notification_handler::send_bulk_notifications),
        )
        // Getting user notifications
        .route(
            "/user/:user_id",
            get(crate::handlers::notification_handler::get_user_notifications),
        )
        // Marking as read
        .route(
            "/mark-read",
            post(crate::handlers::notification_handler::mark_notifications_read),
        )
        // Notification preferences
        .route(
            "/preferences/:user_id",
            get(crate::handlers::notification_handler::get_notification_preferences),
        )
        .route(
            "/preferences",
            post(crate::handlers::notification_handler::update_notification_preferences),
        )
        // Admin cleanup
        .route(
            "/cleanup",
            post(crate::handlers::notification_handler::cleanup_expired_tokens),
        )
}
