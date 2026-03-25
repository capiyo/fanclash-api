// handlers/sub_fixture_handler.rs
use axum::{
    extract::{Path, Query, State},
    Json,
};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, DateTime as BsonDateTime},
    Collection,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    errors::{AppError, Result},
    models::sub_fixture::{
        BulkStatsRequest, CreateSubFixtureRequest, CreateSubFixtureVoteRequest, SubFixture,
        SubFixtureQuery, SubFixtureStats, SubFixtureVote, SubFixtureVoteResponse,
        UpdateSubFixtureRequest, VoterInfo, VotersQuery,
    },
    state::AppState,
};

// ========== GET SUB-FIXTURES ==========
pub async fn get_sub_fixtures(
    State(state): State<AppState>,
    Query(query): Query<SubFixtureQuery>,
) -> Result<Json<Vec<SubFixture>>> {
    let collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
    let mut filter = doc! {};

    if let Some(parent_id) = &query.parent_fixture_id {
        filter.insert("parent_fixture_id", parent_id);
    }
    if let Some(fixture_type) = &query.fixture_type {
        filter.insert("fixture_type", fixture_type);
    }
    if let Some(is_active) = query.is_active {
        filter.insert("is_active", is_active);
    }

    let cursor = collection.find(filter).await?;
    let mut sub_fixtures: Vec<SubFixture> = cursor.try_collect().await?;
    sub_fixtures.sort_by(|a, b| a.display_order.cmp(&b.display_order));

    println!("✅ Fetched {} sub-fixtures", sub_fixtures.len());
    Ok(Json(sub_fixtures))
}

// ========== GET SUB-FIXTURE BY ID ==========
pub async fn get_sub_fixture_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SubFixture>> {
    let collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
    let filter = doc! { "sub_fixture_id": &id };

    match collection.find_one(filter).await? {
        Some(sub_fixture) => Ok(Json(sub_fixture)),
        None => Err(AppError::DocumentNotFound),
    }
}

// ========== SUBMIT SUB-FIXTURE VOTE ==========
pub async fn submit_sub_fixture_vote(
    State(state): State<AppState>,
    Json(req): Json<CreateSubFixtureVoteRequest>,
) -> Result<Json<SubFixtureVoteResponse>> {
    println!("📝 POST /api/votes/sub-fixture - Creating vote");

    // Check if sub-fixture exists and is active
    let sub_fixture_collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
    let sub_fixture_filter = doc! { "sub_fixture_id": &req.sub_fixture_id };
    let sub_fixture = sub_fixture_collection.find_one(sub_fixture_filter).await?;

    let sub_fixture = match sub_fixture {
        Some(sf) => sf,
        None => {
            return Ok(Json(SubFixtureVoteResponse {
                success: false,
                message: "Sub-fixture not found".to_string(),
                vote_id: None,
                data: None,
            }));
        }
    };

    if !sub_fixture.is_active {
        return Ok(Json(SubFixtureVoteResponse {
            success: false,
            message: "This prop bet is no longer active".to_string(),
            vote_id: None,
            data: None,
        }));
    }

    // Check if user has already voted on this sub-fixture
    let votes_collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");
    let existing_filter = doc! {
        "voter_id": &req.voter_id,
        "sub_fixture_id": &req.sub_fixture_id,
    };
    let existing_vote = votes_collection.find_one(existing_filter).await?;

    if existing_vote.is_some() {
        return Ok(Json(SubFixtureVoteResponse {
            success: false,
            message: "You have already voted on this prop bet".to_string(),
            vote_id: None,
            data: None,
        }));
    }

    // Create and insert the vote - using references to avoid ownership issues
    let new_vote = SubFixtureVote::new(
        &req.voter_id,
        &req.username,
        &req.sub_fixture_id,
        &req.parent_fixture_id,
        &req.selection,
    );

    let insert_result = votes_collection.insert_one(&new_vote).await?;
    let vote_id = insert_result
        .inserted_id
        .as_object_id()
        .map(|oid| oid.to_string());

    println!("✅ Sub-fixture vote created: {:?}", vote_id);

    Ok(Json(SubFixtureVoteResponse {
        success: true,
        message: "Prop bet vote submitted successfully".to_string(),
        vote_id,
        data: Some(json!({
            "sub_fixture_id": req.sub_fixture_id,
            "selection": req.selection,
            "voted_at": new_vote.voted_at,
        })),
    }))
}

