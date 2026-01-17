use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use bson::{doc, oid::ObjectId, DateTime as BsonDateTime};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{options::FindOptions, Collection};
use serde_json::json;
use validator::Validate;

use crate::{
    errors::{AppError, Result},
    models::vote::{
        parse_iso_timestamp_or_now, validate_selection, BatchFixtureCountsRequest,
        BatchStatsRequest, BulkVoteRequest, BulkVoteResponse, Comment, CommentQuery,
        CommentResponse, CommentStats, CreateComment, CreateLike, CreateVote,
        FixtureCountsResponse, FixtureStats, Like, LikeResponse, LikeStats, TotalCountsResponse,
        UserVoteStatus, Vote, VoteQuery, VoteResponse, VoteStats,
    },
    state::AppState,
};

// ========== VOTE HANDLERS ==========

pub async fn create_vote(
    State(state): State<AppState>,
    Json(payload): Json<CreateVote>,
) -> Result<Json<VoteResponse>> {
    println!("🗳️ POST /api/votes - Creating vote for user: {} ({})",
        payload.username, payload.voter_id);
    let start_time = std::time::Instant::now();

    // Validate payload
    println!("   → Validating payload...");
    payload
        .validate()
        .map_err(|e| {
            let elapsed = start_time.elapsed();
            println!("❌ Validation failed in {:?}: {}", elapsed, e.to_string());
            AppError::ValidationError(e.to_string())
        })?;
    println!("   → Payload validation successful");

    // Validate selection
    println!("   → Validating selection: {}", payload.selection);
    validate_selection(&payload.selection).map_err(|e| {
        let elapsed = start_time.elapsed();
        println!("❌ Selection validation failed in {:?}: {}", elapsed, e);
        AppError::ValidationError(e)
    })?;

    // Validate draw field is "draw"
    println!("   → Validating draw field: {}", payload.draw);
    if payload.draw != "draw" {
        let elapsed = start_time.elapsed();
        println!("❌ Invalid draw field in {:?}: '{}' (expected 'draw')", elapsed, payload.draw);
        return Err(AppError::ValidationError(
            "draw field must be 'draw'".to_string(),
        ));
    }

    // Generate fixture_id from teams and current date
    let fixture_id = format!(
        "{}_{}_{}",
        payload.home_team.replace(" ", "_").to_lowercase(),
        payload.away_team.replace(" ", "_").to_lowercase(),
        Utc::now().format("%Y%m%d").to_string()
    );
    println!("   → Generated fixture_id: {}", fixture_id);

    // Check if user already voted for this fixture
    let vote_collection: Collection<Vote> = state.db.collection("votes");

    let existing_vote_filter = doc! {
        "voterId": &payload.voter_id,
        "fixture_id": &fixture_id,
    };

    println!("   → Checking for existing vote with filter: {:?}", existing_vote_filter);
    let existing_vote = vote_collection.find_one(existing_vote_filter).await?;

    if existing_vote.is_some() {
        let elapsed = start_time.elapsed();
        println!("❌ User already voted for this fixture (check took {:?})", elapsed);
        return Ok(Json(VoteResponse {
            success: false,
            message: "User already voted for this fixture".to_string(),
            vote_id: None,
            data: None,
        }));
    }
    println!("   → No existing vote found, proceeding with creation");

    // Create vote document
    let vote = Vote {
        id: None,
        voter_id: payload.voter_id.clone(),
        username: payload.username.clone(),
        fixture_id: fixture_id.clone(),
        home_team: payload.home_team.clone(),
        away_team: payload.away_team.clone(),
        draw: payload.draw.clone(),
        selection: payload.selection.clone(),
        vote_timestamp: BsonDateTime::from_chrono(Utc::now()),
        created_at: Some(BsonDateTime::from_chrono(Utc::now())),
    };
    println!("   → Created vote document");

    // Insert into database
    println!("   → Inserting vote into database...");
    let insert_result = vote_collection.insert_one(vote).await?;
    let vote_id = insert_result.inserted_id.as_object_id().unwrap().to_hex();
    println!("   → Vote inserted with ID: {}", vote_id);

    // Fetch the inserted vote
    let filter = doc! { "_id": insert_result.inserted_id };
    println!("   → Fetching inserted vote with filter: {:?}", filter);
    let inserted_vote = vote_collection
        .find_one(filter)
        .await?
        .ok_or_else(|| {
            let elapsed = start_time.elapsed();
            println!("❌ Failed to fetch inserted vote after {:?}", elapsed);
            AppError::DocumentNotFound
        })?;

    let elapsed = start_time.elapsed();
    println!("✅ Vote created successfully in {:?}: {} by {}",
        elapsed, vote_id, payload.username);

    Ok(Json(VoteResponse {
        success: true,
        message: "Vote submitted successfully".to_string(),
        vote_id: Some(vote_id),
        data: Some(inserted_vote),
    }))
}

pub async fn get_votes(
    State(state): State<AppState>,
    Query(query): Query<VoteQuery>,
) -> Result<Json<Vec<Vote>>> {
    println!("🔍 GET /api/votes called with query: {:?}", query);
    let start_time = std::time::Instant::now();

    let collection: Collection<Vote> = state.db.collection("votes");
    let mut filter = doc! {};

    if let Some(fixture_id) = &query.fixture_id {
        filter.insert("fixture_id", fixture_id);
        println!("   → Filtering by fixture_id: {}", fixture_id);
    }

    if let Some(voter_id) = &query.voter_id {
        filter.insert("voterId", voter_id);
        println!("   → Filtering by voter_id: {}", voter_id);
    }

    let mut options = FindOptions::default();

    if let Some(limit) = query.limit {
        options.limit = Some(limit);
        println!("   → Setting limit: {}", limit);
    }

    if let Some(skip) = query.skip {
        options.skip = Some(skip);
        println!("   → Setting skip: {}", skip);
    }

    // Sort by vote_timestamp descending (newest first)
    options.sort = Some(doc! { "vote_timestamp": -1 });
    println!("   → Sorting by vote_timestamp descending");

    println!("   → Database filter: {:?}", filter);
    let cursor = collection.find(filter).await?;
    let votes: Vec<Vote> = cursor.try_collect().await?;

    let elapsed = start_time.elapsed();
    println!("✅ Found {} votes in {:?}", votes.len(), elapsed);
    Ok(Json(votes))
}

