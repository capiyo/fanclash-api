use axum::{
    extract::{State, Query, Path},
    response::Json,
};
use serde::Deserialize;
use mongodb::bson::{doc, oid::ObjectId, DateTime as BsonDateTime};
use chrono::Utc;
use mongodb::Collection;
use futures_util::TryStreamExt;

use crate::state::AppState;
use crate::models::game::{Game, CreateGame};
use crate::errors::{AppError, Result};

#[derive(Debug, Deserialize)]
pub struct GameQuery {
    pub status: Option<String>,
    pub league: Option<String>,
    pub home_team: Option<String>,
    pub away_team: Option<String>,
}

pub async fn get_games(
    State(state): State<AppState>,
    Query(query): Query<GameQuery>,
) -> Result<Json<Vec<Game>>> {
    println!("🔍 GET /api/games called");

    let collection: Collection<Game> = state.db.collection("games");
    let mut filter = doc! {};

    if let Some(status) = &query.status {
        filter.insert("status", status);
    }
    if let Some(league) = &query.league {
        filter.insert("league", league);
    }
    if let Some(home_team) = &query.home_team {
        filter.insert("home_team", home_team);
    }
    if let Some(away_team) = &query.away_team {
        filter.insert("away_team", away_team);
    }

    let cursor = collection.find(filter).await?;
    let mut games: Vec<Game> = cursor.try_collect().await?;

    // Sort by created_at descending (most recent first)
    games.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    println!("✅ Successfully fetched {} games", games.len());
    Ok(Json(games))
}

pub async fn get_game_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Game>> {
    println!("🔍 GET /api/games/{}", id);

    let collection: Collection<Game> = state.db.collection("games");

    let object_id = ObjectId::parse_str(&id)
        .map_err(|_| AppError::invalid_data("Invalid game ID format"))?;

    let filter = doc! { "_id": object_id };

    match collection.find_one(filter).await? {
        Some(game) => {
            println!("✅ Found game: {} vs {}", game.home_team, game.away_team);
            Ok(Json(game))
        }
        None => {
            println!("❌ Game not found: {}", id);
            Err(AppError::DocumentNotFound)
        }
    }
}

pub async fn create_game(
    State(state): State<AppState>,
    Json(payload): Json<CreateGame>,
) -> Result<Json<Game>> {
    println!("🎯 Creating new game: {} vs {}", payload.home_team, payload.away_team);

    // Validate required fields
    if payload.home_team.is_empty() || payload.away_team.is_empty() || payload.league.is_empty() {
        return Err(AppError::InvalidUserData);
    }

    // Validate odds
    if payload.home_win <= 0.0 || payload.away_win <= 0.0 || payload.draw <= 0.0 {
        return Err(AppError::invalid_data("Odds must be greater than 0"));
    }

    let collection: Collection<Game> = state.db.collection("games");

    let game = Game {
        _id: Some(ObjectId::new()),
        home_team: payload.home_team.clone(),
        away_team: payload.away_team.clone(),
        league: payload.league.clone(),
        home_win: payload.home_win,
        away_win: payload.away_win,
        draw: payload.draw,
        date: payload.date.clone(),
        status: "scheduled".to_string(), // Default status
        created_at: BsonDateTime::from_chrono(Utc::now()),
        updated_at: BsonDateTime::from_chrono(Utc::now()),
    };

    // Insert the game
    collection.insert_one(&game).await?;

    println!("✅ Successfully created game: {} vs {} - {}",
             payload.home_team, payload.away_team, payload.league);
    Ok(Json(game))
}

