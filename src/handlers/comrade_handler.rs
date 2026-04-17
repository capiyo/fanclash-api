use axum::{
    extract::{Path, State},
    response::Json,
    Json as AxumJson,
};
use chrono::Utc;
use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, oid::ObjectId, DateTime as BsonDateTime},
    Collection,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    errors::{AppError, Result},
    state::AppState,
};

// ============================================================================
// MODELS
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comrade {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: String,
    pub comrade_id: String,
    pub comrade_username: String,
    pub comrade_nickname: String,
    pub comrade_club: String,
    pub comrade_country: String,
    pub status: String,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AddComradeRequest {
    pub user_id: String,
    pub comrade_id: String,
    pub username: String,
    pub comrade_username: String,
    pub comrade_nickname: String,
    pub comrade_club: String,
    pub comrade_country: String,
}

#[derive(Debug, Deserialize)]
pub struct RemoveComradeRequest {
    pub user_id: String,
    pub comrade_id: String,
}

#[derive(Debug, Serialize)]
pub struct ComradeResponse {
    pub id: String,
    pub comrade_id: String,
    pub comrade_username: String,
    pub comrade_nickname: String,
    pub comrade_club: String,
    pub comrade_country: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_mutual: bool,
}

#[derive(Debug, Serialize)]
pub struct ComradeStatsResponse {
    pub count: i64,
    pub max_comrades: i32,
    pub remaining: i32,
    pub can_add_more: bool,
}

#[derive(Debug, Serialize)]
pub struct UserForComradeResponse {
    pub user_id: String,
    pub username: String,
    pub nickname: String,
    pub club_fan: String,
    pub country_fan: String,
    pub is_already_comrade: bool,
}

// ============================================================================
// ADD COMRADE
// ============================================================================

pub async fn add_comrade(
    State(state): State<AppState>,
    AxumJson(payload): AxumJson<AddComradeRequest>,
) -> Result<Json<ComradeResponse>> {
    println!(
        "🎯 Adding comrade for user: {} -> {}",
        payload.user_id, payload.comrade_id
    );

    // Validate required fields
    if payload.user_id.is_empty() {
        return Err(AppError::missing_field("user_id"));
    }
    if payload.comrade_id.is_empty() {
        return Err(AppError::missing_field("comrade_id"));
    }
    if payload.username.is_empty() {
        return Err(AppError::missing_field("username"));
    }
    if payload.comrade_username.is_empty() {
        return Err(AppError::missing_field("comrade_username"));
    }

    let collection: Collection<Comrade> = state.db.collection("comrades");

    // Check if already exists
    let existing = collection
        .find_one(doc! {
            "user_id": &payload.user_id,
            "comrade_id": &payload.comrade_id,
        })
        .await?;

    if existing.is_some() {
        return Err(AppError::UserAlreadyExists);
    }

    // Check if mutual (comrade already added current user)
    let mutual = collection
        .find_one(doc! {
            "user_id": &payload.comrade_id,
            "comrade_id": &payload.user_id,
        })
        .await?
        .is_some();

    let now = Utc::now();

    let new_comrade = Comrade {
        id: Some(ObjectId::new()),
        user_id: payload.user_id.clone(),
        comrade_id: payload.comrade_id.clone(),
        comrade_username: payload.comrade_username.clone(),
        comrade_nickname: payload.comrade_nickname.clone(),
        comrade_club: payload.comrade_club.clone(),
        comrade_country: payload.comrade_country.clone(),
        status: if mutual {
            "active".to_string()
        } else {
            "active".to_string()
        },
        created_at: now,
    };

    collection.insert_one(&new_comrade).await?;

    // If mutual, update the existing record
    if mutual {
        collection
            .update_one(
                doc! {
                    "user_id": &payload.comrade_id,
                    "comrade_id": &payload.user_id,
                },
                doc! {
                    "$set": {
                        "status": "active",
                        "comrade_nickname": &payload.username,
                        "comrade_club": &payload.comrade_club,
                        "comrade_country": &payload.comrade_country,
                    }
                },
            )
            .await?;
    }

    let response = ComradeResponse {
        id: new_comrade.id.unwrap().to_hex(),
        comrade_id: payload.comrade_id,
        comrade_username: payload.comrade_username,
        comrade_nickname: payload.comrade_nickname,
        comrade_club: payload.comrade_club,
        comrade_country: payload.comrade_country,
        status: if mutual {
            "active".to_string()
        } else {
            "active".to_string()
        },
        created_at: now,
        is_mutual: mutual,
    };

    println!("✅ Comrade added successfully (mutual: {})", mutual);
    Ok(Json(response))
}

// ============================================================================
// GET USER'S COMRADES
// ============================================================================

pub async fn get_user_comrades(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<ComradeResponse>>> {
    println!("🔍 Getting comrades for user: {}", user_id);

    if user_id.is_empty() {
        return Err(AppError::missing_field("user_id"));
    }

    let collection: Collection<Comrade> = state.db.collection("comrades");

    let filter = doc! { "user_id": &user_id };
    let cursor = collection.find(filter).await?;
    let comrades: Vec<Comrade> = cursor.try_collect().await?;

    let mut responses = Vec::new();

    for comrade in comrades {
        // Check if mutual
        let mutual = collection
            .find_one(doc! {
                "user_id": &comrade.comrade_id,
                "comrade_id": &user_id,
            })
            .await?
            .is_some();

        responses.push(ComradeResponse {
            id: comrade.id.unwrap().to_hex(),
            comrade_id: comrade.comrade_id,
            comrade_username: comrade.comrade_username,
            comrade_nickname: comrade.comrade_nickname,
            comrade_club: comrade.comrade_club,
            comrade_country: comrade.comrade_country,
            status: comrade.status,
            created_at: comrade.created_at,
            is_mutual: mutual,
        });
    }

    println!("✅ Found {} comrades for user", responses.len());
    Ok(Json(responses))
}

// ============================================================================
// GET COMRADE STATS (COUNT)
// ============================================================================

pub async fn get_comrade_stats(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<ComradeStatsResponse>> {
    println!("📊 Getting comrade stats for user: {}", user_id);

    if user_id.is_empty() {
        return Err(AppError::missing_field("user_id"));
    }

    let collection: Collection<Comrade> = state.db.collection("comrades");

    let count = collection
        .count_documents(doc! { "user_id": &user_id })
        .await?;

    // Default max comrades is 50 (free tier)
    let max_comrades = 50;
    let remaining = (max_comrades - count as i32).max(0);
    let can_add_more = count < max_comrades as u64;

    let response = ComradeStatsResponse {
        count: count as i64,
        max_comrades,
        remaining,
        can_add_more,
    };

    println!("✅ Comrade stats: {}/{}", count, max_comrades);
    Ok(Json(response))
}

// ============================================================================
// REMOVE COMRADE
// ============================================================================

pub async fn remove_comrade(
    State(state): State<AppState>,
    AxumJson(payload): AxumJson<RemoveComradeRequest>,
) -> Result<Json<serde_json::Value>> {
    println!(
        "🗑️ Removing comrade: user {} -> comrade {}",
        payload.user_id, payload.comrade_id
    );

    if payload.user_id.is_empty() {
        return Err(AppError::missing_field("user_id"));
    }
    if payload.comrade_id.is_empty() {
        return Err(AppError::missing_field("comrade_id"));
    }

    let collection: Collection<Comrade> = state.db.collection("comrades");

    let result = collection
        .delete_one(doc! {
            "user_id": &payload.user_id,
            "comrade_id": &payload.comrade_id,
        })
        .await?;

    if result.deleted_count == 0 {
        return Err(AppError::DocumentNotFound);
    }

    println!("✅ Comrade removed successfully");
    Ok(Json(json!({
        "success": true,
        "message": "Comrade removed successfully"
    })))
}

// ============================================================================
// GET ALL USERS FOR COMRADE SELECTION (from user_profiles)
// ============================================================================

pub async fn get_available_users(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<UserForComradeResponse>>> {
    println!(
        "🔍 Getting available users for comrade selection (excluding: {})",
        user_id
    );

    if user_id.is_empty() {
        return Err(AppError::missing_field("user_id"));
    }

    let profiles_collection: Collection<serde_json::Value> = state.db.collection("user_profiles");
    let comrades_collection: Collection<Comrade> = state.db.collection("comrades");

    // Get existing comrades
    let existing_comrades: Vec<String> = comrades_collection
        .find(doc! { "user_id": &user_id })
        .await?
        .try_collect::<Vec<Comrade>>()
        .await?
        .into_iter()
        .map(|c| c.comrade_id)
        .collect();

    let mut users = Vec::new();

    // Get all users with profiles
    let mut cursor = profiles_collection.find(doc! {}).await?;
    while let Some(profile) = cursor.try_next().await? {
        let profile_user_id = profile["user_id"].as_str().unwrap_or("");

        // Skip current user and existing comrades
        if profile_user_id == user_id || existing_comrades.contains(&profile_user_id.to_string()) {
            continue;
        }

        users.push(UserForComradeResponse {
            user_id: profile_user_id.to_string(),
            username: profile["username"].as_str().unwrap_or("").to_string(),
            nickname: profile["nickname"].as_str().unwrap_or("").to_string(),
            club_fan: profile["club_fan"].as_str().unwrap_or("").to_string(),
            country_fan: profile["country_fan"].as_str().unwrap_or("").to_string(),
            is_already_comrade: false,
        });
    }

    println!(
        "✅ Found {} available users for comrade selection",
        users.len()
    );
    Ok(Json(users))
}

// ============================================================================
// UPGRADE COMRADE LIMIT (PAID TIER)
// ============================================================================

pub async fn upgrade_comrade_limit(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!("💎 Upgrading comrade limit for user: {}", user_id);

    if user_id.is_empty() {
        return Err(AppError::missing_field("user_id"));
    }

    let users_collection: Collection<serde_json::Value> = state.db.collection("users");

    let result = users_collection
        .update_one(
            doc! { "_id": &user_id },
            doc! { "$set": { "max_comrades": 200 } },
        )
        .await?;

    if result.matched_count == 0 {
        return Err(AppError::DocumentNotFound);
    }

    println!("✅ Comrade limit upgraded to 200 for user: {}", user_id);
    Ok(Json(json!({
        "success": true,
        "message": "Comrade limit upgraded to 200",
        "max_comrades": 200
    })))
}