pub async fn bulk_create_votes(
    State(state): State<AppState>,
    Json(payload): Json<BulkVoteRequest>,
) -> Result<Json<BulkVoteResponse>> {
    println!("📦 POST /api/votes/bulk - Creating {} votes", payload.votes.len());
    let start_time = std::time::Instant::now();

    let collection: Collection<Vote> = state.db.collection("votes");
    let mut failed_votes = Vec::new();
    let mut votes_to_insert = Vec::new();
    let now = BsonDateTime::from_chrono(Utc::now());
    let total_votes = payload.votes.len();

    println!("   → Processing {} vote(s)", total_votes);

    // Validate and prepare votes
    for (index, vote_data) in payload.votes.into_iter().enumerate() {
        println!("   → Processing vote {} of {}", index + 1, total_votes);
        match vote_data.validate() {
            Ok(_) => {
                println!("     ✓ Validation passed");
                // Generate fixture_id
                let fixture_id = format!(
                    "{}_{}_{}",
                    vote_data.home_team.replace(" ", "_").to_lowercase(),
                    vote_data.away_team.replace(" ", "_").to_lowercase(),
                    Utc::now().format("%Y%m%d").to_string()
                );
                println!("     → Generated fixture_id: {}", fixture_id);

                let vote = Vote {
                    id: None,
                    voter_id: vote_data.voter_id.clone(),
                    username: vote_data.username.clone(),
                    fixture_id: fixture_id.clone(),
                    home_team: vote_data.home_team.clone(),
                    away_team: vote_data.away_team.clone(),
                    draw: vote_data.draw.clone(),
                    selection: vote_data.selection.clone(),
                    vote_timestamp: now,
                    created_at: Some(now),
                };

                votes_to_insert.push(vote);
            }
            Err(e) => {
                println!("     ✗ Validation failed: {}", e.to_string());
                failed_votes.push(crate::models::vote::FailedVote {
                    index,
                    error: e.to_string(),
                    vote_data,
                });
            }
        }
    }

    let valid_count = votes_to_insert.len();
    let failed_count = failed_votes.len() as u64; // Convert to u64
    println!("   → Validation complete: {} valid, {} failed", valid_count, failed_count);

    // Insert valid votes
    let inserted_count = if !votes_to_insert.is_empty() {
        println!("   → Inserting {} valid vote(s) into database...", valid_count);
        let result = collection.insert_many(votes_to_insert).await?;
        let inserted = result.inserted_ids.len() as u64;
        println!("   → Successfully inserted {} vote(s)", inserted);
        inserted
    } else {
        println!("   → No valid votes to insert");
        0
    };

    let elapsed = start_time.elapsed();
    println!("✅ Bulk vote creation completed in {:?}: {} inserted, {} failed",
        elapsed, inserted_count, failed_count);

    Ok(Json(BulkVoteResponse {
        success: true,
        inserted_count,
        failed_count, // Now this is u64
        failed_votes,
    }))
}
pub async fn get_user_votes(
    State(state): State<AppState>,
    Path(voter_id): Path<String>,
) -> Result<Json<Vec<Vote>>> {
    println!("🔍 GET /api/votes/user/{} - Getting votes for user", voter_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Vote> = state.db.collection("votes");
    let filter = doc! { "voterId": voter_id };
    println!("   → Database filter: {:?}", filter);

    let options = FindOptions::builder()
        .sort(doc! { "vote_timestamp": -1 })
        .build();
    println!("   → Sorting by vote_timestamp descending");

    let cursor = collection.find(filter).await?;
    let votes: Vec<Vote> = cursor.try_collect().await?;

    let elapsed = start_time.elapsed();
    println!("✅ Found {} votes for user in {:?}", votes.len(), elapsed);
    Ok(Json(votes))
}

pub async fn get_fixture_votes(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<VoteStats>> {
    println!("📊 GET /api/votes/fixture/{} - Getting vote stats", fixture_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Vote> = state.db.collection("votes");
    let filter = doc! { "fixture_id": &fixture_id };
    println!("   → Database filter: {:?}", filter);

    // Get all votes for this fixture
    let cursor = collection.find(filter).await?;
    let votes: Vec<Vote> = cursor.try_collect().await?;
    println!("   → Retrieved {} votes from database", votes.len());

    // Calculate statistics
    let total_votes = votes.len() as i64;
    let home_votes = votes.iter().filter(|v| v.selection == "home_team").count() as i64;
    let draw_votes = votes.iter().filter(|v| v.selection == "draw").count() as i64;
    let away_votes = votes.iter().filter(|v| v.selection == "away_team").count() as i64;

    println!("   → Vote breakdown: Home={}, Draw={}, Away={}", home_votes, draw_votes, away_votes);

    // Get home_team and away_team from first vote (if exists)
    let (home_team, away_team) = if let Some(first_vote) = votes.first() {
        (first_vote.home_team.clone(), first_vote.away_team.clone())
    } else {
        ("Unknown".to_string(), "Unknown".to_string())
    };
    println!("   → Fixture: {} vs {}", home_team, away_team);

    // Calculate percentages
    let home_percentage = if total_votes > 0 {
        (home_votes as f64 / total_votes as f64) * 100.0
    } else {
        0.0
    };

    let draw_percentage = if total_votes > 0 {
        (draw_votes as f64 / total_votes as f64) * 100.0
    } else {
        0.0
    };

    let away_percentage = if total_votes > 0 {
        (away_votes as f64 / total_votes as f64) * 100.0
    } else {
        0.0
    };

    println!("   → Percentages: Home={:.1}%, Draw={:.1}%, Away={:.1}%",
        home_percentage, draw_percentage, away_percentage);

    let stats = VoteStats {
        fixture_id: fixture_id.clone(),
        home_team,
        away_team,
        total_votes,
        home_votes,
        draw_votes,
        away_votes,
        home_percentage,
        draw_percentage,
        away_percentage,
    };

    let elapsed = start_time.elapsed();
    println!("✅ Vote stats generated in {:?}: {} votes total", elapsed, total_votes);
    Ok(Json(stats))
}

// NEW: Get total vote count for a specific fixture
pub async fn get_total_votes_for_fixture(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!("📊 GET /api/votes/fixture/{}/total - Getting total vote count", fixture_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Vote> = state.db.collection("votes");
    let filter = doc! { "fixture_id": &fixture_id };
    println!("   → Database filter: {:?}", filter);

    let total_votes = collection.count_documents(filter).await? as i64;

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "total_votes": total_votes,
        "timestamp": Utc::now().to_rfc3339(),
    });

    let elapsed = start_time.elapsed();
    println!("✅ Total votes for fixture {} in {:?}: {}", fixture_id, elapsed, total_votes);
    Ok(Json(response))
}

pub async fn get_user_vote_for_fixture(
    State(state): State<AppState>,
    Path((fixture_id, voter_id)): Path<(String, String)>,
) -> Result<Json<Option<Vote>>> {
    println!("🔍 GET /api/votes/fixture/{}/user/{} - Checking user vote",
        fixture_id, voter_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Vote> = state.db.collection("votes");
    let filter = doc! {
        "voterId": voter_id,
        "fixture_id": fixture_id,
    };
    println!("   → Database filter: {:?}", filter);

    let vote = collection.find_one(filter).await?;

    let elapsed = start_time.elapsed();
    if vote.is_some() {
        println!("✅ User has voted for this fixture (check took {:?})", elapsed);
    } else {
        println!("❌ User has not voted for this fixture (check took {:?})", elapsed);
    }

    Ok(Json(vote))
}

pub async fn delete_vote(
    State(state): State<AppState>,
    Path(vote_id): Path<String>,
) -> Result<Json<VoteResponse>> {
    println!("🗑️ DELETE /api/votes/{} - Deleting vote", vote_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Vote> = state.db.collection("votes");

    println!("   → Parsing ObjectId: {}", vote_id);
    let object_id = ObjectId::parse_str(&vote_id)
        .map_err(|_| {
            let elapsed = start_time.elapsed();
            println!("❌ Invalid vote ID format in {:?}: {}", elapsed, vote_id);
            AppError::invalid_data("Invalid vote ID format")
        })?;

    let filter = doc! { "_id": object_id };
    println!("   → Database filter: {:?}", filter);

    println!("   → Executing delete operation...");
    let delete_result = collection.delete_one(filter).await?;
    println!("   → Delete result: {:?}", delete_result);

    if delete_result.deleted_count == 0 {
        let elapsed = start_time.elapsed();
        println!("❌ Vote not found in {:?}: {}", elapsed, vote_id);
        return Ok(Json(VoteResponse {
            success: false,
            message: "Vote not found".to_string(),
            vote_id: None,
            data: None,
        }));
    }

    let elapsed = start_time.elapsed();
    println!("✅ Vote deleted successfully in {:?}", elapsed);
    Ok(Json(VoteResponse {
        success: true,
        message: "Vote deleted successfully".to_string(),
        vote_id: Some(vote_id),
        data: None,
    }))
}

// ========== LIKE HANDLERS ==========

pub async fn create_like(
    State(state): State<AppState>,
    Json(payload): Json<CreateLike>,
) -> Result<Json<LikeResponse>> {
    println!("👍 POST /api/likes - Creating like for user: {} ({}) on fixture: {}",
        payload.username, payload.voter_id, payload.fixture_id);
    let start_time = std::time::Instant::now();

    // Validate payload
    println!("   → Validating payload...");
    payload
        .validate()
        .map_err(|e| {
            let elapsed = start_time.elapsed();
            println!("❌ Validation failed in {:?}: {}", elapsed, e.to_string());
            AppError::ValidationError(e.to_string())
        })?;
    println!("   → Action requested: {}", payload.action);

    let collection: Collection<Like> = state.db.collection("likes");

    // Check if user already liked this fixture
    let existing_like_filter = doc! {
        "voterId": &payload.voter_id,
        "fixture_id": &payload.fixture_id,
    };
    println!("   → Checking existing like with filter: {:?}", existing_like_filter);

    let existing_like = collection.find_one(existing_like_filter.clone()).await?;

    let total_likes: i64;
    let message: String;

    if let Some(like) = existing_like {
        println!("   → Existing like found");
        // User already liked, check if they're unliking
        if payload.action == "unlike" {
            println!("   → Processing unlike request");
            // Delete the like
            collection.delete_one(existing_like_filter).await?;

            // Get updated like count
            let fixture_filter = doc! { "fixture_id": &payload.fixture_id };
            total_likes = collection.count_documents(fixture_filter).await? as i64;
            message = "Like removed successfully".to_string();

            println!("   → Like removed, new total: {}", total_likes);
        } else {
            let elapsed = start_time.elapsed();
            println!("❌ User already liked this fixture (operation took {:?})", elapsed);
            return Ok(Json(LikeResponse {
                success: false,
                message: "User already liked this fixture".to_string(),
                like_id: None,
                total_likes: 0,
            }));
        }
    } else {
        println!("   → No existing like found");
        // User hasn't liked yet, create new like
        if payload.action != "like" {
            let elapsed = start_time.elapsed();
            println!("❌ Cannot unlike a fixture you haven't liked (operation took {:?})", elapsed);
            return Ok(Json(LikeResponse {
                success: false,
                message: "Cannot unlike a fixture you haven't liked".to_string(),
                like_id: None,
                total_likes: 0,
            }));
        }

        println!("   → Creating new like");
        let like = Like {
            id: None,
            voter_id: payload.voter_id.clone(),
            username: payload.username.clone(),
            fixture_id: payload.fixture_id.clone(),
            action: payload.action.clone(),
            like_timestamp: BsonDateTime::from_chrono(Utc::now()),
            created_at: Some(BsonDateTime::from_chrono(Utc::now())),
        };

        let insert_result = collection.insert_one(like).await?;
        println!("   → Like inserted with ID: {:?}", insert_result.inserted_id);

        // Get updated like count
        let fixture_filter = doc! { "fixture_id": &payload.fixture_id };
        total_likes = collection.count_documents(fixture_filter).await? as i64;
        message = "Like added successfully".to_string();

        println!("   → New like count: {}", total_likes);
    }

    let elapsed = start_time.elapsed();
    println!("✅ Like operation completed in {:?}: {} total likes", elapsed, total_likes);
    Ok(Json(LikeResponse {
        success: true,
        message,
        like_id: None, // Not returning ID for simplicity
        total_likes,
    }))
}

pub async fn get_fixture_likes(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<LikeStats>> {
    println!("👍 GET /api/likes/fixture/{} - Getting like stats", fixture_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Like> = state.db.collection("likes");

    // Get total likes for fixture
    let filter = doc! { "fixture_id": &fixture_id };
    println!("   → Database filter: {:?}", filter);
    let total_likes = collection.count_documents(filter).await? as i64;

    let stats = LikeStats {
        fixture_id: fixture_id.clone(),
        total_likes,
        user_has_liked: false, // This would need user context
    };

    let elapsed = start_time.elapsed();
    println!("✅ Found {} likes for fixture in {:?}", total_likes, elapsed);
    Ok(Json(stats))
}

// NEW: Get total like count for a specific fixture
pub async fn get_total_likes_for_fixture(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!("👍 GET /api/likes/fixture/{}/total - Getting total like count", fixture_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Like> = state.db.collection("likes");
    let filter = doc! { "fixture_id": &fixture_id };
    println!("   → Database filter: {:?}", filter);

    let total_likes = collection.count_documents(filter).await? as i64;

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "total_likes": total_likes,
        "timestamp": Utc::now().to_rfc3339(),
    });

    let elapsed = start_time.elapsed();
    println!("✅ Total likes for fixture {} in {:?}: {}", fixture_id, elapsed, total_likes);
    Ok(Json(response))
}

pub async fn get_user_like_for_fixture(
    State(state): State<AppState>,
    Path((fixture_id, voter_id)): Path<(String, String)>,
) -> Result<Json<LikeStats>> {
    println!("👍 GET /api/likes/fixture/{}/user/{} - Checking user like",
        fixture_id, voter_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Like> = state.db.collection("likes");

    // Get total likes for fixture
    let fixture_filter = doc! { "fixture_id": &fixture_id };
    println!("   → Database filter for total likes: {:?}", fixture_filter);
    let total_likes = collection.count_documents(fixture_filter).await? as i64;
    println!("   → Total likes: {}", total_likes);

    // Check if user has liked
    let user_like_filter = doc! {
        "fixture_id": &fixture_id,
        "voterId": &voter_id,
    };
    println!("   → Database filter for user like: {:?}", user_like_filter);
    let user_has_liked = collection.find_one(user_like_filter).await?.is_some();
    println!("   → User has liked: {}", user_has_liked);

    let stats = LikeStats {
        fixture_id: fixture_id.clone(),
        total_likes,
        user_has_liked,
    };

    let elapsed = start_time.elapsed();
    println!("✅ User like status retrieved in {:?}: has_liked={}", elapsed, user_has_liked);
    Ok(Json(stats))
}

pub async fn delete_like(
    State(state): State<AppState>,
    Path(like_id): Path<String>,
) -> Result<Json<LikeResponse>> {
    println!("🗑️ DELETE /api/likes/{} - Deleting like", like_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Like> = state.db.collection("likes");

    println!("   → Parsing ObjectId: {}", like_id);
    let object_id = ObjectId::parse_str(&like_id)
        .map_err(|_| {
            let elapsed = start_time.elapsed();
            println!("❌ Invalid like ID format in {:?}: {}", elapsed, like_id);
            AppError::invalid_data("Invalid like ID format")
        })?;

    let filter = doc! { "_id": object_id };
    println!("   → Database filter: {:?}", filter);

    // Get the like before deleting to get fixture_id
    println!("   → Fetching like before deletion...");
    let like = collection
        .find_one(filter.clone())
        .await?
        .ok_or_else(|| {
            let elapsed = start_time.elapsed();
            println!("❌ Like not found in {:?}: {}", elapsed, like_id);
            AppError::DocumentNotFound
        })?;
    println!("   → Found like for fixture: {}", like.fixture_id);

    println!("   → Executing delete operation...");
    let delete_result = collection.delete_one(filter).await?;
    println!("   → Delete result: {:?}", delete_result);

    if delete_result.deleted_count == 0 {
        let elapsed = start_time.elapsed();
        println!("❌ Like not found (operation took {:?})", elapsed);
        return Ok(Json(LikeResponse {
            success: false,
            message: "Like not found".to_string(),
            like_id: None,
            total_likes: 0,
        }));
    }

    // Get updated like count for the fixture
    let fixture_filter = doc! { "fixture_id": &like.fixture_id };
    let total_likes = collection.count_documents(fixture_filter).await? as i64;
    println!("   → Updated like count for fixture {}: {}", like.fixture_id, total_likes);

    let elapsed = start_time.elapsed();
    println!("✅ Like deleted successfully in {:?}", elapsed);
    Ok(Json(LikeResponse {
        success: true,
        message: "Like deleted successfully".to_string(),
        like_id: Some(like_id),
        total_likes,
    }))
}

// ========== COMMENT HANDLERS ==========

pub async fn create_comment(
    State(state): State<AppState>,
    Json(payload): Json<CreateComment>,
) -> Result<Json<CommentResponse>> {
    println!("💬 POST /api/comments - Creating comment for user: {} ({}) on fixture: {}",
        payload.username, payload.voter_id, payload.fixture_id);
    let start_time = std::time::Instant::now();

    // Validate payload
    println!("   → Validating payload...");
    payload
        .validate()
        .map_err(|e| {
            let elapsed = start_time.elapsed();
            println!("❌ Validation failed in {:?}: {}", elapsed, e.to_string());
            AppError::ValidationError(e.to_string())
        })?;
    println!("   → Comment length: {} characters", payload.comment.len());

    let collection: Collection<Comment> = state.db.collection("comments");

    // Parse the timestamp from Flutter using helper function
    println!("   → Parsing timestamp: {}", payload.timestamp);
    let comment_timestamp = parse_iso_timestamp_or_now(&payload.timestamp);
    println!("   → Parsed to BSON timestamp");

    let comment = Comment {
        id: None,
        voter_id: payload.voter_id.clone(),
        username: payload.username.clone(),
        fixture_id: payload.fixture_id.clone(),
        comment: payload.comment.clone(),
        timestamp: payload.timestamp.clone(),
        comment_timestamp,
        created_at: Some(BsonDateTime::from_chrono(Utc::now())),
        likes: Some(0),
        replies: Some(Vec::new()),
    };
    println!("   → Created comment document");

    println!("   → Inserting comment into database...");
    let insert_result = collection.insert_one(comment).await?;
    let comment_id = insert_result.inserted_id.as_object_id().unwrap().to_hex();
    println!("   → Comment inserted with ID: {}", comment_id);

    // Fetch the inserted comment
    let filter = doc! { "_id": insert_result.inserted_id };
    println!("   → Fetching inserted comment with filter: {:?}", filter);
    let inserted_comment = collection
        .find_one(filter)
        .await?
        .ok_or_else(|| {
            let elapsed = start_time.elapsed();
            println!("❌ Failed to fetch inserted comment after {:?}", elapsed);
            AppError::DocumentNotFound
        })?;

    let elapsed = start_time.elapsed();
    println!("✅ Comment created successfully in {:?}: {} by {}",
        elapsed, comment_id, payload.username);

    Ok(Json(CommentResponse {
        success: true,
        message: "Comment submitted successfully".to_string(),
        comment_id: Some(comment_id),
        comment: Some(inserted_comment),
    }))
}

pub async fn get_comments(
    State(state): State<AppState>,
    Query(query): Query<CommentQuery>,
) -> Result<Json<Vec<Comment>>> {
    println!("🔍 GET /api/comments called with query: {:?}", query);
    let start_time = std::time::Instant::now();

    let collection: Collection<Comment> = state.db.collection("comments");
    let mut filter = doc! {};

    if let Some(fixture_id) = &query.fixture_id {
        filter.insert("fixture_id", fixture_id);
        println!("   → Filtering by fixture_id: {}", fixture_id);
    }

    if let Some(voter_id) = &query.voter_id {
        filter.insert("voterId", voter_id);
        println!("   → Filtering by voter_id: {}", voter_id);
    }

    let mut options = FindOptions::default();

    if let Some(limit) = query.limit {
        options.limit = Some(limit);
        println!("   → Setting limit: {}", limit);
    }

    if let Some(skip) = query.skip {
        options.skip = Some(skip);
        println!("   → Setting skip: {}", skip);
    }

    // Apply sorting
    if let Some(sort_by) = &query.sort_by {
        match sort_by.as_str() {
            "newest" => {
                options.sort = Some(doc! { "comment_timestamp": -1 });
                println!("   → Sorting by newest");
            }
            "oldest" => {
                options.sort = Some(doc! { "comment_timestamp": 1 });
                println!("   → Sorting by oldest");
            }
            "most_liked" => {
                options.sort = Some(doc! { "likes": -1 });
                println!("   → Sorting by most liked");
            }
            _ => {
                options.sort = Some(doc! { "comment_timestamp": -1 });
                println!("   → Default sorting by newest");
            }
        }
    } else {
        options.sort = Some(doc! { "comment_timestamp": -1 });
        println!("   → Default sorting by newest");
    }

    println!("   → Database filter: {:?}", filter);
    println!("   → Find options: {:?}", options);
    let cursor = collection.find(filter).await?;
    let comments: Vec<Comment> = cursor.try_collect().await?;

    let elapsed = start_time.elapsed();
    println!("✅ Found {} comments in {:?}", comments.len(), elapsed);
    Ok(Json(comments))
}

pub async fn get_fixture_comments(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<CommentStats>> {
    println!("💬 GET /api/comments/fixture/{} - Getting comments", fixture_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Comment> = state.db.collection("comments");
    let filter = doc! { "fixture_id": &fixture_id };
    println!("   → Database filter: {:?}", filter);

    let options = FindOptions::builder()
        .sort(doc! { "comment_timestamp": -1 })
        .limit(20) // Get recent 20 comments
        .build();
    println!("   → Fetching recent 20 comments sorted by newest");

    let cursor = collection.find(filter).await?;
    let all_comments: Vec<Comment> = cursor.try_collect().await?;
    println!("   → Retrieved {} total comments from database", all_comments.len());

    let total_comments = all_comments.len() as i64;

    // Get recent comments (already sorted by timestamp)
    let recent_comments: Vec<Comment> = all_comments.into_iter().take(10).collect();
    println!("   → Returning {} most recent comments", recent_comments.len());

    let stats = CommentStats {
        fixture_id: fixture_id.clone(),
        total_comments,
        recent_comments,
    };

    let elapsed = start_time.elapsed();
    println!("✅ Found {} comments for fixture in {:?}", total_comments, elapsed);
    Ok(Json(stats))
}

// NEW: Get total comment count for a specific fixture
pub async fn get_total_comments_for_fixture(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!("💬 GET /api/comments/fixture/{}/total - Getting total comment count", fixture_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Comment> = state.db.collection("comments");
    let filter = doc! { "fixture_id": &fixture_id };
    println!("   → Database filter: {:?}", filter);

    let total_comments = collection.count_documents(filter).await? as i64;

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "total_comments": total_comments,
        "timestamp": Utc::now().to_rfc3339(),
    });

    let elapsed = start_time.elapsed();
    println!("✅ Total comments for fixture {} in {:?}: {}", fixture_id, elapsed, total_comments);
    Ok(Json(response))
}

pub async fn get_user_comments(
    State(state): State<AppState>,
    Path(voter_id): Path<String>,
) -> Result<Json<Vec<Comment>>> {
    println!("🔍 GET /api/comments/user/{} - Getting user comments", voter_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Comment> = state.db.collection("comments");
    let filter = doc! { "voterId": voter_id };
    println!("   → Database filter: {:?}", filter);

    let options = FindOptions::builder()
        .sort(doc! { "comment_timestamp": -1 })
        .build();
    println!("   → Sorting by comment_timestamp descending");

    let cursor = collection.find(filter).await?;
    let comments: Vec<Comment> = cursor.try_collect().await?;

    let elapsed = start_time.elapsed();
    println!("✅ Found {} comments for user in {:?}", comments.len(), elapsed);
    Ok(Json(comments))
}

pub async fn delete_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
) -> Result<Json<CommentResponse>> {
    println!("🗑️ DELETE /api/comments/{} - Deleting comment", comment_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Comment> = state.db.collection("comments");

    println!("   → Parsing ObjectId: {}", comment_id);
    let object_id = ObjectId::parse_str(&comment_id)
        .map_err(|_| {
            let elapsed = start_time.elapsed();
            println!("❌ Invalid comment ID format in {:?}: {}", elapsed, comment_id);
            AppError::invalid_data("Invalid comment ID format")
        })?;

    let filter = doc! { "_id": object_id };
    println!("   → Database filter: {:?}", filter);

    println!("   → Executing delete operation...");
    let delete_result = collection.delete_one(filter).await?;
    println!("   → Delete result: {:?}", delete_result);

    if delete_result.deleted_count == 0 {
        let elapsed = start_time.elapsed();
        println!("❌ Comment not found in {:?}: {}", elapsed, comment_id);
        return Ok(Json(CommentResponse {
            success: false,
            message: "Comment not found".to_string(),
            comment_id: None,
            comment: None,
        }));
    }

    let elapsed = start_time.elapsed();
    println!("✅ Comment deleted successfully in {:?}", elapsed);
    Ok(Json(CommentResponse {
        success: true,
        message: "Comment deleted successfully".to_string(),
        comment_id: Some(comment_id),
        comment: None,
    }))
}

pub async fn like_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
) -> Result<Json<CommentResponse>> {
    println!("👍 POST /api/comments/{}/like - Liking comment", comment_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Comment> = state.db.collection("comments");

    println!("   → Parsing ObjectId: {}", comment_id);
    let object_id = ObjectId::parse_str(&comment_id)
        .map_err(|_| {
            let elapsed = start_time.elapsed();
            println!("❌ Invalid comment ID format in {:?}: {}", elapsed, comment_id);
            AppError::invalid_data("Invalid comment ID format")
        })?;

    let filter = doc! { "_id": object_id };
    println!("   → Database filter: {:?}", filter);

    println!("   → Incrementing likes count...");
    let update = doc! { "$inc": { "likes": 1 } };

    let update_result = collection.update_one(filter.clone(), update).await?;
    println!("   → Update result: {:?}", update_result);

    if update_result.matched_count == 0 {
        let elapsed = start_time.elapsed();
        println!("❌ Comment not found in {:?}", elapsed);
        return Ok(Json(CommentResponse {
            success: false,
            message: "Comment not found".to_string(),
            comment_id: None,
            comment: None,
        }));
    }

    // Fetch updated comment
    println!("   → Fetching updated comment...");
    let updated_comment = collection
        .find_one(filter)
        .await?
        .ok_or_else(|| {
            let elapsed = start_time.elapsed();
            println!("❌ Failed to fetch updated comment after {:?}", elapsed);
            AppError::DocumentNotFound
        })?;

    println!("   → New like count: {:?}", updated_comment.likes);

    let elapsed = start_time.elapsed();
    println!("✅ Comment liked successfully in {:?}", elapsed);
    Ok(Json(CommentResponse {
        success: true,
        message: "Comment liked successfully".to_string(),
        comment_id: Some(comment_id),
        comment: Some(updated_comment),
    }))
}

// ========== STATISTICS HANDLERS ==========

pub async fn get_vote_stats(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<VoteStats>> {
    println!("📊 GET /api/stats/votes/{} - Getting vote stats", fixture_id);
    let start_time = std::time::Instant::now();

    let result = get_fixture_votes(State(state), Path(fixture_id)).await;

    let elapsed = start_time.elapsed();
    match &result {
        Ok(_) => println!("✅ Vote stats retrieved in {:?}", elapsed),
        Err(e) => println!("❌ Failed to get vote stats in {:?}: {:?}", elapsed, e),
    }

    result
}

pub async fn get_like_stats(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<LikeStats>> {
    println!("👍 GET /api/stats/likes/{} - Getting like stats", fixture_id);
    let start_time = std::time::Instant::now();

    let result = get_fixture_likes(State(state), Path(fixture_id)).await;

    let elapsed = start_time.elapsed();
    match &result {
        Ok(_) => println!("✅ Like stats retrieved in {:?}", elapsed),
        Err(e) => println!("❌ Failed to get like stats in {:?}: {:?}", elapsed, e),
    }

    result
}

pub async fn get_comment_stats(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<CommentStats>> {
    println!("💬 GET /api/stats/comments/{} - Getting comment stats", fixture_id);
    let start_time = std::time::Instant::now();

    let result = get_fixture_comments(State(state), Path(fixture_id)).await;

    let elapsed = start_time.elapsed();
    match &result {
        Ok(_) => println!("✅ Comment stats retrieved in {:?}", elapsed),
        Err(e) => println!("❌ Failed to get comment stats in {:?}: {:?}", elapsed, e),
    }

    result
}

pub async fn get_fixture_stats(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<FixtureStats>> {
    println!("📊 GET /api/stats/fixture/{} - Getting comprehensive stats", fixture_id);
    let start_time = std::time::Instant::now();

    // Get vote stats
    println!("   → Fetching vote stats...");
    let vote_stats = get_vote_stats(State(state.clone()), Path(fixture_id.clone()))
        .await?
        .0;
    println!("   → Vote stats retrieved");

    // Get like stats (without user context)
    println!("   → Fetching like stats...");
    let like_stats = get_like_stats(State(state.clone()), Path(fixture_id.clone()))
        .await?
        .0;
    println!("   → Like stats retrieved");

    // Get comment stats
    println!("   → Fetching comment stats...");
    let comment_stats = get_comment_stats(State(state.clone()), Path(fixture_id.clone()))
        .await?
        .0;
    println!("   → Comment stats retrieved");

    let stats = FixtureStats {
        fixture_id: fixture_id.clone(),
        home_team: vote_stats.home_team.clone(),
        away_team: vote_stats.away_team.clone(),
        vote_stats,
        like_stats,
        comment_stats,
    };

    let elapsed = start_time.elapsed();
    println!("✅ Comprehensive stats generated in {:?}", elapsed);
    Ok(Json(stats))
}

// NEW: Get all counts (votes, likes, comments) for a specific fixture
pub async fn get_all_counts_for_fixture(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<FixtureCountsResponse>> {
    println!("📊 GET /api/fixtures/{}/counts - Getting all counts", fixture_id);
    let start_time = std::time::Instant::now();

    let vote_collection: Collection<Vote> = state.db.collection("votes");
    let like_collection: Collection<Like> = state.db.collection("likes");
    let comment_collection: Collection<Comment> = state.db.collection("comments");

    // Get vote counts
    println!("   → Counting votes...");
    let vote_filter = doc! { "fixture_id": &fixture_id };
    let total_votes = vote_collection.count_documents(vote_filter.clone()).await? as i64;
    println!("   → Total votes: {}", total_votes);

    let home_votes = vote_collection
        .count_documents(doc! {
            "fixture_id": &fixture_id,
            "selection": "home_team"
        })
        .await? as i64;

    let draw_votes = vote_collection
        .count_documents(doc! {
            "fixture_id": &fixture_id,
            "selection": "draw"
        })
        .await? as i64;

    let away_votes = vote_collection
        .count_documents(doc! {
            "fixture_id": &fixture_id,
            "selection": "away_team"
        })
        .await? as i64;

    println!("   → Vote breakdown: H={}, D={}, A={}", home_votes, draw_votes, away_votes);

    // Get like count
    println!("   → Counting likes...");
    let like_filter = doc! { "fixture_id": &fixture_id };
    let total_likes = like_collection.count_documents(like_filter).await? as i64;
    println!("   → Total likes: {}", total_likes);

    // Get comment count
    println!("   → Counting comments...");
    let comment_filter = doc! { "fixture_id": &fixture_id };
    let total_comments = comment_collection.count_documents(comment_filter).await? as i64;
    println!("   → Total comments: {}", total_comments);

    // Get fixture details from first vote (if exists)
    let first_vote = vote_collection.find_one(vote_filter).await?;
    let (home_team, away_team) = if let Some(vote) = first_vote {
        (vote.home_team.clone(), vote.away_team.clone())
    } else {
        ("Unknown".to_string(), "Unknown".to_string())
    };
    println!("   → Fixture: {} vs {}", home_team, away_team);

    let total_engagement = total_votes + total_likes + total_comments;
    println!("   → Total engagement: {}", total_engagement);

    let counts = crate::models::vote::FixtureCounts {
        fixture_id: fixture_id.clone(),
        home_team,
        away_team,
        total_votes,
        home_votes,
        draw_votes,
        away_votes,
        total_likes,
        total_comments,
        total_engagement,
        user_has_voted: false,
        user_has_liked: false,
        user_selection: None,
    };

    let response = FixtureCountsResponse {
        success: true,
        message: format!("Counts retrieved for fixture {}", fixture_id),
        data: counts,
    };

    let elapsed = start_time.elapsed();
    println!("✅ All counts retrieved in {:?}: {} votes, {} likes, {} comments",
        elapsed, total_votes, total_likes, total_comments);
    Ok(Json(response))
}

pub async fn get_user_stats(
    State(state): State<AppState>,
    Path(voter_id): Path<String>,
) -> Result<Json<UserVoteStatus>> {
    println!("👤 GET /api/stats/user/{} - Getting user stats", voter_id);
    let start_time = std::time::Instant::now();

    // Get user's votes
    let vote_collection: Collection<Vote> = state.db.collection("votes");
    let vote_filter = doc! { "voterId": &voter_id };
    println!("   → Counting user votes...");
    let votes_count = vote_collection.count_documents(vote_filter).await? as i64;
    println!("   → User votes: {}", votes_count);

    // Get user's likes
    let like_collection: Collection<Like> = state.db.collection("likes");
    let like_filter = doc! { "voterId": &voter_id };
    println!("   → Counting user likes...");
    let likes_count = like_collection.count_documents(like_filter).await? as i64;
    println!("   → User likes: {}", likes_count);

    // Get user's comments
    let comment_collection: Collection<Comment> = state.db.collection("comments");
    let comment_filter = doc! { "voterId": &voter_id };
    println!("   → Counting user comments...");
    let comments_count = comment_collection.count_documents(comment_filter).await? as i64;
    println!("   → User comments: {}", comments_count);

    let stats = UserVoteStatus {
        fixture_id: "all".to_string(), // For overall user stats
        has_voted: votes_count > 0,
        vote_selection: None, // Can't determine for all fixtures
        has_liked: likes_count > 0,
        user_comments_count: comments_count,
    };

    let elapsed = start_time.elapsed();
    println!(
        "✅ User stats retrieved in {:?}: {} votes, {} likes, {} comments",
        elapsed, votes_count, likes_count, comments_count
    );
    Ok(Json(stats))
}

// NEW: Get total counts across all fixtures
pub async fn get_total_counts(State(state): State<AppState>) -> Result<Json<TotalCountsResponse>> {
    println!("📈 GET /api/stats/totals - Getting total counts");
    let start_time = std::time::Instant::now();

    let vote_collection: Collection<Vote> = state.db.collection("votes");
    let like_collection: Collection<Like> = state.db.collection("likes");
    let comment_collection: Collection<Comment> = state.db.collection("comments");

    // Get total counts
    println!("   → Counting total votes...");
    let total_votes = vote_collection.estimated_document_count().await? as i64;
    println!("   → Total votes: {}", total_votes);

    println!("   → Counting total likes...");
    let total_likes = like_collection.estimated_document_count().await? as i64;
    println!("   → Total likes: {}", total_likes);

    println!("   → Counting total comments...");
    let total_comments = comment_collection.estimated_document_count().await? as i64;
    println!("   → Total comments: {}", total_comments);

    // Get unique users (distinct voterIds)
    println!("   → Counting unique users...");
    let unique_users = vote_collection.distinct("voterId", doc! {}).await?.len() as i64;
    println!("   → Unique users: {}", unique_users);

    let total_engagement = total_votes + total_likes + total_comments;
    println!("   → Total engagement: {}", total_engagement);

    let counts = crate::models::vote::TotalCounts {
        total_votes,
        total_likes,
        total_comments,
        total_engagement,
        total_users: unique_users,
        timestamp: Utc::now().to_rfc3339(),
    };

    let response = TotalCountsResponse {
        success: true,
        message: "Total counts retrieved successfully".to_string(),
        data: counts,
    };

    let elapsed = start_time.elapsed();
    println!("✅ Total counts retrieved in {:?}: {} votes, {} likes, {} comments, {} users",
        elapsed, total_votes, total_likes, total_comments, unique_users);
    Ok(Json(response))
}

// NEW: Get counts for multiple fixtures in batch
pub async fn get_batch_fixture_counts(
    State(state): State<AppState>,
    Json(payload): Json<BatchFixtureCountsRequest>,
) -> Result<Json<crate::models::vote::BatchFixtureCountsResponse>> {
    println!("📊 POST /api/fixtures/batch-counts - Getting batch counts for {} fixtures",
        payload.fixture_ids.len());
    let start_time = std::time::Instant::now();

    let vote_collection: Collection<Vote> = state.db.collection("votes");
    let like_collection: Collection<Like> = state.db.collection("likes");
    let comment_collection: Collection<Comment> = state.db.collection("comments");

    let mut fixture_counts = Vec::new();
    let total_fixtures = payload.fixture_ids.len();

    println!("   → Processing {} fixture(s)", total_fixtures);

    for (index, fixture_id) in payload.fixture_ids.into_iter().enumerate() {
        println!("   → Processing fixture {} of {}: {}", index + 1, total_fixtures, fixture_id);

        // Get vote counts
        let vote_filter = doc! { "fixture_id": &fixture_id };
        let total_votes = vote_collection.count_documents(vote_filter.clone()).await? as i64;

        // Get vote breakdown
        let home_votes = vote_collection
            .count_documents(doc! {
                "fixture_id": &fixture_id,
                "selection": "home_team"
            })
            .await? as i64;

        let draw_votes = vote_collection
            .count_documents(doc! {
                "fixture_id": &fixture_id,
                "selection": "draw"
            })
            .await? as i64;

        let away_votes = vote_collection
            .count_documents(doc! {
                "fixture_id": &fixture_id,
                "selection": "away_team"
            })
            .await? as i64;

        // Get like count
        let like_filter = doc! { "fixture_id": &fixture_id };
        let total_likes = like_collection.count_documents(like_filter).await? as i64;

        // Get comment count
        let comment_filter = doc! { "fixture_id": &fixture_id };
        let total_comments = comment_collection.count_documents(comment_filter).await? as i64;

        // Get fixture details from first vote (if exists)
        let first_vote = vote_collection.find_one(vote_filter).await?;
        let (home_team, away_team) = if let Some(vote) = first_vote {
            (vote.home_team.clone(), vote.away_team.clone())
        } else {
            ("Unknown".to_string(), "Unknown".to_string())
        };

        let total_engagement = total_votes + total_likes + total_comments;

        // Check if user has voted/liked (if user_id is provided)
        let mut user_has_voted = None;
        let mut user_has_liked = None;
        let mut user_selection = None;

        if let Some(user_id) = &payload.user_id {
            // Check if user voted
            let user_vote_filter = doc! {
                "fixture_id": &fixture_id,
                "voterId": user_id,
            };
            let user_vote = vote_collection.find_one(user_vote_filter).await?;
            user_has_voted = Some(user_vote.is_some());
            user_selection = user_vote.map(|v| v.selection);

            // Check if user liked
            let user_like_filter = doc! {
                "fixture_id": &fixture_id,
                "voterId": user_id,
            };
            let user_like = like_collection.find_one(user_like_filter).await?;
            user_has_liked = Some(user_like.is_some());
        }

        let count_item = crate::models::vote::FixtureCountItem {
            fixture_id: fixture_id.clone(),
            home_team,
            away_team,
            total_votes,
            total_likes,
            total_comments,
            total_engagement,
            user_has_voted,
            user_has_liked,
            user_selection,
        };

        fixture_counts.push(count_item);
        println!("     → Counts: {} votes, {} likes, {} comments",
            total_votes, total_likes, total_comments);
    }

    // Get the count before moving the vector
    let count = fixture_counts.len();

    let response = crate::models::vote::BatchFixtureCountsResponse {
        success: true,
        message: format!("Counts retrieved for {} fixtures", count),
        data: fixture_counts, // Now this is OK, we already got the length
        count,
    };

    let elapsed = start_time.elapsed();
    println!("✅ Batch counts retrieved in {:?} for {} fixtures", elapsed, count);
    Ok(Json(response))
}

// ========== ADMIN HANDLERS ==========

pub async fn cleanup_old_votes(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    println!("🧹 POST /api/admin/cleanup - Cleaning up old votes");
    let start_time = std::time::Instant::now();

    let collection: Collection<Vote> = state.db.collection("votes");

    // Delete votes older than 30 days
    let cutoff_date = Utc::now() - Duration::days(30);
    let cutoff_bson = BsonDateTime::from_chrono(cutoff_date);
    println!("   → Deleting votes older than: {}", cutoff_date);

    let filter = doc! {
        "vote_timestamp": {
            "$lt": cutoff_bson
        }
    };
    println!("   → Database filter: {:?}", filter);

    println!("   → Executing delete operation...");
    let delete_result = collection.delete_many(filter).await?;
    println!("   → Delete result: {:?}", delete_result);

    let response = json!({
        "success": true,
        "message": format!("Cleaned up {} old votes", delete_result.deleted_count),
        "deleted_count": delete_result.deleted_count,
        "timestamp": Utc::now().to_rfc3339(),
    });

    let elapsed = start_time.elapsed();
    println!(
        "✅ Cleanup completed in {:?}: {} votes deleted",
        elapsed, delete_result.deleted_count
    );
    Ok(Json(response))
}

pub async fn get_overview_stats(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    println!("📈 GET /api/admin/overview - Getting overview statistics");
    let start_time = std::time::Instant::now();

    // Get counts from all collections
    let vote_collection: Collection<Vote> = state.db.collection("votes");
    let like_collection: Collection<Like> = state.db.collection("likes");
    let comment_collection: Collection<Comment> = state.db.collection("comments");

    println!("   → Counting total documents...");
    let total_votes = vote_collection.estimated_document_count().await? as i64;
    let total_likes = like_collection.estimated_document_count().await? as i64;
    let total_comments = comment_collection.estimated_document_count().await? as i64;
    println!("   → Totals: {} votes, {} likes, {} comments",
        total_votes, total_likes, total_comments);

    // Get votes by selection
    println!("   → Counting votes by selection...");
    let home_votes = vote_collection
        .count_documents(doc! { "selection": "home_team" })
        .await? as i64;
    let draw_votes = vote_collection
        .count_documents(doc! { "selection": "draw" })
        .await? as i64;
    let away_votes = vote_collection
        .count_documents(doc! { "selection": "away_team" })
        .await? as i64;
    println!("   → Vote distribution: H={}, D={}, A={}", home_votes, draw_votes, away_votes);

    // Get today's votes
    let today_start = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    let today_end = Utc::now().date_naive().and_hms_opt(23, 59, 59).unwrap();

    let today_start_bson = BsonDateTime::from_chrono(today_start.and_utc());
    let today_end_bson = BsonDateTime::from_chrono(today_end.and_utc());

    println!("   → Counting today's votes...");
    let today_votes = vote_collection
        .count_documents(doc! {
            "vote_timestamp": {
                "$gte": today_start_bson,
                "$lte": today_end_bson
            }
        })
        .await? as i64;
    println!("   → Today's votes: {}", today_votes);

    let stats = json!({
        "success": true,
        "data": {
            "totals": {
                "votes": total_votes,
                "likes": total_likes,
                "comments": total_comments
            },
            "vote_distribution": {
                "home_team": home_votes,
                "draw": draw_votes,
                "away_team": away_votes
            },
            "today": {
                "votes": today_votes
            },
            "percentages": {
                "home": if total_votes > 0 { (home_votes as f64 / total_votes as f64) * 100.0 } else { 0.0 },
                "draw": if total_votes > 0 { (draw_votes as f64 / total_votes as f64) * 100.0 } else { 0.0 },
                "away": if total_votes > 0 { (away_votes as f64 / total_votes as f64) * 100.0 } else { 0.0 }
            }
        },
        "timestamp": Utc::now().to_rfc3339()
    });

    let elapsed = start_time.elapsed();
    println!("✅ Overview stats generated in {:?}", elapsed);
    Ok(Json(stats))
}

// ========== ADDITIONAL HANDLERS FOR COMMENT COUNTS AND TOTAL LIKES ==========

pub async fn get_comment_counts_for_multiple_fixtures(
    State(state): State<AppState>,
    Json(fixture_ids): Json<Vec<String>>,
) -> Result<Json<serde_json::Value>> {
    println!(
        "📊 POST /api/comments/batch-counts - Getting comment counts for {} fixtures",
        fixture_ids.len()
    );
    let start_time = std::time::Instant::now();

    let collection: Collection<Comment> = state.db.collection("comments");

    let mut result = serde_json::Map::new();
    let total_fixtures = fixture_ids.len();
    println!("   → Processing {} fixture(s)", total_fixtures);

    for (index, fixture_id) in fixture_ids.into_iter().enumerate() {
        println!("   → Processing fixture {} of {}: {}", index + 1, total_fixtures, fixture_id);
        let filter = doc! { "fixture_id": &fixture_id };
        let count = collection.count_documents(filter).await? as i64;
        result.insert(fixture_id, serde_json::Value::Number(count.into()));
        println!("     → {} comments", count);
    }

    let elapsed = start_time.elapsed();
    println!("✅ Comment counts retrieved in {:?} for {} fixtures", elapsed, total_fixtures);
    Ok(Json(serde_json::Value::Object(result)))
}

pub async fn get_total_likes_for_multiple_fixtures(
    State(state): State<AppState>,
    Json(fixture_ids): Json<Vec<String>>,
) -> Result<Json<serde_json::Value>> {
    println!("👍 POST /api/likes/batch-counts - Getting like counts for {} fixtures",
        fixture_ids.len());
    let start_time = std::time::Instant::now();

    let collection: Collection<Like> = state.db.collection("likes");

    let mut result = serde_json::Map::new();
    let total_fixtures = fixture_ids.len();
    println!("   → Processing {} fixture(s)", total_fixtures);

    for (index, fixture_id) in fixture_ids.into_iter().enumerate() {
        println!("   → Processing fixture {} of {}: {}", index + 1, total_fixtures, fixture_id);
        let filter = doc! { "fixture_id": &fixture_id };
        let count = collection.count_documents(filter).await? as i64;
        result.insert(fixture_id, serde_json::Value::Number(count.into()));
        println!("     → {} likes", count);
    }

    let elapsed = start_time.elapsed();
    println!("✅ Like counts retrieved in {:?} for {} fixtures", elapsed, total_fixtures);
    Ok(Json(serde_json::Value::Object(result)))
}

pub async fn get_combined_stats_for_multiple_fixtures(
    State(state): State<AppState>,
    Json(fixture_ids): Json<Vec<String>>,
) -> Result<Json<serde_json::Value>> {
    println!(
        "📈 POST /api/stats/batch - Getting combined stats for {} fixtures",
        fixture_ids.len()
    );
    let start_time = std::time::Instant::now();

    let mut result = Vec::new();
    let total_fixtures = fixture_ids.len();
    println!("   → Processing {} fixture(s)", total_fixtures);

    for (index, fixture_id) in fixture_ids.into_iter().enumerate() {
        println!("   → Processing fixture {} of {}: {}", index + 1, total_fixtures, fixture_id);

        // Get vote stats
        let vote_stats = get_vote_stats(State(state.clone()), Path(fixture_id.clone())).await?;
        println!("     → Vote stats retrieved");

        // Get like stats
        let like_stats = get_like_stats(State(state.clone()), Path(fixture_id.clone())).await?;
        println!("     → Like stats retrieved");

        // Get comment count
        let comment_collection: Collection<Comment> = state.db.collection("comments");
        let comment_filter = doc! { "fixture_id": &fixture_id };
        let comment_count = comment_collection.count_documents(comment_filter).await? as i64;
        println!("     → Comment count: {}", comment_count);

        let stats = json!({
            "fixture_id": fixture_id,
            "vote_stats": vote_stats.0,
            "like_stats": like_stats.0,
            "comment_count": comment_count,
        });

        result.push(stats);
    }

    let elapsed = start_time.elapsed();
    println!("✅ Combined stats retrieved in {:?} for {} fixtures", elapsed, result.len());
    Ok(Json(json!({
        "success": true,
        "data": result,
        "count": result.len(),
        "timestamp": Utc::now().to_rfc3339(),
    })))
}

// ========== NEW HANDLERS FOR REAL-TIME UPDATES ==========

pub async fn get_realtime_vote_updates(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!(
        "🔄 GET /api/realtime/{} - Getting real-time vote updates",
        fixture_id
    );
    let start_time = std::time::Instant::now();

    let vote_collection: Collection<Vote> = state.db.collection("votes");
    let like_collection: Collection<Like> = state.db.collection("likes");
    let comment_collection: Collection<Comment> = state.db.collection("comments");

    // Get vote counts by selection
    println!("   → Counting votes by selection...");
    let home_votes = vote_collection
        .count_documents(doc! {
            "fixture_id": &fixture_id,
            "selection": "home_team"
        })
        .await? as i64;

    let draw_votes = vote_collection
        .count_documents(doc! {
            "fixture_id": &fixture_id,
            "selection": "draw"
        })
        .await? as i64;

    let away_votes = vote_collection
        .count_documents(doc! {
            "fixture_id": &fixture_id,
            "selection": "away_team"
        })
        .await? as i64;

    // Get like count
    println!("   → Counting likes...");
    let like_count = like_collection
        .count_documents(doc! {
            "fixture_id": &fixture_id
        })
        .await? as i64;

    // Get comment count
    println!("   → Counting comments...");
    let comment_count = comment_collection
        .count_documents(doc! {
            "fixture_id": &fixture_id
        })
        .await? as i64;

    let total_votes = home_votes + draw_votes + away_votes;
    let total_engagement = total_votes + like_count + comment_count;

    println!("   → Totals: {} votes, {} likes, {} comments",
        total_votes, like_count, comment_count);

    let response = json!({
        "success": true,
        "data": {
            "fixture_id": fixture_id,
            "votes": {
                "home": home_votes,
                "draw": draw_votes,
                "away": away_votes,
                "total": total_votes
            },
            "likes": like_count,
            "comments": comment_count,
            "total_engagement": total_engagement,
            "last_updated": Utc::now().to_rfc3339()
        }
    });

    let elapsed = start_time.elapsed();
    println!("✅ Real-time stats retrieved in {:?}", elapsed);
    Ok(Json(response))
}

// NEW: Get vote counts by selection for a fixture
pub async fn get_vote_counts_by_selection(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!("📊 GET /api/votes/{}/breakdown - Getting vote counts by selection", fixture_id);
    let start_time = std::time::Instant::now();

    let collection: Collection<Vote> = state.db.collection("votes");

    println!("   → Counting home team votes...");
    let home_votes = collection
        .count_documents(doc! {
            "fixture_id": &fixture_id,
            "selection": "home_team"
        })
        .await? as i64;

    println!("   → Counting draw votes...");
    let draw_votes = collection
        .count_documents(doc! {
            "fixture_id": &fixture_id,
            "selection": "draw"
        })
        .await? as i64;

    println!("   → Counting away team votes...");
    let away_votes = collection
        .count_documents(doc! {
            "fixture_id": &fixture_id,
            "selection": "away_team"
        })
        .await? as i64;

    let total_votes = home_votes + draw_votes + away_votes;
    println!("   → Vote totals: H={}, D={}, A={}, Total={}",
        home_votes, draw_votes, away_votes, total_votes);

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "vote_counts": {
            "home_team": home_votes,
            "draw": draw_votes,
            "away_team": away_votes,
            "total": total_votes
        },
        "percentages": {
            "home": if total_votes > 0 { (home_votes as f64 / total_votes as f64) * 100.0 } else { 0.0 },
            "draw": if total_votes > 0 { (draw_votes as f64 / total_votes as f64) * 100.0 } else { 0.0 },
            "away": if total_votes > 0 { (away_votes as f64 / total_votes as f64) * 100.0 } else { 0.0 }
        },
        "timestamp": Utc::now().to_rfc3339()
    });

    let elapsed = start_time.elapsed();
    println!("✅ Vote counts by selection retrieved in {:?}", elapsed);
    Ok(Json(response))
}

// NEW: Get engagement summary for a fixture
pub async fn get_fixture_engagement_summary(
    State(state): State<AppState>,
    Path(fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    println!("📊 GET /api/engagement/{} - Getting engagement summary", fixture_id);
    let start_time = std::time::Instant::now();

    let vote_collection: Collection<Vote> = state.db.collection("votes");
    let like_collection: Collection<Like> = state.db.collection("likes");
    let comment_collection: Collection<Comment> = state.db.collection("comments");

    // Get counts
    println!("   → Getting engagement counts...");
    let vote_filter = doc! { "fixture_id": &fixture_id };
    let total_votes = vote_collection.count_documents(vote_filter.clone()).await? as i64;
    let total_likes = like_collection.count_documents(vote_filter.clone()).await? as i64;
    let total_comments = comment_collection.count_documents(vote_filter.clone()).await? as i64;
    let total_engagement = total_votes + total_likes + total_comments;

    println!("   → Counts: {} votes, {} likes, {} comments",
        total_votes, total_likes, total_comments);

    // Get fixture details
    let first_vote = vote_collection.find_one(vote_filter).await?;
    let (home_team, away_team) = if let Some(vote) = first_vote {
        (vote.home_team.clone(), vote.away_team.clone())
    } else {
        ("Unknown".to_string(), "Unknown".to_string())
    };
    println!("   → Fixture: {} vs {}", home_team, away_team);

    // Calculate engagement score (weighted)
    let engagement_score = (total_votes as f64 * 1.0) +
                          (total_likes as f64 * 0.5) +
                          (total_comments as f64 * 1.5);
    println!("   → Engagement score: {:.2}", engagement_score);

    let response = json!({
        "success": true,
        "fixture_id": fixture_id,
        "home_team": home_team,
        "away_team": away_team,
        "engagement_metrics": {
            "votes": total_votes,
            "likes": total_likes,
            "comments": total_comments,
            "total_engagement": total_engagement,
            "engagement_score": engagement_score
        },
        "engagement_percentages": {
            "vote_percentage": if total_engagement > 0 { (total_votes as f64 / total_engagement as f64) * 100.0 } else { 0.0 },
            "like_percentage": if total_engagement > 0 { (total_likes as f64 / total_engagement as f64) * 100.0 } else { 0.0 },
            "comment_percentage": if total_engagement > 0 { (total_comments as f64 / total_engagement as f64) * 100.0 } else { 0.0 }
        },
        "timestamp": Utc::now().to_rfc3339()
    });

    let elapsed = start_time.elapsed();
    println!("✅ Engagement summary retrieved in {:?}", elapsed);
    Ok(Json(response))
}
