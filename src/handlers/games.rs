use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, DateTime as BsonDateTime};
use mongodb::Collection;
use serde::Deserialize;
use serde_json::json;

use crate::errors::{AppError, Result};
use crate::models::game::{Game, GameQuery as ModelGameQuery, LiveGamesResponse, LiveUpdate};
use crate::state::AppState;

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
    println!("🔍 GET /api/games called with query: {:?}", query);
    let start_time = std::time::Instant::now();

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

    games.sort_by(|a, b| b.scraped_at.cmp(&a.scraped_at));

    let elapsed = start_time.elapsed();
    println!("✅ Fetched {} games in {:?}", games.len(), elapsed);
    Ok(Json(games))
}

pub async fn get_game_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Game>> {
    let collection: Collection<Game> = state.db.collection("games");
    let filter = doc! { "_id": &id };

    match collection.find_one(filter).await? {
        Some(game) => Ok(Json(game)),
        None => Err(AppError::DocumentNotFound),
    }
}

pub async fn get_game_by_match_id(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<Game>> {
    let collection: Collection<Game> = state.db.collection("games");
    let filter = doc! { "match_id": &match_id };

    match collection.find_one(filter).await? {
        Some(game) => Ok(Json(game)),
        None => Err(AppError::DocumentNotFound),
    }
}

pub async fn get_live_games(State(state): State<AppState>) -> Result<Json<LiveGamesResponse>> {
    let collection: Collection<Game> = state.db.collection("games");
    let filter = doc! { "status": "live", "is_live": true };

    let cursor = collection.find(filter).await?;
    let live_games: Vec<Game> = cursor.try_collect().await?;
    let count = live_games.len();

    let current_time = BsonDateTime::from_chrono(Utc::now());
    let max_timestamp = live_games
        .iter()
        .map(|g| g.scraped_at.timestamp_millis())
        .max()
        .unwrap_or_else(|| current_time.timestamp_millis());

    let response = LiveGamesResponse {
        live_games,
        count,
        last_updated: BsonDateTime::from_millis(max_timestamp),
    };

    println!("✅ Fetched {} live games", count);
    Ok(Json(response))
}

/// Parse a game's kickoff time from its date_iso ("2026-04-08") and time ("22:00")
/// into a UTC chrono timestamp. Times in the DB are stored as EAT (UTC+3).
fn parse_kickoff_utc(date_iso: &str, time_str: &str) -> Option<chrono::DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(date_iso, "%Y-%m-%d").ok()?;
    let time = NaiveTime::parse_from_str(time_str, "%H:%M").ok()?;
    let naive = NaiveDateTime::new(date, time);
    // Times stored as EAT (UTC+3) — subtract 3 hours to get UTC
    let utc = chrono::FixedOffset::east_opt(3 * 3600)?
        .from_local_datetime(&naive)
        .single()?
        .with_timezone(&Utc);
    Some(utc)
}

pub async fn get_upcoming_games(State(state): State<AppState>) -> Result<Json<Vec<Game>>> {
    println!("⏳ GET /api/games/upcoming called");
    let start_time = std::time::Instant::now();

    let collection: Collection<Game> = state.db.collection("games");
    let filter = doc! { "status": "upcoming" };

    let cursor = collection.find(filter).await?;
    let games: Vec<Game> = cursor.try_collect().await?;

    println!("   → Fetched {} upcoming games", games.len());

    let now = Utc::now();
    const MATCH_DURATION_MINS: i64 = 120; // 90 min + extra time buffer

    // Split into two buckets:
    //   - not_started: kickoff hasn't passed 120 min window yet → sort soonest first
    //   - likely_over:  kickoff + 120 min < now → sort most recently kicked off first
    let mut not_started: Vec<&Game> = Vec::new();
    let mut likely_over: Vec<&Game> = Vec::new();

    for game in &games {
        match parse_kickoff_utc(&game.date_iso, &game.time) {
            Some(kickoff) => {
                let end_estimate = kickoff + chrono::Duration::minutes(MATCH_DURATION_MINS);
                if end_estimate < now {
                    likely_over.push(game);
                } else {
                    not_started.push(game);
                }
            }
            // Can't parse time (TBD etc.) — treat as not started
            None => not_started.push(game),
        }
    }

    // Not started: soonest kickoff first
    not_started.sort_by(|a, b| {
        let ka = format!("{} {}", a.date_iso, a.time);
        let kb = format!("{} {}", b.date_iso, b.time);
        ka.cmp(&kb)
    });

    // Likely over: most recent kickoff first (most relevant banter at top)
    likely_over.sort_by(|a, b| {
        let ka = format!("{} {}", a.date_iso, a.time);
        let kb = format!("{} {}", b.date_iso, b.time);
        kb.cmp(&ka)
    });

    // Combine: upcoming games first, then likely-finished games at the bottom
    let mut sorted: Vec<Game> = not_started
        .into_iter()
        .chain(likely_over)
        .cloned()
        .collect();

    let elapsed = start_time.elapsed();
    println!(
        "✅ Returning {} upcoming games (sorted) in {:?}",
        sorted.len(),
        elapsed
    );
    Ok(Json(sorted))
}

