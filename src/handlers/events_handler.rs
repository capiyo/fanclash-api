use axum::{
    extract::{Path, State},
    response::Json,
};
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::Collection;
use serde_json::json;
use tracing;

use crate::errors::Result;
use crate::models::events::{TimelineEvent, TimelineEventRequest};
use crate::state::AppState;

// GET all events for a match
pub async fn get_match_events(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<TimelineEvent> = state.db.collection("events");
    let filter = doc! { "match_id": &match_id };
    let sort = doc! { "minute": 1 };

    let cursor = collection.find(filter).sort(sort).await?;
    let events: Vec<TimelineEvent> = cursor.try_collect().await?;

    Ok(Json(json!({
        "success": true,
        "data": events,
        "count": events.len(),
    })))
}

// GET events by type
pub async fn get_events_by_type(
    State(state): State<AppState>,
    Path((match_id, event_type)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<TimelineEvent> = state.db.collection("events");
    let filter = doc! {
        "match_id": &match_id,
        "event_type": &event_type
    };
    let sort = doc! { "minute": 1 };

    let cursor = collection.find(filter).sort(sort).await?;
    let events: Vec<TimelineEvent> = cursor.try_collect().await?;

    Ok(Json(json!({
        "success": true,
        "data": events,
        "count": events.len(),
    })))
}

// GET latest event
pub async fn get_latest_event(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
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
            "message": "No events found",
            "data": null,
        }))),
    }
}

// ADD timeline event from poller
pub async fn add_timeline_event(
    State(state): State<AppState>,
    Json(req): Json<TimelineEventRequest>,
) -> Result<Json<serde_json::Value>> {
    let event = TimelineEvent::from_request(req);

    let collection: Collection<TimelineEvent> = state.db.collection("events");
    collection.insert_one(&event).await?;

    Ok(Json(json!({
        "success": true,
        "message": "Event added successfully",
    })))
}

// DELETE all events for a match
pub async fn delete_match_events(
    State(state): State<AppState>,
    Path(match_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<TimelineEvent> = state.db.collection("events");
    let filter = doc! { "match_id": &match_id };
    let result = collection.delete_many(filter).await?;

    Ok(Json(json!({
        "success": true,
        "message": format!("Deleted {} events", result.deleted_count),
        "deleted_count": result.deleted_count,
    })))
}
