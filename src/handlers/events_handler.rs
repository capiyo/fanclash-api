use axum::{
    extract::{Path, State},
    response::Json,
};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, to_bson, DateTime as BsonDateTime};
use mongodb::Collection;
use serde_json::json;
use tracing;

use crate::errors::{AppError, Result};
use crate::models::events::TimelineEvent;
use crate::state::AppState;

// ============================================================================
// GET ALL EVENTS FOR A MATCH
// ============================================================================

pub async fn get_match_events(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    tracing::info!("📋 GET /api/games/{}/events called", match_id);

    let collection: Collection<TimelineEvent> = state.db.collection("events");
    let filter = doc! { "match_id": &match_id };
    let sort = doc! { "minute": 1 };

    let cursor = collection.find(filter).sort(sort).await?;
    let events: Vec<TimelineEvent> = cursor.try_collect().await?;

    tracing::info!("✅ Found {} events for match {}", events.len(), match_id);

    Ok(Json(json!({
        "success": true,
        "data": events,
        "count": events.len(),
    })))
}

// ============================================================================
// GET EVENTS BY TYPE (goals, cards, etc.)
// ============================================================================

pub async fn get_events_by_type(
    State(state): State<AppState>,
    Path((match_id, event_type)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    tracing::info!(
        "📋 GET /api/games/{}/events/{} called",
        match_id,
        event_type
    );

    let collection: Collection<TimelineEvent> = state.db.collection("events");
    let filter = doc! {
        "match_id": &match_id,
        "event_type": &event_type
    };
    let sort = doc! { "minute": 1 };

    let cursor = collection.find(filter).sort(sort).await?;
    let events: Vec<TimelineEvent> = cursor.try_collect().await?;

    tracing::info!("✅ Found {} {} events", events.len(), event_type);

    Ok(Json(json!({
        "success": true,
        "data": events,
        "count": events.len(),
    })))
}

// ============================================================================
// GET LATEST EVENT FOR A MATCH
// ============================================================================

pub async fn get_latest_event(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    tracing::info!("📋 GET /api/games/{}/events/latest called", match_id);

    let collection: Collection<TimelineEvent> = state.db.collection("events");
    let filter = doc! { "match_id": &match_id };
    let sort = doc! { "minute": -1 };

    let event = collection.find_one(filter).sort(sort).await?;

    match event {
        Some(e) => Ok(Json(json!({
            "success": true,
            "data": e,
        }))),
        None => Ok(Json(json!({
            "success": false,
            "message": "No events found for this match",
            "data": null,
        }))),
    }
}
pub async fn add_timeline_event(
    State(state): State<AppState>,
    Json(event): Json<TimelineEvent>,
) -> Result<Json<serde_json::Value>> {
    tracing::info!(
        "➕ Adding timeline event: {} for match {} at minute {}",
        event.event_type,
        event.match_id,
        event.minute
    );

    let collection: Collection<TimelineEvent> = state.db.collection("events");

    // Insert directly - no manual BSON conversion needed
    collection.insert_one(&event).await?;

    tracing::info!("✅ Timeline event stored successfully");

    Ok(Json(json!({
        "success": true,
        "message": "Event added successfully",
        "data": {
            "id": event.id,
            "event_type": event.event_type,
            "minute": event.minute,
        }
    })))
}
// ============================================================================
// ADD TIMELINE EVENT (Called by Poller)
// ============================================================================

// ============================================================================
// DELETE ALL EVENTS FOR A MATCH (Admin/Testing)
// ============================================================================

pub async fn delete_match_events(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    tracing::info!("🗑️ DELETE /api/games/{}/events called", match_id);

    let collection: Collection<TimelineEvent> = state.db.collection("events");
    let filter = doc! { "match_id": &match_id };

    let result = collection.delete_many(filter).await?;

    tracing::info!(
        "✅ Deleted {} events for match {}",
        result.deleted_count,
        match_id
    );

    Ok(Json(json!({
        "success": true,
        "message": format!("Deleted {} events", result.deleted_count),
        "deleted_count": result.deleted_count,
    })))
}