pub async fn update_game_score(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
    Json(payload): Json<LiveUpdate>,
) -> Result<Json<Game>> {
    let collection: Collection<Game> = state.db.collection("games");
    let filter = doc! { "match_id": &match_id };
    let mut update_doc = doc! {};

    if let Some(home_score) = payload.home_score {
        update_doc.insert("home_score", home_score);
    }
    if let Some(away_score) = payload.away_score {
        update_doc.insert("away_score", away_score);
    }
    update_doc.insert("scraped_at", BsonDateTime::from_chrono(Utc::now()));

    let update_result = collection
        .update_one(filter.clone(), doc! { "$set": update_doc })
        .await?;
    if update_result.matched_count == 0 {
        return Err(AppError::DocumentNotFound);
    }

    match collection.find_one(filter).await? {
        Some(game) => Ok(Json(game)),
        None => Err(AppError::DocumentNotFound),
    }
}

pub async fn get_game_stats(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Game> = state.db.collection("games");
    let cursor = collection.find(doc! {}).await?;
    let games: Vec<Game> = cursor.try_collect().await?;

    let total_games = games.len() as i64;
    let upcoming_games = games.iter().filter(|g| g.status == "upcoming").count() as i64;
    let live_games = games.iter().filter(|g| g.status == "live").count() as i64;
    let completed_games = games.iter().filter(|g| g.status == "completed").count() as i64;

    use std::collections::HashMap;
    let mut league_counts: HashMap<String, i64> = HashMap::new();
    for game in &games {
        *league_counts.entry(game.league.clone()).or_insert(0) += 1;
    }
    let league_stats: Vec<_> = league_counts
        .into_iter()
        .map(|(league, count)| serde_json::json!({ "league": league, "count": count }))
        .collect();

    let total_f64 = total_games as f64;
    let avg_home_win = if total_games > 0 {
        games.iter().map(|g| g.home_win).sum::<f64>() / total_f64
    } else {
        0.0
    };
    let avg_away_win = if total_games > 0 {
        games.iter().map(|g| g.away_win).sum::<f64>() / total_f64
    } else {
        0.0
    };
    let avg_draw = if total_games > 0 {
        games.iter().map(|g| g.draw).sum::<f64>() / total_f64
    } else {
        0.0
    };

    let recent_games_json: Vec<_> = games
        .iter()
        .take(5)
        .map(|g| {
            serde_json::json!({
                "match_id": g.match_id,
                "match":    format!("{} vs {}", g.home_team, g.away_team),
                "league":   g.league,
                "status":   g.status,
                "is_live":  g.is_live,
                "date":     g.date,
                "time":     g.time,
                "score": if let (Some(h), Some(a)) = (g.home_score, g.away_score) {
                    format!("{}-{}", h, a)
                } else { "TBD".to_string() },
                "odds": { "home_win": g.home_win, "away_win": g.away_win, "draw": g.draw }
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "total_games": total_games,
        "by_status": { "upcoming": upcoming_games, "live": live_games, "completed": completed_games },
        "by_league": league_stats,
        "average_odds": { "home_win": avg_home_win, "away_win": avg_away_win, "draw": avg_draw },
        "recent_games": recent_games_json
    })))
}

