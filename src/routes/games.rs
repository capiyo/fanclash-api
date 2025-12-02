use axum::{
    routing::{get, post},
    Router,
};
use mongodb::Database;

use crate::handlers::games::{get_games,};

pub fn routes() -> Router<Database> {
    Router::new()
        .route("/", get(get_games))
        //.route("/", post(create_game))
}