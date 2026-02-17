// src/handlers/notification_handler.rs

use axum::{
    extract::{Path, State},
    response::Json,
};
use mongodb::{Collection, bson::{doc, oid::ObjectId, DateTime as BsonDateTime}};
use serde_json::json;
use futures_util::TryStreamExt;
use crate::services::fcm_service;

use crate::{
    errors::{AppError, Result},
    models::notification::{
        FCMToken, Notification, RegisterTokenRequest,
        SendNotificationRequest, MarkReadRequest, UpdatePreferencesRequest,
        NotificationPreferences,
    },
   // use crate::services::fcm_service;
   //services::fcm_service::{self, FCMService}, // Import the service
    state::AppState,
};

// Register FCM token for a user
pub async fn register_token(
    State(state): State<AppState>,
    Json(payload): Json<RegisterTokenRequest>,
) -> Result<Json<serde_json::Value>> {
    println!("📱 Registering FCM token for user: {}", payload.user_id);

    let collection: Collection<FCMToken> = state.db.collection("fcm_tokens");

    // Check if token already exists
    let filter = doc! {
        "user_id": &payload.user_id,
        "fcm_token": &payload.fcm_token,
    };

    let existing = collection.find_one(filter.clone()).await?;

    if existing.is_none() {
        // Create new token document
        let token_doc = FCMToken {
            id: None,
            user_id: payload.user_id.clone(),
            fcm_token: payload.fcm_token.clone(),
            platform: payload.platform.clone(),
            created_at: BsonDateTime::now(),
            updated_at: BsonDateTime::now(),
        };

        collection.insert_one(token_doc).await?;
        println!("✅ FCM token registered for user: {}", payload.user_id);
    } else {
        // Update existing token
        let update = doc! {
            "$set": {
                "updated_at": BsonDateTime::now(),
                "platform": &payload.platform,
            }
        };
        collection.update_one(filter, update).await?;
        println!("✅ FCM token updated for user: {}", payload.user_id);
    }

    Ok(Json(json!({
        "success": true,
        "message": "Token registered successfully",
        "user_id": payload.user_id,
    })))
}

// Send notification to a specific user
pub async fn send_notification(
    State(state): State<AppState>,
    Json(payload): Json<SendNotificationRequest>,
) -> Result<Json<serde_json::Value>> {
    println!("📤 Sending notification to user: {}", payload.user_id);

    // Initialize FCM service inside the function
    let fcm_service = fcm_service::init_fcm_service().await
        .map_err(|e| AppError::InternalServerError(format!("Failed to init FCM: {}", e)))?;

    // FIXED: Changed 'static' to 'let'
    let success = fcm_service.send_to_user(
        &state,
        &payload.user_id,
        &payload.title,
        &payload.body,
        payload.data.clone(),
        &payload.notification_type,
    ).await?;

    if success {
        println!("✅ Notification sent to user: {}", payload.user_id);
        Ok(Json(json!({
            "success": true,
            "message": "Notification sent successfully",
            "user_id": payload.user_id,
        })))
    } else {
        println!("⚠️ Failed to send notification to user: {}", payload.user_id);
        Ok(Json(json!({
            "success": false,
            "message": "User has no registered FCM tokens",
            "user_id": payload.user_id,
        })))
    }
}

// Send notification to multiple users (for batch operations)
pub async fn send_bulk_notifications(
    State(state): State<AppState>,
    Json(payload): Json<Vec<SendNotificationRequest>>,
) -> Result<Json<serde_json::Value>> {
    println!("📤 Sending {} bulk notifications", payload.len());

    let mut success_count = 0;
    let mut failed_count = 0;

    // Initialize FCM service once for all notifications
    let fcm_service = fcm_service::init_fcm_service().await
        .map_err(|e| AppError::InternalServerError(format!("Failed to init FCM: {}", e)))?;

    for notification in payload {
        // FIXED: Using fcm_service instance, not FCMService::
        let success = fcm_service.send_to_user(
            &state,
            &notification.user_id,
            &notification.title,
            &notification.body,
            notification.data.clone(),
            &notification.notification_type,
        ).await?;

        if success {
            success_count += 1;
        } else {
            failed_count += 1;
        }
    }

    println!("✅ Bulk notifications: {} sent, {} failed", success_count, failed_count);

    Ok(Json(json!({
        "success": true,
        "message": format!("Sent {} notifications, {} failed", success_count, failed_count),
        "sent": success_count,
        "failed": failed_count,
    })))
}

