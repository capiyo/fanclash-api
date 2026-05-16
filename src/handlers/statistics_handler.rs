use axum::{
    extract::{Path, State},
    response::Json,
};
use mongodb::bson::{doc, to_bson, DateTime as BsonDateTime};
use mongodb::Collection;
use serde_json::json;
use tracing;

use crate::errors::{AppError, Result};
use crate::models::statistics::MatchStatistics;
use crate::state::AppState;
use futures_util::TryStreamExt;

// ============================================================================
// GET ALL STATISTICS FOR A MATCH
// ============================================================================

pub async fn get_match_statistics(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<Vec<MatchStatistics>>> {
    tracing::info!("📊 GET /api/games/{}/statistics called", match_id);

    let collection: Collection<MatchStatistics> = state.db.collection("statistics");
    let filter = doc! { "match_id": &match_id };
    let sort = doc! { "minute": 1 };

    let cursor = collection.find(filter).sort(sort).await?;
    let stats: Vec<MatchStatistics> = cursor.try_collect().await?;

    tracing::info!(
        "✅ Fetched {} statistic snapshots for match {}",
        stats.len(),
        match_id
    );
    Ok(Json(stats))
}

// ============================================================================
// GET LATEST STATISTICS
// ============================================================================

pub async fn get_latest_statistics(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<Option<MatchStatistics>>> {
    tracing::info!("📊 GET /api/games/{}/statistics/latest called", match_id);

    let collection: Collection<MatchStatistics> = state.db.collection("statistics");
    let filter = doc! { "match_id": &match_id };
    let sort = doc! { "minute": -1 };

    let stats = collection.find_one(filter).sort(sort).await?;

    Ok(Json(stats))
}

// ============================================================================
// GET STATISTICS AT SPECIFIC MINUTE
// ============================================================================

pub async fn get_statistics_at_minute(
    State(state): State<AppState>,
    Path((match_id, minute)): Path<(String, i32)>,
) -> Result<Json<Option<MatchStatistics>>> {
    tracing::info!(
        "📊 GET /api/games/{}/statistics/{} called",
        match_id,
        minute
    );

    let collection: Collection<MatchStatistics> = state.db.collection("statistics");
    let filter = doc! {
        "match_id": &match_id,
        "minute": minute
    };

    let stats = collection.find_one(filter).await?;

    Ok(Json(stats))
}

// ============================================================================
// ADD STATISTICS SNAPSHOT (Called by Poller)
// ============================================================================

pub async fn add_statistics_snapshot(
    State(state): State<AppState>,
    Json(stats): Json<MatchStatistics>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<MatchStatistics> = state.db.collection("statistics");

    // Just pass &stats - no manual conversion
    collection.insert_one(&stats).await?;

    Ok(Json(json!({ "success": true })))
}

// ============================================================================
// BULK UPDATE STATISTICS
// ============================================================================

pub async fn bulk_update_statistics(
    State(state): State<AppState>,
    Json(stats_list): Json<Vec<MatchStatistics>>,
) -> Result<Json<serde_json::Value>> {
    tracing::info!("📊 Bulk updating {} statistics records", stats_list.len());

    let collection: Collection<MatchStatistics> = state.db.collection("statistics");

    let mut inserted = 0;
    let mut updated = 0;

    for stats in &stats_list {
        let filter = doc! {
            "match_id": &stats.match_id,
            "minute": stats.minute
        };

        let bson_stats = to_bson(stats).map_err(|e| {
            AppError::InternalServerError(format!("Failed to serialize stats: {}", e))
        })?;
        let update = doc! { "$set": bson_stats };

        let result = collection.update_one(filter, update).upsert(true).await?;

        if result.upserted_id.is_some() {
            inserted += 1;
        } else if result.modified_count > 0 {
            updated += 1;
        }
    }

    let response = json!({
        "success": true,
        "inserted": inserted,
        "updated": updated,
        "total": stats_list.len(),
    });

    Ok(Json(response))
}