// ========== GET SUB-FIXTURE STATS ==========
pub async fn get_sub_fixture_stats(
    State(state): State<AppState>,
    Path(sub_fixture_id): Path<String>,
) -> Result<Json<SubFixtureStats>> {
    let sub_fixture_collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
    let votes_collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");

    // Get sub-fixture details
    let sub_fixture = sub_fixture_collection
        .find_one(doc! { "sub_fixture_id": &sub_fixture_id })
        .await?;

    let sub_fixture = match sub_fixture {
        Some(sf) => sf,
        None => return Err(AppError::DocumentNotFound),
    };

    // Get vote counts
    let pipeline = vec![
        doc! { "$match": { "sub_fixture_id": &sub_fixture_id } },
        doc! { "$group": {
            "_id": "$selection",
            "count": { "$sum": 1 }
        }},
    ];

    let cursor = votes_collection.aggregate(pipeline).await?;
    let mut option_a_votes = 0i64;
    let mut option_b_votes = 0i64;
    let mut option_c_votes = 0i64;

    use futures_util::StreamExt;
    let mut cursor_stream = cursor;
    while let Some(result) = cursor_stream.next().await {
        let doc = result?;
        let selection = doc.get_str("_id").unwrap_or("");
        let count = doc.get_i64("count").unwrap_or(0);

        if selection == sub_fixture.option_a {
            option_a_votes = count;
        } else if selection == sub_fixture.option_b {
            option_b_votes = count;
        } else if let Some(ref option_c) = sub_fixture.option_c {
            if selection == option_c {
                option_c_votes = count;
            }
        }
    }

    let total_votes = option_a_votes + option_b_votes + option_c_votes;

    let option_a_percentage = if total_votes > 0 {
        (option_a_votes as f64 / total_votes as f64) * 100.0
    } else {
        0.0
    };

    let option_b_percentage = if total_votes > 0 {
        (option_b_votes as f64 / total_votes as f64) * 100.0
    } else {
        0.0
    };

    let option_c_percentage = if total_votes > 0 && sub_fixture.option_c.is_some() {
        Some((option_c_votes as f64 / total_votes as f64) * 100.0)
    } else {
        None
    };

    Ok(Json(SubFixtureStats {
        sub_fixture_id: sub_fixture.sub_fixture_id,
        question: sub_fixture.question,
        total_votes,
        option_a_votes,
        option_b_votes,
        option_c_votes: if sub_fixture.option_c.is_some() {
            Some(option_c_votes)
        } else {
            None
        },
        option_a_percentage,
        option_b_percentage,
        option_c_percentage,
        user_vote: None,
    }))
}

// ========== GET VOTERS FOR SUB-FIXTURE ==========
pub async fn get_sub_fixture_voters(
    State(state): State<AppState>,
    Path(sub_fixture_id): Path<String>,
    Query(query): Query<VotersQuery>,
) -> Result<Json<Vec<VoterInfo>>> {
    let collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");
    let mut filter = doc! { "sub_fixture_id": &sub_fixture_id };

    if let Some(selection) = &query.selection {
        filter.insert("selection", selection);
    }

    let limit = query.limit.unwrap_or(50);
    let skip = query.offset.unwrap_or(0);

    let options = mongodb::options::FindOptions::builder()
        .sort(doc! { "voted_at": -1 })
        .limit(limit)
        .skip(skip)
        .build();

    let cursor = collection.find(filter).with_options(options).await?;
    let votes: Vec<SubFixtureVote> = cursor.try_collect().await?;

    let voters: Vec<VoterInfo> = votes
        .into_iter()
        .map(|vote| VoterInfo {
            voter_id: vote.voter_id,
            username: vote.username,
            selection: vote.selection,
            voted_at: vote.voted_at,
        })
        .collect();

    println!("✅ Fetched {} voters for sub-fixture", voters.len());
    Ok(Json(voters))
}