pub async fn get_recent_games(State(state): State<AppState>) -> Result<Json<Vec<Game>>> {
    let collection: Collection<Game> = state.db.collection("games");
    let cursor = collection.find(doc! {}).await?;
    let mut games: Vec<Game> = cursor.try_collect().await?;

    games.sort_by(|a, b| b.scraped_at.cmp(&a.scraped_at));
    let recent_games: Vec<Game> = games.into_iter().take(10).collect();

    println!("✅ Fetched {} recent games", recent_games.len());
    Ok(Json(recent_games))
}

pub async fn update_game_status(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Game>> {
    let collection: Collection<Game> = state.db.collection("games");

    let status = payload
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::invalid_data("Status is required"))?;

    let valid_statuses = ["upcoming", "live", "completed"];
    if !valid_statuses.contains(&status) {
        return Err(AppError::invalid_data(&format!(
            "Invalid status. Must be one of: {:?}",
            valid_statuses
        )));
    }

    let filter = doc! { "match_id": &match_id };
    let is_live = status == "live";
    let update = doc! { "$set": {
        "status":     status,
        "is_live":    is_live,
        "scraped_at": BsonDateTime::from_chrono(Utc::now()),
    }};

    let result = collection.update_one(filter.clone(), update).await?;
    if result.matched_count == 0 {
        return Err(AppError::DocumentNotFound);
    }

    match collection.find_one(filter).await? {
        Some(game) => Ok(Json(game)),
        None => Err(AppError::DocumentNotFound),
    }
}

// ========== FAST VOTE COUNT ENDPOINT ==========
// This reads the vote count directly from the games collection counter
// No need to count millions of vote documents!

pub async fn get_fixture_vote_count_fast(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!("📊 Getting vote count for fixture: {} (FAST)", fixture_id);

    let games_collection: Collection<Game> = state.db.collection("games");
    let filter = doc! { "match_id": &fixture_id };

    let game = games_collection
        .find_one(filter)
        .await?
        .ok_or_else(|| AppError::DocumentNotFound)?;

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "votes": game.votes,
        "timestamp": Utc::now().to_rfc3339(),
    });

    println!(
        "✅ Fixture {} has {} votes (from counter)",
        fixture_id, game.votes
    );
    Ok(Json(response))
}

// ========== FAST COMMENT COUNT ENDPOINT ==========
// This reads the comment count directly from the games collection counter

pub async fn get_fixture_comment_count_fast(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!(
        "📊 Getting comment count for fixture: {} (FAST)",
        fixture_id
    );

    let games_collection: Collection<Game> = state.db.collection("games");
    let filter = doc! { "match_id": &fixture_id };

    let game = games_collection
        .find_one(filter)
        .await?
        .ok_or_else(|| AppError::DocumentNotFound)?;

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "comments": game.comments,
        "timestamp": Utc::now().to_rfc3339(),
    });

    println!(
        "✅ Fixture {} has {} comments (from counter)",
        fixture_id, game.comments
    );
    Ok(Json(response))
}

// ========== FAST BOTH COUNTS ENDPOINT ==========
// Returns both vote and comment counts in one request

pub async fn get_fixture_counts_fast(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!(
        "📊 Getting vote and comment counts for fixture: {} (FAST)",
        fixture_id
    );

    let games_collection: Collection<Game> = state.db.collection("games");
    let filter = doc! { "match_id": &fixture_id };

    let game = games_collection
        .find_one(filter)
        .await?
        .ok_or_else(|| AppError::DocumentNotFound)?;

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "votes": game.votes,
        "comments": game.comments,
        "timestamp": Utc::now().to_rfc3339(),
    });

    println!(
        "✅ Fixture {} has {} votes and {} comments",
        fixture_id, game.votes, game.comments
    );
    Ok(Json(response))
}

// ========== BATCH FAST COUNTS ENDPOINT ==========
// Get counts for multiple fixtures at once

// ========== GET ALL VOTERS FOR A FIXTURE (FOR VOTERS MODAL) ==========