pub async fn get_game_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    println!("📊 Getting game statistics...");

    let collection: Collection<Game> = state.db.collection("games");

    // Get all games
    let cursor = collection.find(doc! {}).await?;
    let games: Vec<Game> = cursor.try_collect().await?;

    // Calculate statistics
    let total_games = games.len() as i64;

    // Count by status
    let scheduled_games = games.iter()
        .filter(|g| g.status == "scheduled")
        .count() as i64;

    let ongoing_games = games.iter()
        .filter(|g| g.status == "ongoing" || g.status == "live")
        .count() as i64;

    let completed_games = games.iter()
        .filter(|g| g.status == "completed" || g.status == "finished")
        .count() as i64;

    // Count by league
    use std::collections::HashMap;
    let mut league_counts: HashMap<String, i64> = HashMap::new();

    for game in &games {
        *league_counts.entry(game.league.clone()).or_insert(0) += 1;
    }

    let league_stats: Vec<_> = league_counts.into_iter()
        .map(|(league, count)| serde_json::json!({
            "league": league,
            "count": count
        }))
        .collect();

    // Calculate average odds
    let total_games_f64 = total_games as f64;
    let avg_home_win = if total_games > 0 {
        games.iter().map(|g| g.home_win).sum::<f64>() / total_games_f64
    } else { 0.0 };

    let avg_away_win = if total_games > 0 {
        games.iter().map(|g| g.away_win).sum::<f64>() / total_games_f64
    } else { 0.0 };

    let avg_draw = if total_games > 0 {
        games.iter().map(|g| g.draw).sum::<f64>() / total_games_f64
    } else { 0.0 };

    let stats = serde_json::json!({
        "total_games": total_games,
        "by_status": {
            "scheduled": scheduled_games,
            "ongoing": ongoing_games,
            "completed": completed_games
        },
        "by_league": league_stats,
        "average_odds": {
            "home_win": avg_home_win,
            "away_win": avg_away_win,
            "draw": avg_draw
        },
        "recent_games": games.iter()
            .take(5)
            .map(|g| serde_json::json!({
                "id": g._id,
                "match": format!("{} vs {}", g.home_team, g.away_team),
                "league": g.league,
                "status": g.status,
                "date": g.date,
                "odds": {
                    "home_win": g.home_win,
                    "away_win": g.away_win,
                    "draw": g.draw
                }
            }))
            .collect::<Vec<_>>()
    });

    println!("✅ Successfully fetched game statistics");
    Ok(Json(stats))
}

pub async fn get_recent_games(
    State(state): State<AppState>,
) -> Result<Json<Vec<Game>>> {
    println!("🕒 Getting recent games...");

    let collection: Collection<Game> = state.db.collection("games");

    let cursor = collection.find(doc! {}).await?;
    let mut games: Vec<Game> = cursor.try_collect().await?;

    // Sort by created_at descending (most recent first)
    games.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Take only last 10
    let recent_games: Vec<Game> = games.into_iter().take(10).collect();

    println!("✅ Successfully fetched {} recent games", recent_games.len());
    Ok(Json(recent_games))
}

pub async fn update_game_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Game>> {
    println!("📝 Updating game status: {}", id);

    let collection: Collection<Game> = state.db.collection("games");

    let object_id = ObjectId::parse_str(&id)
        .map_err(|_| AppError::invalid_data("Invalid game ID"))?;

    let status = payload.get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::invalid_data("Status is required"))?;

    // Validate status value
    let valid_statuses = vec!["scheduled", "ongoing", "completed", "cancelled"];
    if !valid_statuses.contains(&status) {
        return Err(AppError::invalid_data(
            &format!("Invalid status. Must be one of: {:?}", valid_statuses)
        ));
    }

    let filter = doc! { "_id": object_id };
    let update = doc! {
        "$set": {
            "status": status,
            "updated_at": BsonDateTime::from_chrono(Utc::now())
        }
    };

    // First, update the document
    let update_result = collection.update_one(filter.clone(), update).await?;

    if update_result.matched_count == 0 {
        println!("❌ Game not found: {}", id);
        return Err(AppError::DocumentNotFound);
    }

    // Then, fetch the updated document
    match collection.find_one(filter).await? {
        Some(game) => {
            println!("✅ Updated game {} to status: {}", id, status);
            Ok(Json(game))
        }
        None => {
            println!("❌ Game not found after update: {}", id);
            Err(AppError::DocumentNotFound)
        }
    }
}