// ========== GET USER'S SUB-FIXTURE VOTES FOR A FIXTURE ==========
pub async fn get_user_sub_fixture_votes(
    State(state): State<AppState>,
    Path((user_id, fixture_id)): Path<(String, String)>,
) -> Result<Json<Vec<SubFixtureVote>>> {
    let collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");
    let filter = doc! {
        "voter_id": &user_id,
        "parent_fixture_id": &fixture_id,
    };

    let cursor = collection.find(filter).await?;
    let votes: Vec<SubFixtureVote> = cursor.try_collect().await?;

    println!(
        "✅ Fetched {} sub-fixture votes for user {}",
        votes.len(),
        user_id
    );
    Ok(Json(votes))
}

// ========== GET ALL VOTES FOR A SUB-FIXTURE (ADMIN) ==========
pub async fn get_all_sub_fixture_votes(
    State(state): State<AppState>,
    Path(sub_fixture_id): Path<String>,
) -> Result<Json<Vec<SubFixtureVote>>> {
    let collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");
    let filter = doc! { "sub_fixture_id": &sub_fixture_id };

    let cursor = collection.find(filter).await?;
    let votes: Vec<SubFixtureVote> = cursor.try_collect().await?;

    Ok(Json(votes))
}

// ========== GET VOTE COUNTS FOR SUB-FIXTURE (CHART DATA) ==========
pub async fn get_sub_fixture_vote_counts(
    State(state): State<AppState>,
    Path(sub_fixture_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let votes_collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");

    let pipeline = vec![
        doc! { "$match": { "sub_fixture_id": &sub_fixture_id } },
        doc! { "$group": {
            "_id": "$selection",
            "count": { "$sum": 1 }
        }},
    ];

    let cursor = votes_collection.aggregate(pipeline).await?;
    let mut counts = std::collections::HashMap::new();

    use futures_util::StreamExt;
    let mut cursor_stream = cursor;
    while let Some(result) = cursor_stream.next().await {
        let doc = result?;
        let selection = doc.get_str("_id").unwrap_or("").to_string();
        let count = doc.get_i64("count").unwrap_or(0);
        counts.insert(selection, count);
    }

    Ok(Json(json!({
        "counts": counts,
        "total": counts.values().sum::<i64>()
    })))
}

// ========== CHECK IF USER HAS VOTED ==========
pub async fn check_user_sub_fixture_vote(
    State(state): State<AppState>,
    Path((sub_fixture_id, user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");
    let filter = doc! {
        "sub_fixture_id": &sub_fixture_id,
        "voter_id": &user_id,
    };

    let vote = collection.find_one(filter).await?;

    Ok(Json(json!({
        "has_voted": vote.is_some(),
        "vote": vote.map(|v| json!({
            "selection": v.selection,
            "voted_at": v.voted_at,
        })),
    })))
}

// ========== GET SUB-FIXTURES WITH USER VOTES ==========
pub async fn get_sub_fixtures_with_user_votes(
    State(state): State<AppState>,
    Path((fixture_id, user_id)): Path<(String, String)>,
) -> Result<Json<Vec<serde_json::Value>>> {
    // Get all sub-fixtures for this fixture
    let sub_fixture_collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
    let filter = doc! { "parent_fixture_id": &fixture_id, "is_active": true };
    let cursor = sub_fixture_collection.find(filter).await?;
    let sub_fixtures: Vec<SubFixture> = cursor.try_collect().await?;

    // Get user's votes for these sub-fixtures
    let votes_collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");
    let sub_fixture_ids: Vec<String> = sub_fixtures
        .iter()
        .map(|sf| sf.sub_fixture_id.clone())
        .collect();

    let user_votes_filter = doc! {
        "sub_fixture_id": { "$in": sub_fixture_ids },
        "voter_id": &user_id,
    };
    let user_votes_cursor = votes_collection.find(user_votes_filter).await?;
    let user_votes: Vec<SubFixtureVote> = user_votes_cursor.try_collect().await?;

    // Create a map of sub_fixture_id -> user_vote
    let user_vote_map: std::collections::HashMap<String, SubFixtureVote> = user_votes
        .into_iter()
        .map(|vote| (vote.sub_fixture_id.clone(), vote))
        .collect();

    // Build response
    let mut result = Vec::new();
    for sub_fixture in sub_fixtures {
        let user_vote = user_vote_map.get(&sub_fixture.sub_fixture_id);

        result.push(json!({
            "sub_fixture": sub_fixture,
            "user_vote": user_vote.map(|v| json!({
                "selection": v.selection,
                "voted_at": v.voted_at,
            })),
            "has_voted": user_vote.is_some(),
        }));
    }

    Ok(Json(result))
}

// ========== GET BULK SUB-FIXTURE STATS ==========
pub async fn get_bulk_sub_fixture_stats(
    State(state): State<AppState>,
    Json(req): Json<BulkStatsRequest>,
) -> Result<Json<serde_json::Value>> {
    let mut results = std::collections::HashMap::new();

    for sub_fixture_id in req.sub_fixture_ids {
        // Get sub-fixture details
        let sub_fixture_collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
        let sub_fixture = sub_fixture_collection
            .find_one(doc! { "sub_fixture_id": &sub_fixture_id })
            .await?;

        if let Some(sf) = sub_fixture {
            // Get vote counts
            let votes_collection: Collection<SubFixtureVote> =
                state.db.collection("sub_fixture_votes");
            let pipeline = vec![
                doc! { "$match": { "sub_fixture_id": &sub_fixture_id } },
                doc! { "$group": {
                    "_id": "$selection",
                    "count": { "$sum": 1 }
                }},
            ];

            let cursor = votes_collection.aggregate(pipeline).await?;
            let mut counts = std::collections::HashMap::new();

            use futures_util::StreamExt;
            let mut cursor_stream = cursor;
            while let Some(result) = cursor_stream.next().await {
                let doc = result?;
                let selection = doc.get_str("_id").unwrap_or("").to_string();
                let count = doc.get_i64("count").unwrap_or(0);
                counts.insert(selection, count);
            }

            let option_a_votes = *counts.get(&sf.option_a).unwrap_or(&0);
            let option_b_votes = *counts.get(&sf.option_b).unwrap_or(&0);
            let option_c_votes = sf
                .option_c
                .as_ref()
                .map(|c| *counts.get(c).unwrap_or(&0))
                .unwrap_or(0);
            let total_votes = option_a_votes + option_b_votes + option_c_votes;

            results.insert(sub_fixture_id, json!({
                "sub_fixture_id": sf.sub_fixture_id,
                "question": sf.question,
                "total_votes": total_votes,
                "option_a_votes": option_a_votes,
                "option_b_votes": option_b_votes,
                "option_c_votes": if sf.option_c.is_some() { Some(option_c_votes) } else { None },
                "option_a_percentage": if total_votes > 0 { (option_a_votes as f64 / total_votes as f64) * 100.0 } else { 0.0 },
                "option_b_percentage": if total_votes > 0 { (option_b_votes as f64 / total_votes as f64) * 100.0 } else { 0.0 },
                "option_c_percentage": if total_votes > 0 && sf.option_c.is_some() { Some((option_c_votes as f64 / total_votes as f64) * 100.0) } else { None },
            }));
        }
    }

    Ok(Json(json!({ "stats": results })))
}

// ========== GET TRENDING SUB-FIXTURES ==========
pub async fn get_trending_sub_fixtures(
    State(state): State<AppState>,
    limit: Option<Query<i64>>,
) -> Result<Json<Vec<serde_json::Value>>> {
    let limit_val = limit.unwrap_or(Query(10)).0;

    let pipeline = vec![
        doc! { "$group": {
            "_id": "$sub_fixture_id",
            "total_votes": { "$sum": 1 }
        }},
        doc! { "$sort": { "total_votes": -1 } },
        doc! { "$limit": limit_val },
    ];

    let votes_collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");
    let cursor = votes_collection.aggregate(pipeline).await?;
    let mut results = Vec::new();

    use futures_util::StreamExt;
    let mut cursor_stream = cursor;
    while let Some(result) = cursor_stream.next().await {
        let doc = result?;
        let sub_fixture_id = doc.get_str("_id").unwrap_or("").to_string();
        let total_votes = doc.get_i64("total_votes").unwrap_or(0);

        // Get sub-fixture details
        let sub_fixture_collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
        if let Some(sf) = sub_fixture_collection
            .find_one(doc! { "sub_fixture_id": &sub_fixture_id })
            .await?
        {
            results.push(json!({
                "sub_fixture": sf,
                "total_votes": total_votes,
            }));
        }
    }

    Ok(Json(results))
}

// ========== ADMIN: CREATE SUB-FIXTURE ==========
pub async fn create_sub_fixture(
    State(state): State<AppState>,
    Json(req): Json<CreateSubFixtureRequest>,
) -> Result<Json<SubFixture>> {
    let collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
    let now = BsonDateTime::from_chrono(chrono::Utc::now());

    let sub_fixture_id = format!("{}_{}", req.fixture_type, uuid::Uuid::new_v4());

    let new_sub_fixture = SubFixture {
        id: None,
        sub_fixture_id,
        parent_fixture_id: req.parent_fixture_id,
        fixture_type: req.fixture_type,
        question: req.question,
        option_a: req.option_a,
        option_b: req.option_b,
        option_c: req.option_c,
        odds_a: req.odds_a,
        odds_b: req.odds_b,
        odds_c: req.odds_c,
        is_active: true,
        display_order: req.display_order,
        icon: req.icon,
        created_at: now,
        updated_at: now,
    };

    collection.insert_one(&new_sub_fixture).await?;
    Ok(Json(new_sub_fixture))
}

// ========== ADMIN: UPDATE SUB-FIXTURE ==========
pub async fn update_sub_fixture(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSubFixtureRequest>,
) -> Result<Json<SubFixture>> {
    let collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
    let mut update_doc = doc! {};

    if let Some(question) = req.question {
        update_doc.insert("question", question);
    }
    if let Some(option_a) = req.option_a {
        update_doc.insert("option_a", option_a);
    }
    if let Some(option_b) = req.option_b {
        update_doc.insert("option_b", option_b);
    }
    if let Some(option_c) = req.option_c {
        update_doc.insert("option_c", option_c);
    }
    if let Some(odds_a) = req.odds_a {
        update_doc.insert("odds_a", odds_a);
    }
    if let Some(odds_b) = req.odds_b {
        update_doc.insert("odds_b", odds_b);
    }
    if let Some(odds_c) = req.odds_c {
        update_doc.insert("odds_c", odds_c);
    }
    if let Some(is_active) = req.is_active {
        update_doc.insert("is_active", is_active);
    }
    if let Some(display_order) = req.display_order {
        update_doc.insert("display_order", display_order);
    }

    update_doc.insert("updated_at", BsonDateTime::from_chrono(chrono::Utc::now()));

    let filter = doc! { "sub_fixture_id": &id };
    let update = doc! { "$set": update_doc };

    let result = collection.update_one(filter.clone(), update).await?;
    if result.matched_count == 0 {
        return Err(AppError::DocumentNotFound);
    }

    match collection.find_one(filter).await? {
        Some(sub_fixture) => Ok(Json(sub_fixture)),
        None => Err(AppError::DocumentNotFound),
    }
}

// ========== ADMIN: DELETE SUB-FIXTURE ==========
pub async fn delete_sub_fixture(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<SubFixture> = state.db.collection("sub_fixtures");
    let filter = doc! { "sub_fixture_id": &id };

    let result = collection.delete_one(filter).await?;
    if result.deleted_count == 0 {
        return Err(AppError::DocumentNotFound);
    }

    // Also delete all votes for this sub-fixture
    let votes_collection: Collection<SubFixtureVote> = state.db.collection("sub_fixture_votes");
    let _ = votes_collection
        .delete_many(doc! { "sub_fixture_id": &id })
        .await?;

    Ok(Json(json!({
        "success": true,
        "message": "Sub-fixture deleted successfully",
    })))
}
