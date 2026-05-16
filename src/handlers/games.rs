use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, to_bson, DateTime as BsonDateTime};
use mongodb::Collection;
use serde::Deserialize;
use serde_json::json;

use crate::errors::{AppError, Result};
use crate::handlers::ws_handler::broadcast_live_match_update;
use crate::models::game::{
    Game, GameQuery, GameStatusUpdate, LiveGameUpdate, LiveGamesResponse, MatchStatistics,
    StatisticsData, TimelineEvent, TimelineEventData, UpdateGameScore, Voter,
};
use crate::state::AppState;

// ============================================================================
// GAME HANDLERS
// ============================================================================

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
    if let Some(is_live) = query.is_live {
        filter.insert("is_live", is_live);
    }
    if let Some(tournament) = &query.tournament {
        filter.insert("tournament", tournament);
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

fn parse_kickoff_utc(date_iso: &str, time_str: &str) -> Option<chrono::DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(date_iso, "%Y-%m-%d").ok()?;
    let time = NaiveTime::parse_from_str(time_str, "%H:%M").ok()?;
    let naive = NaiveDateTime::new(date, time);
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
    const MATCH_DURATION_MINS: i64 = 120;

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
            None => not_started.push(game),
        }
    }

    not_started.sort_by(|a, b| {
        let ka = format!("{} {}", a.date_iso, a.time);
        let kb = format!("{} {}", b.date_iso, b.time);
        ka.cmp(&kb)
    });

    likely_over.sort_by(|a, b| {
        let ka = format!("{} {}", a.date_iso, a.time);
        let kb = format!("{} {}", b.date_iso, b.time);
        kb.cmp(&ka)
    });

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
    Json(payload): Json<UpdateGameScore>,
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
    if let Some(status) = &payload.status {
        update_doc.insert("status", status);
    }
    if let Some(is_live) = payload.is_live {
        update_doc.insert("is_live", is_live);
    }
    if let Some(time_elapsed) = payload.time_elapsed {
        update_doc.insert("time_elapsed", time_elapsed);
    }
    if let Some(period) = &payload.period {
        update_doc.insert("period", period);
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
    Json(payload): Json<GameStatusUpdate>,
) -> Result<Json<Game>> {
    let collection: Collection<Game> = state.db.collection("games");

    let valid_statuses = ["upcoming", "live", "completed"];
    if !valid_statuses.contains(&payload.status.as_str()) {
        return Err(AppError::invalid_data(&format!(
            "Invalid status. Must be one of: {:?}",
            valid_statuses
        )));
    }

    let filter = doc! { "match_id": &match_id };
    let update = doc! { "$set": {
        "status": &payload.status,
        "is_live": payload.is_live,
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

// ============================================================================
// FAST COUNT HANDLERS
// ============================================================================

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

    println!("✅ Fixture {} has {} votes", fixture_id, game.votes);
    Ok(Json(response))
}

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

    println!("✅ Fixture {} has {} comments", fixture_id, game.comments);
    Ok(Json(response))
}

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

    let voters: Vec<serde_json::Value> = game
        .voters
        .iter()
        .map(|v| {
            json!({
                "userId": v.user_id,
                "userName": v.user_name,
                "selection": v.selection,
                "votedAt": v.voted_at.to_chrono().to_rfc3339(),
            })
        })
        .collect();

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

    let games_collection: Collection<Game> = state.db.collection("games");
    let mut results = Vec::new();
    let mut error_count = 0;

    for fixture_id in fixture_ids {
        let filter = doc! { "match_id": &fixture_id };

        match games_collection.find_one(filter).await {
            Ok(Some(game)) => {
                results.push(json!({
                    "fixture_id": fixture_id,
                    "votes": game.votes,
                    "comments": game.comments,
                }));
            }
            Ok(None) => {
                results.push(json!({
                    "fixture_id": fixture_id,
                    "votes": 0,
                    "comments": 0,
                }));
            }
            Err(e) => {
                error_count += 1;
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

    println!("✅ Returning batch counts for {} fixtures", results.len());
    Ok(Json(response))
}

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

// ============================================================================
// TIMELINE HANDLERS
// ============================================================================

pub async fn get_match_timeline(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<Vec<TimelineEvent>>> {
    println!("📜 GET /api/games/{}/timeline called", match_id);

    let collection: Collection<TimelineEvent> = state.db.collection("timeline");
    let filter = doc! { "match_id": &match_id };
    let sort = doc! { "data.minute": 1 };

    let cursor = collection.find(filter).sort(sort).await?;
    let events: Vec<TimelineEvent> = cursor.try_collect().await?;

    println!(
        "✅ Fetched {} timeline events for match {}",
        events.len(),
        match_id
    );
    Ok(Json(events))
}

pub async fn get_match_timeline_by_type(
    State(state): State<AppState>,
    Path((match_id, event_type)): Path<(String, String)>,
) -> Result<Json<Vec<TimelineEvent>>> {
    println!(
        "📜 GET /api/games/{}/timeline/{} called",
        match_id, event_type
    );

    let collection: Collection<TimelineEvent> = state.db.collection("timeline");
    let filter = doc! {
        "match_id": &match_id,
        "event_type": &event_type
    };
    let sort = doc! { "data.minute": 1 };

    let cursor = collection.find(filter).sort(sort).await?;
    let events: Vec<TimelineEvent> = cursor.try_collect().await?;

    println!("✅ Fetched {} {} events", events.len(), event_type);
    Ok(Json(events))
}

pub async fn get_latest_goal(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<Option<TimelineEvent>>> {
    println!("⚽ GET /api/games/{}/latest-goal called", match_id);

    let collection: Collection<TimelineEvent> = state.db.collection("timeline");
    let filter = doc! {
        "match_id": &match_id,
        "event_type": "goal"
    };
    let sort = doc! { "data.minute": -1 };

    let event = collection.find_one(filter).sort(sort).await?;

    Ok(Json(event))
}

pub async fn add_timeline_event(
    State(state): State<AppState>,
    Json(event): Json<TimelineEvent>,
) -> Result<Json<TimelineEvent>> {
    println!("➕ Adding timeline event for match {}", event.match_id);

    let collection: Collection<TimelineEvent> = state.db.collection("timeline");
    collection.insert_one(&event).await?;

    println!("✅ Timeline event added");
    Ok(Json(event))
}

// ============================================================================
// STATISTICS HANDLERS
// ============================================================================

pub async fn get_match_statistics(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<Vec<MatchStatistics>>> {
    println!("📊 GET /api/games/{}/statistics called", match_id);

    let collection: Collection<MatchStatistics> = state.db.collection("statistics");
    let filter = doc! { "match_id": &match_id };
    let sort = doc! { "minute": 1 };

    let cursor = collection.find(filter).sort(sort).await?;
    let stats: Vec<MatchStatistics> = cursor.try_collect().await?;

    println!(
        "✅ Fetched {} statistic snapshots for match {}",
        stats.len(),
        match_id
    );
    Ok(Json(stats))
}

pub async fn get_latest_statistics(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<Option<MatchStatistics>>> {
    println!("📊 GET /api/games/{}/statistics/latest called", match_id);

    let collection: Collection<MatchStatistics> = state.db.collection("statistics");
    let filter = doc! { "match_id": &match_id };
    let sort = doc! { "minute": -1 };

    let stats = collection.find_one(filter).sort(sort).await?;

    Ok(Json(stats))
}

pub async fn get_statistics_at_minute(
    State(state): State<AppState>,
    Path((match_id, minute)): Path<(String, i32)>,
) -> Result<Json<Option<MatchStatistics>>> {
    println!(
        "📊 GET /api/games/{}/statistics/{} called",
        match_id, minute
    );

    let collection: Collection<MatchStatistics> = state.db.collection("statistics");
    let filter = doc! {
        "match_id": &match_id,
        "minute": minute
    };

    let stats = collection.find_one(filter).await?;

    Ok(Json(stats))
}

pub async fn add_statistics_snapshot(
    State(state): State<AppState>,
    Json(stats): Json<MatchStatistics>,
) -> Result<Json<MatchStatistics>> {
    println!(
        "📊 Adding statistics snapshot for match {} at minute {}",
        stats.match_id, stats.minute
    );

    let collection: Collection<MatchStatistics> = state.db.collection("statistics");

    let filter = doc! {
        "match_id": &stats.match_id,
        "minute": stats.minute
    };

    // Convert to BSON, handling the error properly
    let bson_stats = to_bson(&stats)
        .map_err(|e| AppError::InternalServerError(format!("Failed to serialize stats: {}", e)))?;
    let update = doc! { "$set": bson_stats };

    collection.update_one(filter, update).upsert(true).await?;

    println!("✅ Statistics snapshot saved");
    Ok(Json(stats))
}

pub async fn bulk_update_statistics(
    State(state): State<AppState>,
    Json(stats_list): Json<Vec<MatchStatistics>>,
) -> Result<Json<serde_json::Value>> {
    println!("📊 Bulk updating {} statistics records", stats_list.len());

    let collection: Collection<MatchStatistics> = state.db.collection("statistics");

    let mut inserted = 0;
    let mut updated = 0;

    // Use &stats_list to borrow instead of moving
    for stats in &stats_list {
        let filter = doc! {
            "match_id": &stats.match_id,
            "minute": stats.minute
        };

        // Convert to BSON, handling the error properly
        let bson_stats = to_bson(stats).map_err(|e| {
            AppError::internal_server_error(format!("Failed to serialize stats: {}", e))
        })?;
        let update = doc! { "$set": bson_stats };

        let result = collection.update_one(filter, update).upsert(true).await?;

        if result.upserted_id.is_some() {
            inserted += 1;
        } else if result.modified_count > 0 {
            updated += 1;
        }
    }

    // Now stats_list is still available because we borrowed it
    let response = json!({
        "success": true,
        "inserted": inserted,
        "updated": updated,
        "total": stats_list.len(),
    });

    Ok(Json(response))
}

// ============================================================================
// LIVE UPDATE HANDLER (Called by Python Poller)
// ============================================================================

// In your game_routes.rs, update receive_live_update:
pub async fn receive_live_update(
    State(state): State<AppState>,
    Json(update): Json<LiveGameUpdate>,
) -> Result<Json<serde_json::Value>> {
    println!("🔴 Live update received: {:?}", update);

    let games_col: Collection<Game> = state.db.collection("games");
    let filter = doc! { "match_id": &update.fixture_id };

    // ========== 1. UPDATE GAMES COLLECTION ==========
    let mut set_doc = doc! {
        "home_score": update.home_score,
        "away_score": update.away_score,
        "time_elapsed": update.minute,
        "last_polled_at": BsonDateTime::from_chrono(Utc::now()),
    };

    if update.event_type == "goal" {
        set_doc.insert("last_goal_at", BsonDateTime::from_chrono(Utc::now()));
        set_doc.insert("last_goal_minute", update.minute);
        if let Some(ref scorer) = update.scorer {
            set_doc.insert("last_goal_scorer", scorer);
        }
    }

    games_col
        .update_one(filter.clone(), doc! { "$set": set_doc })
        .await?;

    // ========== 2. INSERT INTO TIMELINE COLLECTION ==========
    if update.event_type == "goal" {
        let timeline_col: Collection<TimelineEvent> = state.db.collection("timeline");

        let timeline_data = TimelineEventData {
            minute: Some(update.minute),
            scorer: update.scorer.clone(),
            scored_team: update.team.clone(),
            player: update.player.clone(),
            team: update.team.clone(),
            home_score: Some(update.home_score),
            away_score: Some(update.away_score),
            score_display: Some(format!("{}-{}", update.home_score, update.away_score)),
        };

        let timeline_event = TimelineEvent {
            match_id: update.fixture_id.clone(),
            event_type: "goal".to_string(),
            data: timeline_data,
            timestamp: BsonDateTime::from_chrono(Utc::now()),
        };

        timeline_col.insert_one(timeline_event).await?;
    }

    // ========== 3. BROADCAST TO WEBSOCKET ==========
    broadcast_live_match_update(
        &state,
        &update.fixture_id,
        &update.event_type,
        json!(update),
    )
    .await;

    // ========== 4. SEND PUSH NOTIFICATIONS TO VOTERS ==========
    if update.event_type == "goal" {
        // Fetch the updated fixture with voters
        if let Some(fixture) = games_col.find_one(filter).await? {
            let voters = fixture.voters;

            if !voters.is_empty() {
                let home_team = fixture.home_team;
                let away_team = fixture.away_team;
                let scored_team = if update.scorer == Some("home_team".to_string()) {
                    &home_team
                } else {
                    &away_team
                };
                let score_line = format!("{}-{}", update.home_score, update.away_score);

                for voter in &voters {
                    let (title, body, ntype) = if Some(&voter.selection) == update.scorer.as_ref() {
                        (
                            format!("⚽ GOAL! Your team scored!"),
                            format!("{} scores! {} ({})", scored_team, score_line, update.minute),
                            "goal_your_team".to_string(),
                        )
                    } else if voter.selection == "draw" {
                        (
                            format!("⚽ Goal! Draw under pressure"),
                            format!(
                                "{} scores → {} ({})",
                                scored_team, score_line, update.minute
                            ),
                            "goal_draw_pressure".to_string(),
                        )
                    } else {
                        (
                            format!("⚔️ RIVAL SCORED!"),
                            format!(
                                "Your rival's team ({}) scored! {} ({})",
                                scored_team, score_line, update.minute
                            ),
                            "goal_rival_team".to_string(),
                        )
                    };

                    // Send push notification (you need to implement this)
                    // send_push_notification(&voter.user_id, &title, &body, &ntype, &update).await;

                    tracing::info!(
                        "📲 Would send notification to {}: {} - {}",
                        voter.user_id,
                        title,
                        body
                    );
                }

                tracing::info!(
                    "📲 Goal notifications ready for {} voters for fixture {}",
                    voters.len(),
                    update.fixture_id
                );
            }
        }
    }

    Ok(Json(json!({
        "success": true,
        "message": "Live update processed",
        "fixture_id": update.fixture_id,
        "event_type": update.event_type,
    })))
}
// ============================================================================
// BULK UPDATE HANDLERS (for poller)
// ============================================================================

pub async fn bulk_add_timeline_events(
    State(state): State<AppState>,
    Json(events): Json<Vec<TimelineEvent>>,
) -> Result<Json<serde_json::Value>> {
    println!("📜 Bulk adding {} timeline events", events.len());

    let collection: Collection<TimelineEvent> = state.db.collection("timeline");

    let mut inserted = 0;
    for event in &events {
        collection.insert_one(event).await?;
        inserted += 1;
    }

    let response = json!({
        "success": true,
        "inserted": inserted,
        "total": events.len(),
    });

    Ok(Json(response))
}
