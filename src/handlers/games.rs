use axum::{
    extract::{State, Query},
    response::Json,
};
use serde::Deserialize;
use mongodb::bson::{doc, oid::ObjectId};
use chrono::Utc;
use mongodb::{Database, Collection};
use futures_util::TryStreamExt;

use crate::models::game::{Game, CreateGame};
use crate::errors::{AppError, Result}; // Use your Result alias

#[derive(Debug, Deserialize)]
pub struct GameQuery {
    pub status: Option<String>,
    pub league: Option<String>,
}

pub async fn get_games(
    State(db): State<Database>,
    Query(query): Query<GameQuery>,
) -> Result<Json<Vec<Game>>> {
    println!("🔍 GET /api/games called - Starting MongoDB query...");

    let collection: Collection<Game> = db.collection("games");
    let mut filter = doc! {};

    if let Some(status) = &query.status {
        filter.insert("status", status);
    }
    if let Some(league) = &query.league {
        filter.insert("league", league);
    }

    let cursor = collection.find(filter).await?;
    let mut games: Vec<Game> = cursor.try_collect().await?;

    // SIMPLIFIED: Direct comparison
    games.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    println!("✅ Successfully fetched {} games", games.len());
    Ok(Json(games))
}