// Get user's notifications
pub async fn get_user_notifications(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<Notification>>> {
    println!("📬 Getting notifications for user: {}", user_id);

    let collection: Collection<Notification> = state.db.collection("notifications");
    let filter = doc! { "user_id": &user_id };

    let options = mongodb::options::FindOptions::builder()
        .sort(doc! { "created_at": -1 })
        .limit(50)
        .build();

    let cursor = collection.find(filter).await?;
    let notifications: Vec<Notification> = cursor.try_collect().await?;

    println!("✅ Found {} notifications for user: {}", notifications.len(), user_id);
    Ok(Json(notifications))
}

// Mark notifications as read
pub async fn mark_notifications_read(
    State(state): State<AppState>,
    Json(payload): Json<MarkReadRequest>,
) -> Result<Json<serde_json::Value>> {
    println!("📖 Marking notifications as read for user: {}", payload.user_id);

    let collection: Collection<Notification> = state.db.collection("notifications");

    let mut filter = doc! { "user_id": &payload.user_id };

    if let Some(ids) = &payload.notification_ids {
        let object_ids: Vec<ObjectId> = ids
            .iter()
            .filter_map(|id| ObjectId::parse_str(id).ok())
            .collect();
        filter.insert("_id", doc! { "$in": object_ids });
    }

    let update = doc! { "$set": { "is_read": true } };
    let result = collection.update_many(filter, update).await?;

    println!("✅ Marked {} notifications as read", result.modified_count);

    Ok(Json(json!({
        "success": true,
        "message": format!("Marked {} notifications as read", result.modified_count),
        "modified_count": result.modified_count,
    })))
}

// Get user's notification preferences
pub async fn get_notification_preferences(
    State(_state): State<AppState>, // Added underscore to mark as unused
    Path(user_id): Path<String>,
) -> Result<Json<NotificationPreferences>> {
    println!("⚙️ Getting notification preferences for user: {}", user_id);

    // You can store preferences in a separate collection
    // For now, return defaults
    let preferences = NotificationPreferences {
        vote_alerts: true,
        like_alerts: true,
        comment_alerts: true,
    };

    Ok(Json(preferences))
}

// Update notification preferences
pub async fn update_notification_preferences(
    State(_state): State<AppState>, // Added underscore to mark as unused
    Json(payload): Json<UpdatePreferencesRequest>,
) -> Result<Json<serde_json::Value>> {
    println!("⚙️ Updating notification preferences for user: {}", payload.user_id);

    // TODO: Save to database when you implement preferences collection

    Ok(Json(json!({
        "success": true,
        "message": "Preferences updated successfully",
        "user_id": payload.user_id,
    })))
}

// Delete expired/old tokens
pub async fn cleanup_expired_tokens(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>> {
    println!("🧹 Cleaning up old FCM tokens...");

    let collection: Collection<FCMToken> = state.db.collection("fcm_tokens");

    // Delete tokens older than 30 days (or based on your criteria)
    let thirty_days_ago = chrono::Utc::now() - chrono::Duration::days(30);
    let thirty_days_bson = mongodb::bson::DateTime::from_chrono(thirty_days_ago);

    let filter = doc! {
        "updated_at": { "$lt": thirty_days_bson }
    };

    let result = collection.delete_many(filter).await?;

    println!("✅ Deleted {} expired tokens", result.deleted_count);

    Ok(Json(json!({
        "success": true,
        "message": format!("Deleted {} expired tokens", result.deleted_count),
        "deleted_count": result.deleted_count,
    })))
}