pub async fn get_fixture_voters_fast(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!("📊 Getting voters for fixture: {} (FAST)", fixture_id);

    let games_collection: Collection<Game> = state.db.collection("games");
    let filter = doc! { "match_id": &fixture_id };

    let game = games_collection
        .find_one(filter)
        .await?
        .ok_or_else(|| AppError::DocumentNotFound)?;

    // Format voters for frontend
    let voters: Vec<serde_json::Value> = game
        .voters
        .into_iter()
        .map(|v| {
            json!({
                "userId": v.user_id,
                "userName": v.user_name,
                "selection": v.selection,
                "votedAt": v.voted_at.to_chrono().to_rfc3339(),
            })
        })
        .collect();

    // Calculate vote breakdown
    let home_votes = voters
        .iter()
        .filter(|v| v["selection"] == "home_team")
        .count();
    let draw_votes = voters.iter().filter(|v| v["selection"] == "draw").count();
    let away_votes = voters
        .iter()
        .filter(|v| v["selection"] == "away_team")
        .count();

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "home_team": game.home_team,
        "away_team": game.away_team,
        "total_votes": game.votes,
        "voters": voters,
        "breakdown": {
            "home": home_votes,
            "draw": draw_votes,
            "away": away_votes,
        },
        "timestamp": Utc::now().to_rfc3339(),
    });

    println!(
        "✅ Returning {} voters for fixture {}",
        voters.len(),
        fixture_id
    );
    Ok(Json(response))
}

pub async fn get_batch_fixture_counts_fast(
    State(state): State<AppState>,
    Json(fixture_ids): Json<Vec<String>>,
) -> Result<Json<serde_json::Value>> {
    println!(
        "📊 Getting batch counts for {} fixtures (FAST)",
        fixture_ids.len()
    );
    println!("🔍 Fixture IDs: {:?}", fixture_ids);

    let games_collection: Collection<Game> = state.db.collection("games");
    let mut results = Vec::new();
    let mut error_count = 0;

    for fixture_id in fixture_ids {
        println!("   🔍 Processing: {}", fixture_id);

        let filter = doc! { "match_id": &fixture_id };

        // DON'T use ? here - handle error per item
        match games_collection.find_one(filter).await {
            Ok(Some(game)) => {
                println!("   ✅ Found: votes={}", game.votes);
                results.push(json!({
                    "fixture_id": fixture_id,
                    "votes": game.votes,
                    "comments": game.comments,
                }));
            }
            Ok(None) => {
                println!("   ⚠️ Not found: {}", fixture_id);
                results.push(json!({
                    "fixture_id": fixture_id,
                    "votes": 0,
                    "comments": 0,
                }));
            }
            Err(e) => {
                error_count += 1;
                println!("   ❌ Database error for {}: {}", fixture_id, e);
                results.push(json!({
                    "fixture_id": fixture_id,
                    "votes": 0,
                    "comments": 0,
                    "error": format!("{}", e)
                }));
            }
        }
    }

    let response = json!({
        "success": true,
        "count": results.len(),
        "data": results,
        "errors": error_count,
        "timestamp": Utc::now().to_rfc3339(),
    });

    println!(
        "✅ Returning batch counts for {} fixtures ({} errors)",
        results.len(),
        error_count
    );
    Ok(Json(response))
}

// ========== CHECK IF USER HAS VOTED (FAST) ==========

pub async fn check_user_voted_fast(
    State(state): State<AppState>,
    Path((fixture_id, user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    println!(
        "🔍 Checking if user {} voted for fixture {}",
        user_id, fixture_id
    );

    let games_collection: Collection<Game> = state.db.collection("games");
    let filter = doc! {
        "match_id": &fixture_id,
        "voters.userId": &user_id
    };

    let game = games_collection.find_one(filter).await?;

    let has_voted = game.is_some();
    let selection = if let Some(game) = game {
        game.voters
            .iter()
            .find(|v| v.user_id == user_id)
            .map(|v| v.selection.clone())
    } else {
        None
    };

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "user_id": user_id,
        "has_voted": has_voted,
        "selection": selection,
    });

    Ok(Json(response))
}
