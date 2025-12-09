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
use crate::models::game::{Game, LiveUpdate, LiveGamesResponse, GameQuery as ModelGameQuery};
use crate::errors::{AppError, Result};

#[derive(Debug, Deserialize)]
pub struct GameQuery {
    pub status: Option<String>,
    pub league: Option<String>,
    pub home_team: Option<String>,
    pub away_team: Option<String>,
    pub is_live: Option<bool>,
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
    if let Some(is_live) = query.is_live {
        filter.insert("is_live", is_live);
    }

    let cursor = collection.find(filter).await?;
    let mut games: Vec<Game> = cursor.try_collect().await?;

    // Sort by last_updated descending (most recent first)
    games.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));

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

pub async fn get_game_by_match_id(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<Game>> {
    println!("🔍 GET /api/games/match/{}", match_id);

    let collection: Collection<Game> = state.db.collection("games");

    let filter = doc! { "match_id": &match_id };

    match collection.find_one(filter).await? {
        Some(game) => {
            println!("✅ Found game: {} vs {}", game.home_team, game.away_team);
            Ok(Json(game))
        }
        None => {
            println!("❌ Game not found with match_id: {}", match_id);
            Err(AppError::DocumentNotFound)
        }
    }
}

pub async fn get_live_games(
    State(state): State<AppState>,
) -> Result<Json<LiveGamesResponse>> {
    println!("🔥 GET /api/games/live called");

    let collection: Collection<Game> = state.db.collection("games");

    let filter = doc! {
        "status": "live",
        "is_live": true
    };

    let cursor = collection.find(filter).await?;
    let live_games: Vec<Game> = cursor.try_collect().await?;

    // Get count BEFORE moving live_games
    let count = live_games.len();

    // Get current time as fallback
    let current_time = BsonDateTime::from_chrono(Utc::now());

    // Find max timestamp using references only
    let max_timestamp = live_games.iter()
        .map(|g| g.last_updated.timestamp_millis())
        .max()
        .unwrap_or_else(|| current_time.timestamp_millis());

    // Create new BsonDateTime from milliseconds
    let last_updated = BsonDateTime::from_millis(max_timestamp);

    // Create response - now live_games is only used once
    let response = LiveGamesResponse {
        live_games,  // This moves live_games into the response
        count,
        last_updated,
    };

    println!("✅ Successfully fetched {} live games", count);
    Ok(Json(response))
}
pub async fn get_upcoming_games(
    State(state): State<AppState>,
) -> Result<Json<Vec<Game>>> {
    println!("⏳ GET /api/games/upcoming called");

    let collection: Collection<Game> = state.db.collection("games");

    let filter = doc! {
        "status": "upcoming"
    };

    let cursor = collection.find(filter).await?;
    let mut games: Vec<Game> = cursor.try_collect().await?;

    // Sort by date and time (earliest first)
    games.sort_by(|a, b| {
        let date_time_a = format!("{} {}", a.date, a.time);
        let date_time_b = format!("{} {}", b.date, b.time);
        date_time_a.cmp(&date_time_b)
    });

    println!("✅ Successfully fetched {} upcoming games", games.len());
    Ok(Json(games))
}

pub async fn update_game_score(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
    Json(payload): Json<LiveUpdate>,
) -> Result<Json<Game>> {
    println!("📝 Updating game score for match_id: {}", match_id);

    let collection: Collection<Game> = state.db.collection("games");

    let filter = doc! { "match_id": &match_id };

    let mut update_doc = doc! {};

    if let Some(home_score) = payload.home_score {
        update_doc.insert("home_score", home_score);
    }

    if let Some(away_score) = payload.away_score {
        update_doc.insert("away_score", away_score);
    }

    // Always update last_updated timestamp
    update_doc.insert("last_updated", BsonDateTime::from_chrono(Utc::now()));

    let update = doc! {
        "$set": update_doc
    };

    // First, update the document
    let update_result = collection.update_one(filter.clone(), update).await?;

    if update_result.matched_count == 0 {
        println!("❌ Game not found with match_id: {}", match_id);
        return Err(AppError::DocumentNotFound);
    }

    // Then, fetch the updated document
    match collection.find_one(filter).await? {
        Some(game) => {
            println!("✅ Updated game {} score to {}-{}",
                     match_id,
                     game.home_score.unwrap_or(0),
                     game.away_score.unwrap_or(0));
            Ok(Json(game))
        }
        None => {
            println!("❌ Game not found after update: {}", match_id);
            Err(AppError::DocumentNotFound)
        }
    }
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

    // Count by status (using your database status values)
    let upcoming_games = games.iter()
        .filter(|g| g.status == "upcoming")
        .count() as i64;

    let live_games = games.iter()
        .filter(|g| g.status == "live")
        .count() as i64;

    let completed_games = games.iter()
        .filter(|g| g.status == "completed")
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
            "upcoming": upcoming_games,
            "live": live_games,
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
                "match_id": g.match_id.clone(),
                "match": format!("{} vs {}", g.home_team, g.away_team),
                "league": g.league.clone(),
                "status": g.status.clone(),
                "is_live": g.is_live,
                "date": g.date.clone(),
                "time": g.time.clone(),
                "score": if let (Some(h), Some(a)) = (g.home_score, g.away_score) {
                    format!("{}-{}", h, a)
                } else {
                    "TBD".to_string()
                },
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

    // Sort by last_updated descending (most recent first)
    games.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));

    // Take only last 10
    let recent_games: Vec<Game> = games.into_iter().take(10).collect();

    println!("✅ Successfully fetched {} recent games", recent_games.len());
    Ok(Json(recent_games))
}

pub async fn update_game_status(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Game>> {
    println!("📝 Updating game status for match_id: {}", match_id);

    let collection: Collection<Game> = state.db.collection("games");

    let status = payload.get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::invalid_data("Status is required"))?;

    // Validate status value (using your database status values)
    let valid_statuses = vec!["upcoming", "live", "completed"];
    if !valid_statuses.contains(&status) {
        return Err(AppError::invalid_data(
            &format!("Invalid status. Must be one of: {:?}", valid_statuses)
        ));
    }

    let filter = doc! { "match_id": &match_id };

    // Update is_live based on status
    let is_live = status == "live";

    let update = doc! {
        "$set": {
            "status": status,
            "is_live": is_live,
            "last_updated": BsonDateTime::from_chrono(Utc::now())
        }
    };

    // First, update the document
    let update_result = collection.update_one(filter.clone(), update).await?;

    if update_result.matched_count == 0 {
        println!("❌ Game not found with match_id: {}", match_id);
        return Err(AppError::DocumentNotFound);
    }

    // Then, fetch the updated document
    match collection.find_one(filter).await? {
        Some(game) => {
            println!("✅ Updated game {} to status: {}", match_id, status);
            Ok(Json(game))
        }
        None => {
            println!("❌ Game not found after update: {}", match_id);
            Err(AppError::DocumentNotFound)
        }
    }
}