use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use mongodb::bson::Bson;

use axum_extra::extract::Multipart;
use chrono::Utc;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId, Document};
use mongodb::{options::FindOptions, Collection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use uuid::Uuid;

use crate::errors::{AppError, Result};
use crate::models::posta::{Post, PostResponse, Comment, CommentResponse, LikeRequest, CreateCommentRequest, UpdateCommentRequest};
use crate::services::cloudinary::CloudinaryService;
use crate::state::AppState;

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
const ALLOWED_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "gif"];
const DEFAULT_PAGE_SIZE: i64 = 20;
const MAX_PAGE_SIZE: i64 = 100;

#[derive(Debug, Deserialize, Serialize)]
pub struct PaginationParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateCaptionRequest {
    pub caption: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdatePostRequest {
    pub caption: Option<String>,
    pub image: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub user_id: Option<String>,
    pub start_date: Option<chrono::DateTime<Utc>>,
    pub end_date: Option<chrono::DateTime<Utc>>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

fn bson_to_json_value(bson: &Bson) -> JsonValue {
    match bson {
        Bson::ObjectId(oid) => json!(oid.to_hex()),
        Bson::DateTime(dt) => json!(dt.to_chrono().to_rfc3339()),
        Bson::String(s) => json!(s),
        Bson::Int32(i) => json!(i),
        Bson::Int64(i) => json!(i),
        Bson::Double(d) => json!(d),
        Bson::Boolean(b) => json!(b),
        Bson::Null => JsonValue::Null,
        Bson::Array(arr) => JsonValue::Array(arr.iter().map(bson_to_json_value).collect()),
        Bson::Document(doc) => {
            let mut map = serde_json::Map::new();
            for (k, v) in doc {
                map.insert(k.clone(), bson_to_json_value(v));
            }
            JsonValue::Object(map)
        }
        _ => json!(bson.to_string()),
    }
}

fn document_to_json(doc: Document) -> JsonValue {
    let mut map = serde_json::Map::new();
    for (key, value) in doc {
        map.insert(key, bson_to_json_value(&value));
    }
    JsonValue::Object(map)
}

// ========== LOGGING MACROS ==========
// Note: Using statements instead of expressions to avoid semicolon issues
macro_rules! log_info {
    ($($arg:tt)*) => {
        println!("[INFO] [{}] {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"), format!($($arg)*))
    }
}

macro_rules! log_debug {
    ($($arg:tt)*) => {
        println!("[DEBUG] [{}] {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"), format!($($arg)*))
    }
}

macro_rules! log_warn {
    ($($arg:tt)*) => {
        println!("[WARN] [{}] {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"), format!($($arg)*))
    }
}

macro_rules! log_error {
    ($($arg:tt)*) => {
        eprintln!("[ERROR] [{}] {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"), format!($($arg)*))
    }
}

macro_rules! log_trace {
    ($($arg:tt)*) => {
        println!("[TRACE] [{}] {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"), format!($($arg)*))
    }
}

// ========== ORIGINAL POST HANDLERS ==========

pub async fn get_posts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting get_posts handler", request_id);
    log_debug!("[{}] Pagination params: page={:?}, limit={:?}, user_id={:?}",
        request_id, params.page, params.limit, params.user_id);

    let collection: Collection<Post> = state.db.collection("posts");

    let mut filter = doc! {};
    if let Some(user_id) = &params.user_id {
        filter.insert("user_id", user_id);
        log_debug!("[{}] Filtering by user_id: {}", request_id, user_id);
    }

    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE)
        .max(1);
    let skip = (page - 1) * limit;

    log_debug!("[{}] Calculated pagination: page={}, limit={}, skip={}",
        request_id, page, limit, skip);

    let options = FindOptions::builder()
        .sort(doc! { "created_at": -1 })
        .skip(skip as u64)
        .limit(limit)
        .build();

    log_trace!("[{}] Starting database query: count_documents", request_id);
    let total_count = collection.count_documents(filter.clone()).await? as i64;
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;
    log_trace!("[{}] Database query completed: total_count={}", request_id, total_count);

    log_trace!("[{}] Starting database query: find with options", request_id);
    let cursor = collection.find(filter).await?;
    let posts: Vec<Post> = cursor.try_collect().await?;
    log_trace!("[{}] Database query completed: found {} posts", request_id, posts.len());

    let post_responses: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();

    let duration = start_time.elapsed();
    log_info!("[{}] get_posts completed in {:?}. Found {} posts",
        request_id, duration, post_responses.len());

    Ok(Json(json!({
        "success": true,
        "posts": post_responses,
        "pagination": {
            "page": page,
            "limit": limit,
            "total_count": total_count,
            "total_pages": total_pages,
            "has_next": page < total_pages,
            "has_previous": page > 1
        }
    })))
}

pub async fn create_post(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting create_post handler", request_id);

    let mut caption = String::new();
    let mut user_id = String::new();
    let mut user_name = String::new();
    let mut image_data = None;
    let mut file_extension = None;

    log_debug!("[{}] Starting multipart field processing", request_id);
    let mut field_count = 0;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| {
            log_error!("[{}] Failed to process multipart field: {}", request_id, e);
            AppError::Multipart(format!("Failed to process multipart field: {}", e))
        })?
    {
        field_count += 1;
        let field_name = field.name().unwrap_or("").to_string();
        log_trace!("[{}] Processing field {}: {}", request_id, field_count, field_name);

        match field_name.as_str() {
            "caption" => {
                caption = field
                    .text()
                    .await
                    .map_err(|e| {
                        log_error!("[{}] Failed to read caption: {}", request_id, e);
                        AppError::Multipart(format!("Failed to read caption: {}", e))
                    })?;
                log_debug!("[{}] Caption extracted: {} characters", request_id, caption.len());
            }
            "userId" => {
                user_id = field
                    .text()
                    .await
                    .map_err(|e| {
                        log_error!("[{}] Failed to read user_id: {}", request_id, e);
                        AppError::Multipart(format!("Failed to read user_id: {}", e))
                    })?;
                log_debug!("[{}] User ID extracted: {}", request_id, user_id);
            }
            "userName" => {
                user_name = field
                    .text()
                    .await
                    .map_err(|e| {
                        log_error!("[{}] Failed to read user_name: {}", request_id, e);
                        AppError::Multipart(format!("Failed to read user_name: {}", e))
                    })?;
                log_debug!("[{}] User Name extracted: {}", request_id, user_name);
            }
            "image" => {
                let file_name = field.file_name().unwrap_or("image").to_string();
                log_debug!("[{}] Processing image file: {}", request_id, file_name);

                let data = field.bytes().await.map_err(|e| {
                    log_error!("[{}] Failed to read image data: {}", request_id, e);
                    AppError::Multipart(format!("Failed to read image data: {}", e))
                })?;

                if data.len() as u64 > MAX_FILE_SIZE {
                    log_warn!("[{}] Image too large: {} bytes (max: {} bytes)",
                        request_id, data.len(), MAX_FILE_SIZE);
                    return Err(AppError::ImageTooLarge);
                }

                let ext = std::path::Path::new(&file_name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
                    log_warn!("[{}] Invalid image format: {}", request_id, ext);
                    return Err(AppError::InvalidImageFormat);
                }

                file_extension = Some(ext.clone());
                image_data = Some(data.to_vec());
                log_debug!("[{}] Image processed: {} bytes, extension: {}",
                    request_id, data.len(), ext);
            }
            _ => {
                log_trace!("[{}] Skipping unknown field: {}", request_id, field_name);
                continue;
            }
        }
    }

    log_debug!("[{}] Multipart processing complete. Fields processed: {}",
        request_id, field_count);

    if user_id.trim().is_empty() {
        log_error!("[{}] Empty user_id provided", request_id);
        return Err(AppError::InvalidUserData);
    }

    if user_name.trim().is_empty() {
        log_error!("[{}] Empty user_name provided", request_id);
        return Err(AppError::InvalidUserData);
    }

    let image_data = image_data.ok_or_else(|| {
        log_error!("[{}] No image provided in request", request_id);
        AppError::NoImageProvided
    })?;

    let file_extension = file_extension.ok_or_else(|| {
        log_error!("[{}] No file extension found", request_id);
        AppError::InvalidImageFormat
    })?;

    log_debug!("[{}] Validation passed. Image size: {} bytes",
        request_id, image_data.len());

    let cloudinary_service = &state.cloudinary;
    let public_id = format!("post_{}_{}", user_id, Uuid::new_v4());
    let upload_path = format!("fanclash/posts/{}", user_id);

    log_info!("[{}] Starting Cloudinary upload. Public ID: {}, Path: {}",
        request_id, public_id, upload_path);

    let upload_start = std::time::Instant::now();
    let (image_url, cloudinary_public_id) = match cloudinary_service
        .upload_image_with_preset(
            &image_data,
            &upload_path,
            Some(&public_id),
        )
        .await
    {
        Ok(result) => {
            log_info!("[{}] Cloudinary upload with preset successful", request_id);
            result
        }
        Err(preset_error) => {
            log_warn!("[{}] Cloudinary upload with preset failed: {}. Trying signed upload...",
                request_id, preset_error);

            cloudinary_service
                .upload_image_signed(
                    &image_data,
                    &upload_path,
                    Some(&public_id),
                )
                .await
                .map_err(|e| {
                    log_error!("[{}] Both Cloudinary upload methods failed: {}", request_id, e);
                    AppError::invalid_data(format!("Both upload methods failed. Last error: {}", e))
                })?
        }
    };

    let upload_duration = upload_start.elapsed();
    log_info!("[{}] Cloudinary upload completed in {:?}. URL: {}",
        request_id, upload_duration, image_url);

    let collection: Collection<Post> = state.db.collection("posts");
    let now = Utc::now();
    let post = Post {
        _id: Some(ObjectId::new()),
        user_id: user_id.clone(),
        user_name: user_name.clone(),
        caption: caption.clone(),
        image_url: image_url.clone(),
        cloudinary_public_id: cloudinary_public_id.clone(),
        image_format: file_extension.clone(),
        likes_count: 0,
        comments_count: 0,
        shares_count: 0,
        liked_by: Vec::new(),
        is_saved: false,
        created_at: now,
        updated_at: now,
    };

    log_debug!("[{}] Creating post document with ID: {:?}", request_id, post._id);

    let db_start = std::time::Instant::now();
    collection.insert_one(&post).await?;
    let db_duration = db_start.elapsed();
    log_info!("[{}] Database insert completed in {:?}", request_id, db_duration);

    let post_response = PostResponse::from(post);

    let total_duration = start_time.elapsed();
    log_info!("[{}] create_post completed in {:?}. Post created successfully",
        request_id, total_duration);

    Ok(Json(json!({
        "success": true,
        "message": "Post created successfully",
        "post": post_response
    })))
}

pub async fn get_post_by_id(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting get_post_by_id handler. Post ID: {}", request_id, post_id);

    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse ObjectId from '{}': {}", request_id, post_id, e);
            return Err(AppError::PostNotFound);
        }
    };

    let filter = doc! { "_id": object_id };
    log_trace!("[{}] Querying database with filter: {:?}", request_id, filter);

    let query_start = std::time::Instant::now();
    let post = collection.find_one(filter).await?;
    let query_duration = query_start.elapsed();

    log_trace!("[{}] Database query completed in {:?}", request_id, query_duration);

    match post {
        Some(post) => {
            log_info!("[{}] Post found: {}", request_id, post_id);
            let post_response = PostResponse::from(post);

            let total_duration = start_time.elapsed();
            log_info!("[{}] get_post_by_id completed in {:?}", request_id, total_duration);

            Ok(Json(json!({
                "success": true,
                "post": post_response
            })))
        }
        None => {
            log_warn!("[{}] Post not found: {}", request_id, post_id);
            Err(AppError::PostNotFound)
        }
    }
}

pub async fn get_posts_by_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting get_posts_by_user handler. User ID: {}", request_id, user_id);
    log_debug!("[{}] Pagination params: page={:?}, limit={:?}",
        request_id, params.page, params.limit);

    let collection: Collection<Post> = state.db.collection("posts");

    let filter = doc! { "user_id": &user_id };
    log_debug!("[{}] Using filter: {:?}", request_id, filter);

    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE)
        .max(1);
    let skip = (page - 1) * limit;

    log_debug!("[{}] Pagination: page={}, limit={}, skip={}",
        request_id, page, limit, skip);

    let options = FindOptions::builder()
        .sort(doc! { "created_at": -1 })
        .skip(skip as u64)
        .limit(limit)
        .build();

    log_trace!("[{}] Starting count query", request_id);
    let count_start = std::time::Instant::now();
    let total_count = collection.count_documents(filter.clone()).await? as i64;
    let count_duration = count_start.elapsed();
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;
    log_trace!("[{}] Count query completed in {:?}. Total count: {}",
        request_id, count_duration, total_count);

    log_trace!("[{}] Starting posts query", request_id);
    let posts_start = std::time::Instant::now();
    let cursor = collection.find(filter).await?;
    let posts: Vec<Post> = cursor.try_collect().await?;
    let posts_duration = posts_start.elapsed();
    log_trace!("[{}] Posts query completed in {:?}. Found {} posts",
        request_id, posts_duration, posts.len());

    let post_responses: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();

    let total_duration = start_time.elapsed();
    log_info!("[{}] get_posts_by_user completed in {:?}. Found {} posts for user {}",
        request_id, total_duration, post_responses.len(), user_id);

    Ok(Json(json!({
        "success": true,
        "user_id": user_id,
        "posts": post_responses,
        "pagination": {
            "page": page,
            "limit": limit,
            "total_count": total_count,
            "total_pages": total_pages,
            "has_next": page < total_pages,
            "has_previous": page > 1
        }
    })))
}

pub async fn update_post_caption(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    axum::extract::Json(payload): axum::extract::Json<UpdateCaptionRequest>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting update_post_caption handler. Post ID: {}", request_id, post_id);
    log_debug!("[{}] New caption: {} characters", request_id, payload.caption.len());

    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse ObjectId: {}", request_id, e);
            return Err(AppError::PostNotFound);
        }
    };

    let filter = doc! { "_id": object_id };
    let update = doc! {
        "$set": {
            "caption": payload.caption.clone(),
            "updated_at": Utc::now()
        }
    };

    log_debug!("[{}] Update operation: filter={:?}, update={:?}", request_id, filter, update);

    log_trace!("[{}] Executing database update", request_id);
    let update_start = std::time::Instant::now();
    let result = collection.update_one(filter, update).await?;
    let update_duration = update_start.elapsed();
    log_trace!("[{}] Update completed in {:?}. Matched: {}, Modified: {}",
        request_id, update_duration, result.matched_count, result.modified_count);

    if result.matched_count == 0 {
        log_warn!("[{}] Post not found for update: {}", request_id, post_id);
        return Err(AppError::PostNotFound);
    }

    let total_duration = start_time.elapsed();
    log_info!("[{}] update_post_caption completed in {:?}. Caption updated successfully",
        request_id, total_duration);

    Ok(Json(json!({
        "success": true,
        "message": "Post caption updated successfully",
        "caption": payload.caption
    })))
}

pub async fn delete_post(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting delete_post handler. Post ID: {}", request_id, post_id);

    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse ObjectId: {}", request_id, e);
            return Err(AppError::PostNotFound);
        }
    };

    let filter = doc! { "_id": object_id };

    log_trace!("[{}] Looking up post before deletion", request_id);
    let lookup_start = std::time::Instant::now();
    let post = collection.find_one(filter.clone()).await?;
    let lookup_duration = lookup_start.elapsed();
    log_trace!("[{}] Post lookup completed in {:?}", request_id, lookup_duration);

    match post {
        Some(post) => {
            log_info!("[{}] Post found. Starting Cloudinary deletion", request_id);

            let cloudinary_start = std::time::Instant::now();
            let cloudinary_service = &state.cloudinary;
            let delete_result = cloudinary_service.delete_image(&post.cloudinary_public_id).await;
            let cloudinary_duration = cloudinary_start.elapsed();

            match delete_result {
                Ok(_) => log_info!("[{}] Cloudinary image deleted successfully in {:?}",
                    request_id, cloudinary_duration),
                Err(e) => log_warn!("[{}] Cloudinary deletion failed: {} (took {:?})",
                    request_id, e, cloudinary_duration),
            }

            log_trace!("[{}] Deleting from database", request_id);
            let db_delete_start = std::time::Instant::now();
            let delete_result = collection.delete_one(filter).await?;
            let db_delete_duration = db_delete_start.elapsed();
            log_trace!("[{}] Database deletion completed in {:?}", request_id, db_delete_duration);

            if delete_result.deleted_count == 0 {
                log_error!("[{}] Database reported 0 deletions despite finding post", request_id);
                return Err(AppError::PostNotFound);
            }

            let total_duration = start_time.elapsed();
            log_info!("[{}] delete_post completed in {:?}. Post {} deleted successfully",
                request_id, total_duration, post_id);

            Ok(Json(json!({
                "success": true,
                "message": "Post deleted successfully",
                "post_id": post_id
            })))
        }
        None => {
            log_warn!("[{}] Post not found for deletion: {}", request_id, post_id);
            Err(AppError::PostNotFound)
        }
    }
}

pub async fn delete_posts_by_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting delete_posts_by_user handler. User ID: {}", request_id, user_id);

    let collection: Collection<Post> = state.db.collection("posts");

    let filter = doc! { "user_id": &user_id };
    log_debug!("[{}] Using filter: {:?}", request_id, filter);

    log_trace!("[{}] Finding all posts for user", request_id);
    let find_start = std::time::Instant::now();
    let cursor = collection.find(filter.clone()).await?;
    let posts: Vec<Post> = cursor.try_collect().await?;
    let find_duration = find_start.elapsed();

    log_info!("[{}] Found {} posts for user {} in {:?}",
        request_id, posts.len(), user_id, find_duration);

    if posts.is_empty() {
        log_info!("[{}] No posts found for user. Exiting early.", request_id);
        return Ok(Json(json!({
            "success": true,
            "message": "No posts found for user",
            "deleted_count": 0
        })));
    }

    let cloudinary_service = &state.cloudinary;
    let mut deleted_from_cloudinary = 0;

    log_info!("[{}] Starting Cloudinary deletions ({} images)", request_id, posts.len());
    let cloudinary_start = std::time::Instant::now();

    for (i, post) in posts.iter().enumerate() {
        log_trace!("[{}] Deleting Cloudinary image {}/{}: {}",
            request_id, i + 1, posts.len(), post.cloudinary_public_id);

        let _ = cloudinary_service.delete_image(&post.cloudinary_public_id).await;
        deleted_from_cloudinary += 1;
    }

    let cloudinary_duration = cloudinary_start.elapsed();
    log_info!("[{}] Cloudinary deletions completed in {:?}. Successfully deleted: {}/{}",
        request_id, cloudinary_duration, deleted_from_cloudinary, posts.len());

    log_trace!("[{}] Deleting from database", request_id);
    let db_delete_start = std::time::Instant::now();
    let delete_result = collection.delete_many(filter).await?;
    let db_delete_duration = db_delete_start.elapsed();
    log_trace!("[{}] Database deletion completed in {:?}. Deleted: {}",
        request_id, db_delete_duration, delete_result.deleted_count);

    let total_duration = start_time.elapsed();
    log_info!("[{}] delete_posts_by_user completed in {:?}. Deleted {} posts for user {}",
        request_id, total_duration, delete_result.deleted_count, user_id);

    Ok(Json(json!({
        "success": true,
        "message": "All user posts deleted successfully",
        "deleted_from_db": delete_result.deleted_count,
        "deleted_from_cloudinary": deleted_from_cloudinary,
        "user_id": user_id
    })))
}

pub async fn get_post_thumbnail(
    State(state): State<AppState>,
    Path((post_id, width, height)): Path<(String, u32, u32)>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting get_post_thumbnail handler. Post ID: {}, Dimensions: {}x{}",
        request_id, post_id, width, height);

    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse ObjectId: {}", request_id, e);
            return Err(AppError::PostNotFound);
        }
    };

    let filter = doc! { "_id": object_id };
    log_trace!("[{}] Querying database", request_id);

    let query_start = std::time::Instant::now();
    let post = collection.find_one(filter).await?;
    let query_duration = query_start.elapsed();
    log_trace!("[{}] Database query completed in {:?}", request_id, query_duration);

    match post {
        Some(post) => {
            log_debug!("[{}] Post found. Generating thumbnail URL", request_id);

            let cloudinary_service = &state.cloudinary;
            let thumbnail_start = std::time::Instant::now();
            let thumbnail_url = cloudinary_service.generate_thumbnail_url(
                &post.cloudinary_public_id,
                width,
                height,
            );
            let thumbnail_duration = thumbnail_start.elapsed();

            log_debug!("[{}] Thumbnail URL generated in {:?}", request_id, thumbnail_duration);

            let total_duration = start_time.elapsed();
            log_info!("[{}] get_post_thumbnail completed in {:?}", request_id, total_duration);

            Ok(Json(json!({
                "success": true,
                "thumbnail_url": thumbnail_url,
                "post_id": post_id,
                "original_url": post.image_url,
                "dimensions": {
                    "width": width,
                    "height": height
                }
            })))
        }
        None => {
            log_warn!("[{}] Post not found for thumbnail generation: {}", request_id, post_id);
            Err(AppError::PostNotFound)
        }
    }
}

pub async fn get_post_with_transform(
    State(state): State<AppState>,
    Path((post_id, transformations)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting get_post_with_transform handler. Post ID: {}, Transformations: {}",
        request_id, post_id, transformations);

    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse ObjectId: {}", request_id, e);
            return Err(AppError::PostNotFound);
        }
    };

    let filter = doc! { "_id": object_id };
    log_trace!("[{}] Querying database", request_id);

    let query_start = std::time::Instant::now();
    let post = collection.find_one(filter).await?;
    let query_duration = query_start.elapsed();
    log_trace!("[{}] Database query completed in {:?}", request_id, query_duration);

    match post {
        Some(post) => {
            log_debug!("[{}] Post found. Generating transformed URL", request_id);

            let cloudinary_service = &state.cloudinary;
            let transform_start = std::time::Instant::now();
            let transformed_url = cloudinary_service
                .generate_transformed_url(&post.cloudinary_public_id, &transformations);
            let transform_duration = transform_start.elapsed();

            log_debug!("[{}] Transformed URL generated in {:?}", request_id, transform_duration);

            let total_duration = start_time.elapsed();
            log_info!("[{}] get_post_with_transform completed in {:?}", request_id, total_duration);

            Ok(Json(json!({
                "success": true,
                "transformed_url": transformed_url,
                "post_id": post_id,
                "transformations": transformations,
                "original_url": post.image_url
            })))
        }
        None => {
            log_warn!("[{}] Post not found for transformation: {}", request_id, post_id);
            Err(AppError::PostNotFound)
        }
    }
}

pub async fn search_posts(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting search_posts handler", request_id);
    log_debug!("[{}] Search params: query={:?}, user_id={:?}, start_date={:?}, end_date={:?}, page={:?}, limit={:?}",
        request_id, params.q, params.user_id, params.start_date, params.end_date, params.page, params.limit);

    let collection: Collection<Post> = state.db.collection("posts");

    let search_query = params.q.clone();
    let search_user_id = params.user_id.clone();
    let search_start_date = params.start_date;
    let search_end_date = params.end_date;

    let mut filter = doc! {};
    log_debug!("[{}] Building search filter", request_id);

    if let Some(query) = &params.q {
        filter.insert(
            "caption",
            doc! {
                "$regex": query,
                "$options": "i"
            },
        );
        log_debug!("[{}] Added caption regex filter: {}", request_id, query);
    }

    if let Some(user_id) = &params.user_id {
        filter.insert("user_id", user_id);
        log_debug!("[{}] Added user_id filter: {}", request_id, user_id);
    }

    if let Some(start_date) = &params.start_date {
        filter.insert("created_at", doc! { "$gte": start_date });
        log_debug!("[{}] Added start_date filter: {}", request_id, start_date);
    }

    if let Some(end_date) = &params.end_date {
        filter.insert("created_at", doc! { "$lte": end_date });
        log_debug!("[{}] Added end_date filter: {}", request_id, end_date);
    }

    log_debug!("[{}] Final filter: {:?}", request_id, filter);

    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE)
        .max(1);
    let skip = (page - 1) * limit;

    log_debug!("[{}] Pagination: page={}, limit={}, skip={}",
        request_id, page, limit, skip);

    let options = FindOptions::builder()
        .sort(doc! { "created_at": -1 })
        .skip(skip as u64)
        .limit(limit)
        .build();

    log_trace!("[{}] Starting count query", request_id);
    let count_start = std::time::Instant::now();
    let total_count = collection.count_documents(filter.clone()).await? as i64;
    let count_duration = count_start.elapsed();
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;
    log_trace!("[{}] Count query completed in {:?}. Total count: {}",
        request_id, count_duration, total_count);

    log_trace!("[{}] Starting search query", request_id);
    let search_start = std::time::Instant::now();
    let cursor = collection.find(filter).await?;
    let posts: Vec<Post> = cursor.try_collect().await?;
    let search_duration = search_start.elapsed();
    log_trace!("[{}] Search query completed in {:?}. Found {} posts",
        request_id, search_duration, posts.len());

    let post_responses: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();

    let total_duration = start_time.elapsed();
    log_info!("[{}] search_posts completed in {:?}. Found {} posts",
        request_id, total_duration, post_responses.len());

    Ok(Json(json!({
        "success": true,
        "posts": post_responses,
        "search_params": {
            "q": search_query,
            "user_id": search_user_id,
            "start_date": search_start_date.map(|d| d.to_rfc3339()),
            "end_date": search_end_date.map(|d| d.to_rfc3339()),
            "page": page,
            "limit": limit
        },
        "pagination": {
            "page": page,
            "limit": limit,
            "total_count": total_count,
            "total_pages": total_pages,
            "has_next": page < total_pages,
            "has_previous": page > 1
        }
    })))
}

pub async fn get_post_stats(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting get_post_stats handler", request_id);

    let collection: Collection<Post> = state.db.collection("posts");

    log_trace!("[{}] Counting total posts", request_id);
    let total_start = std::time::Instant::now();
    let total_posts: u64 = collection.count_documents(doc! {}).await?;
    let total_duration = total_start.elapsed();
    log_trace!("[{}] Total posts count completed in {:?}: {}",
        request_id, total_duration, total_posts);

    let seven_days_ago = Utc::now() - chrono::Duration::days(7);
    log_trace!("[{}] Counting posts from last week (since {})", request_id, seven_days_ago);
    let week_start = std::time::Instant::now();
    let posts_last_week: u64 = collection
        .count_documents(doc! { "created_at": { "$gte": seven_days_ago } })
        .await?;
    let week_duration = week_start.elapsed();
    log_trace!("[{}] Last week count completed in {:?}: {}",
        request_id, week_duration, posts_last_week);

    log_trace!("[{}] Aggregating top users", request_id);
    let top_users_start = std::time::Instant::now();
    let pipeline = vec![
        doc! {
            "$group": {
                "_id": "$user_id",
                "count": { "$sum": 1 },
                "user_name": { "$first": "$user_name" }
            }
        },
        doc! { "$sort": { "count": -1 } },
        doc! { "$limit": 10 },
    ];

    let cursor = collection.aggregate(pipeline).await?;
    let top_users_docs: Vec<Document> = cursor.try_collect().await?;
    let top_users: Vec<JsonValue> = top_users_docs.into_iter().map(document_to_json).collect();
    let top_users_duration = top_users_start.elapsed();
    log_trace!("[{}] Top users aggregation completed in {:?}. Found {} users",
        request_id, top_users_duration, top_users.len());

    log_trace!("[{}] Aggregating posts by hour", request_id);
    let hour_start = std::time::Instant::now();
    let hour_pipeline = vec![
        doc! {
            "$group": {
                "_id": { "$hour": "$created_at" },
                "count": { "$sum": 1 }
            }
        },
        doc! { "$sort": { "_id": 1 } },
    ];

    let cursor = collection.aggregate(hour_pipeline).await?;
    let posts_by_hour_docs: Vec<Document> = cursor.try_collect().await?;
    let posts_by_hour: Vec<JsonValue> = posts_by_hour_docs
        .into_iter()
        .map(document_to_json)
        .collect();
    let hour_duration = hour_start.elapsed();
    log_trace!("[{}] Posts by hour aggregation completed in {:?}. Found {} hour buckets",
        request_id, hour_duration, posts_by_hour.len());

    let total_duration = start_time.elapsed();
    log_info!("[{}] get_post_stats completed in {:?}", request_id, total_duration);

    Ok(Json(json!({
        "success": true,
        "stats": {
            "total_posts": total_posts,
            "posts_last_week": posts_last_week,
            "top_users": top_users,
            "posts_by_hour": posts_by_hour,
            "timestamp": Utc::now().to_rfc3339()
        }
    })))
}

pub async fn get_user_post_stats(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting get_user_post_stats handler. User ID: {}", request_id, user_id);

    let collection: Collection<Post> = state.db.collection("posts");

    let filter = doc! { "user_id": &user_id };
    log_debug!("[{}] Using filter: {:?}", request_id, filter);

    log_trace!("[{}] Counting total posts for user", request_id);
    let count_start = std::time::Instant::now();
    let total_posts: u64 = collection.count_documents(filter.clone()).await?;
    let count_duration = count_start.elapsed();
    log_trace!("[{}] Count completed in {:?}: {} posts",
        request_id, count_duration, total_posts);

    log_trace!("[{}] Finding latest and first posts", request_id);
    let find_start = std::time::Instant::now();
    let latest_post = collection.find_one(filter.clone()).await?;
    let first_post = collection.find_one(filter.clone()).await?;
    let find_duration = find_start.elapsed();
    log_trace!("[{}] Find operations completed in {:?}", request_id, find_duration);

    log_trace!("[{}] Aggregating posts by month", request_id);
    let month_start = std::time::Instant::now();
    let pipeline = vec![
        doc! { "$match": filter.clone() },
        doc! {
            "$group": {
                "_id": {
                    "year": { "$year": "$created_at" },
                    "month": { "$month": "$created_at" }
                },
                "count": { "$sum": 1 }
            }
        },
        doc! { "$sort": { "_id.year": 1, "_id.month": 1 } },
    ];

    let cursor = collection.aggregate(pipeline).await?;
    let posts_by_month_docs: Vec<Document> = cursor.try_collect().await?;
    let posts_by_month: Vec<JsonValue> = posts_by_month_docs
        .into_iter()
        .map(document_to_json)
        .collect();
    let month_duration = month_start.elapsed();
    log_trace!("[{}] Monthly aggregation completed in {:?}. Found {} months",
        request_id, month_duration, posts_by_month.len());

    let total_duration = start_time.elapsed();
    log_info!("[{}] get_user_post_stats completed in {:?}", request_id, total_duration);

    Ok(Json(json!({
        "success": true,
        "user_id": user_id,
        "stats": {
            "total_posts": total_posts,
            "latest_post": latest_post.and_then(|p| p._id.map(|id| id.to_hex())),
            "first_post": first_post.and_then(|p| p._id.map(|id| id.to_hex())),
            "posts_by_month": posts_by_month,
            "timestamp": Utc::now().to_rfc3339()
        }
    })))
}

// ========== LIKE/COMMENT HANDLERS ==========

pub async fn like_post(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting like_post handler. Post ID: {}, User ID: {}",
        request_id, post_id, payload.user_id);

    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse ObjectId: {}", request_id, e);
            return Err(AppError::PostNotFound);
        }
    };

    log_trace!("[{}] Finding post", request_id);
    let find_start = std::time::Instant::now();
    let post = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(post) => {
            log_debug!("[{}] Post found. Current likes: {}", request_id, post.likes_count);
            post
        }
        None => {
            log_warn!("[{}] Post not found: {}", request_id, post_id);
            return Err(AppError::PostNotFound);
        }
    };
    let find_duration = find_start.elapsed();
    log_trace!("[{}] Find completed in {:?}", request_id, find_duration);

    if post.liked_by.contains(&payload.user_id) {
        log_info!("[{}] User {} already liked post {}. Skipping.",
            request_id, payload.user_id, post_id);
        let post_response = PostResponse::from(post);
        return Ok(Json(json!({
            "success": true,
            "message": "Post already liked by user",
            "post": post_response
        })));
    }

    log_debug!("[{}] User {} has not liked post yet. Proceeding with like.",
        request_id, payload.user_id);

    let update_doc = doc! {
        "$inc": { "likes_count": 1 },
        "$push": { "liked_by": &payload.user_id },
        "$set": { "updated_at": Utc::now() }
    };

    log_trace!("[{}] Updating post with like", request_id);
    let update_start = std::time::Instant::now();
    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            let update_duration = update_start.elapsed();
            log_trace!("[{}] Update completed in {:?}. Matched: {}, Modified: {}",
                request_id, update_duration, result.matched_count, result.modified_count);

            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_post) => {
                    log_info!("[{}] Post {} liked successfully by user {}. New like count: {}",
                        request_id, post_id, payload.user_id, updated_post.likes_count);

                    let post_response = PostResponse::from(updated_post);

                    let total_duration = start_time.elapsed();
                    log_info!("[{}] like_post completed in {:?}", request_id, total_duration);

                    Ok(Json(json!({
                        "success": true,
                        "message": "Post liked successfully",
                        "post": post_response
                    })))
                }
                None => {
                    log_error!("[{}] Post disappeared after update: {}", request_id, post_id);
                    Err(AppError::PostNotFound)
                }
            }
        }
        Err(e) => {
            log_error!("[{}] Error liking post: {}", request_id, e);
            Err(AppError::from(e))
        }
    }
}

pub async fn unlike_post(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting unlike_post handler. Post ID: {}, User ID: {}",
        request_id, post_id, payload.user_id);

    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse ObjectId: {}", request_id, e);
            return Err(AppError::PostNotFound);
        }
    };

    log_trace!("[{}] Finding post", request_id);
    let find_start = std::time::Instant::now();
    let post = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(post) => {
            log_debug!("[{}] Post found. Current likes: {}", request_id, post.likes_count);
            post
        }
        None => {
            log_warn!("[{}] Post not found: {}", request_id, post_id);
            return Err(AppError::PostNotFound);
        }
    };
    let find_duration = find_start.elapsed();
    log_trace!("[{}] Find completed in {:?}", request_id, find_duration);

    if !post.liked_by.contains(&payload.user_id) {
        log_info!("[{}] User {} hasn't liked post {} yet. Skipping.",
            request_id, payload.user_id, post_id);
        let post_response = PostResponse::from(post);
        return Ok(Json(json!({
            "success": true,
            "message": "Post not liked by user",
            "post": post_response
        })));
    }

    log_debug!("[{}] User {} has liked post. Proceeding with unlike.",
        request_id, payload.user_id);

    let update_doc = doc! {
        "$inc": { "likes_count": -1 },
        "$pull": { "liked_by": &payload.user_id },
        "$set": { "updated_at": Utc::now() }
    };

    log_trace!("[{}] Updating post with unlike", request_id);
    let update_start = std::time::Instant::now();
    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            let update_duration = update_start.elapsed();
            log_trace!("[{}] Update completed in {:?}. Matched: {}, Modified: {}",
                request_id, update_duration, result.matched_count, result.modified_count);

            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_post) => {
                    log_info!("[{}] Post {} unliked successfully by user {}. New like count: {}",
                        request_id, post_id, payload.user_id, updated_post.likes_count);

                    let post_response = PostResponse::from(updated_post);

                    let total_duration = start_time.elapsed();
                    log_info!("[{}] unlike_post completed in {:?}", request_id, total_duration);

                    Ok(Json(json!({
                        "success": true,
                        "message": "Post unliked successfully",
                        "post": post_response
                    })))
                }
                None => {
                    log_error!("[{}] Post disappeared after update: {}", request_id, post_id);
                    Err(AppError::PostNotFound)
                }
            }
        }
        Err(e) => {
            log_error!("[{}] Error unliking post: {}", request_id, e);
            Err(AppError::from(e))
        }
    }
}

pub async fn get_comments(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting get_comments handler. Post ID: {}", request_id, post_id);
    log_debug!("[{}] Pagination params: page={:?}, limit={:?}",
        request_id, params.page, params.limit);

    let collection: Collection<Comment> = state.db.collection("comments");

    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE)
        .max(1);
    let skip = (page - 1) * limit;

    log_debug!("[{}] Pagination: page={}, limit={}, skip={}",
        request_id, page, limit, skip);

    let options = FindOptions::builder()
        .sort(doc! { "created_at": -1 })
        .skip(skip as u64)
        .limit(limit)
        .build();

    log_trace!("[{}] Counting comments", request_id);
    let count_start = std::time::Instant::now();
    let total_count = collection.count_documents(doc! { "post_id": &post_id }).await? as i64;
    let count_duration = count_start.elapsed();
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;
    log_trace!("[{}] Count completed in {:?}. Total comments: {}",
        request_id, count_duration, total_count);

    log_trace!("[{}] Fetching comments", request_id);
    let fetch_start = std::time::Instant::now();
    let cursor = collection.find(doc! { "post_id": &post_id }).await?;
    let comments: Vec<Comment> = cursor.try_collect().await?;
    let fetch_duration = fetch_start.elapsed();
    log_trace!("[{}] Fetch completed in {:?}. Found {} comments",
        request_id, fetch_duration, comments.len());

    let comment_responses: Vec<CommentResponse> = comments.into_iter().map(CommentResponse::from).collect();

    let total_duration = start_time.elapsed();
    log_info!("[{}] get_comments completed in {:?}. Found {} comments for post {}",
        request_id, total_duration, comment_responses.len(), post_id);

    Ok(Json(json!({
        "success": true,
        "comments": comment_responses,
        "post_id": post_id,
        "pagination": {
            "page": page,
            "limit": limit,
            "total_count": total_count,
            "total_pages": total_pages,
            "has_next": page < total_pages,
            "has_previous": page > 1
        }
    })))
}

pub async fn create_comment(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Json(payload): Json<CreateCommentRequest>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting create_comment handler. Post ID: {}, User ID: {}",
        request_id, post_id, payload.user_id);

    log_debug!("[{}] Comment text: {} characters", request_id, payload.comment.len());

    if payload.comment.trim().is_empty() {
        log_error!("[{}] Empty comment provided", request_id);
        return Err(AppError::invalid_data("Comment cannot be empty"));
    }

    let comment_collection: Collection<Comment> = state.db.collection("comments");
    let post_collection: Collection<Post> = state.db.collection("posts");

    let post_object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed post ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse post ObjectId: {}", request_id, e);
            return Err(AppError::PostNotFound);
        }
    };

    log_trace!("[{}] Checking if post exists", request_id);
    let post_check_start = std::time::Instant::now();
    let post_exists = post_collection.find_one(doc! { "_id": post_object_id }).await?;
    let post_check_duration = post_check_start.elapsed();

    if post_exists.is_none() {
        log_warn!("[{}] Post not found: {}", request_id, post_id);
        return Err(AppError::PostNotFound);
    }
    log_trace!("[{}] Post check completed in {:?}. Post exists.", request_id, post_check_duration);

    log_trace!("[{}] Checking for existing comment from user", request_id);
    let existing_check_start = std::time::Instant::now();
    let existing_comment = comment_collection.find_one(
        doc! {
            "post_id": &post_id,
            "user_id": &payload.user_id
        }
    ).await?;
    let existing_check_duration = existing_check_start.elapsed();

    if existing_comment.is_some() {
        log_warn!("[{}] User {} already has a comment on post {}",
            request_id, payload.user_id, post_id);
        return Err(AppError::invalid_data("You have already commented on this post. You can edit your existing comment."));
    }
    log_trace!("[{}] Existing comment check completed in {:?}. No existing comment found.",
        request_id, existing_check_duration);

    let now = Utc::now();
    let comment = Comment {
        _id: Some(ObjectId::new()),
        post_id: post_id.clone(),
        user_id: payload.user_id.clone(),
        user_name: payload.user_name.clone(),
        comment: payload.comment.clone(),
        likes_count: 0,
        liked_by: Vec::new(),
        created_at: now,
        updated_at: now,
    };

    log_debug!("[{}] Creating comment document with ID: {:?}", request_id, comment._id);

    log_trace!("[{}] Inserting comment", request_id);
    let insert_start = std::time::Instant::now();
    match comment_collection.insert_one(&comment).await {
        Ok(_) => {
            let insert_duration = insert_start.elapsed();
            log_trace!("[{}] Comment insert completed in {:?}", request_id, insert_duration);

            log_trace!("[{}] Updating post comments count", request_id);
            let update_start = std::time::Instant::now();
            let _ = post_collection.update_one(
                doc! { "_id": post_object_id },
                doc! { "$inc": { "comments_count": 1 }, "$set": { "updated_at": now } }
            ).await;
            let update_duration = update_start.elapsed();
            log_trace!("[{}] Post update completed in {:?}", request_id, update_duration);

            let comment_response = CommentResponse::from(comment);

            let total_duration = start_time.elapsed();
            log_info!("[{}] create_comment completed in {:?}. Comment created successfully",
                request_id, total_duration);

            Ok(Json(json!({
                "success": true,
                "message": "Comment created successfully",
                "comment": comment_response
            })))
        }
        Err(e) => {
            log_error!("[{}] Error creating comment: {}", request_id, e);
            Err(AppError::from(e))
        }
    }
}

pub async fn update_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
    Json(payload): Json<UpdateCommentRequest>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting update_comment handler. Comment ID: {}, User ID: {}",
        request_id, comment_id, payload.user_id);

    log_debug!("[{}] New comment text: {} characters", request_id, payload.comment.len());

    if payload.comment.trim().is_empty() {
        log_error!("[{}] Empty comment provided", request_id);
        return Err(AppError::invalid_data("Comment cannot be empty"));
    }

    let collection: Collection<Comment> = state.db.collection("comments");

    let object_id = match ObjectId::parse_str(&comment_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed comment ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse comment ObjectId: {}", request_id, e);
            return Err(AppError::invalid_data("Invalid comment ID"));
        }
    };

    log_trace!("[{}] Finding comment", request_id);
    let find_start = std::time::Instant::now();
    let comment = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(comment) => {
            log_debug!("[{}] Comment found. Current user: {}, Request user: {}",
                request_id, comment.user_id, payload.user_id);
            comment
        }
        None => {
            log_warn!("[{}] Comment not found: {}", request_id, comment_id);
            return Err(AppError::invalid_data("Comment not found"));
        }
    };
    let find_duration = find_start.elapsed();
    log_trace!("[{}] Find completed in {:?}", request_id, find_duration);

    if comment.user_id != payload.user_id {
        log_warn!("[{}] Permission denied. Comment owner: {}, Request user: {}",
            request_id, comment.user_id, payload.user_id);
        return Err(AppError::invalid_data("You can only edit your own comments"));
    }

    let update_doc = doc! {
        "$set": {
            "comment": payload.comment,
            "updated_at": Utc::now()
        }
    };

    log_debug!("[{}] Update document: {:?}", request_id, update_doc);

    log_trace!("[{}] Updating comment", request_id);
    let update_start = std::time::Instant::now();
    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            let update_duration = update_start.elapsed();
            log_trace!("[{}] Update completed in {:?}. Matched: {}, Modified: {}",
                request_id, update_duration, result.matched_count, result.modified_count);

            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_comment) => {
                    log_info!("[{}] Comment {} updated successfully", request_id, comment_id);

                    let comment_response = CommentResponse::from(updated_comment);

                    let total_duration = start_time.elapsed();
                    log_info!("[{}] update_comment completed in {:?}", request_id, total_duration);

                    Ok(Json(json!({
                        "success": true,
                        "message": "Comment updated successfully",
                        "comment": comment_response
                    })))
                }
                None => {
                    log_error!("[{}] Comment disappeared after update: {}", request_id, comment_id);
                    Err(AppError::invalid_data("Comment not found after update"))
                }
            }
        }
        Err(e) => {
            log_error!("[{}] Error updating comment: {}", request_id, e);
            Err(AppError::from(e))
        }
    }
}

pub async fn delete_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting delete_comment handler. Comment ID: {}, User ID: {}",
        request_id, comment_id, payload.user_id);

    let comment_collection: Collection<Comment> = state.db.collection("comments");

    let object_id = match ObjectId::parse_str(&comment_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed comment ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse comment ObjectId: {}", request_id, e);
            return Err(AppError::invalid_data("Invalid comment ID"));
        }
    };

    log_trace!("[{}] Finding comment", request_id);
    let find_start = std::time::Instant::now();
    let comment = match comment_collection.find_one(doc! { "_id": object_id }).await? {
        Some(comment) => {
            log_debug!("[{}] Comment found. Owner: {}, Request user: {}",
                request_id, comment.user_id, payload.user_id);
            comment
        }
        None => {
            log_warn!("[{}] Comment not found: {}", request_id, comment_id);
            return Err(AppError::invalid_data("Comment not found"));
        }
    };
    let find_duration = find_start.elapsed();
    log_trace!("[{}] Find completed in {:?}", request_id, find_duration);

    if comment.user_id != payload.user_id {
        log_warn!("[{}] Permission denied. Comment owner: {}, Request user: {}",
            request_id, comment.user_id, payload.user_id);
        return Err(AppError::invalid_data("You can only delete your own comments"));
    }

    log_trace!("[{}] Deleting comment", request_id);
    let delete_start = std::time::Instant::now();
    match comment_collection.delete_one(doc! { "_id": object_id }).await {
        Ok(result) if result.deleted_count > 0 => {
            let delete_duration = delete_start.elapsed();
            log_trace!("[{}] Delete completed in {:?}. Deleted count: {}",
                request_id, delete_duration, result.deleted_count);

            let post_collection: Collection<Post> = state.db.collection("posts");
            let post_object_id = ObjectId::parse_str(&comment.post_id);

            if let Ok(post_id) = post_object_id {
                log_trace!("[{}] Updating post comments count", request_id);
                let update_start = std::time::Instant::now();
                let _ = post_collection.update_one(
                    doc! { "_id": post_id },
                    doc! { "$inc": { "comments_count": -1 }, "$set": { "updated_at": Utc::now() } }
                ).await;
                let update_duration = update_start.elapsed();
                log_trace!("[{}] Post update completed in {:?}", request_id, update_duration);
            } else {
                log_warn!("[{}] Failed to parse post ID: {}", request_id, comment.post_id);
            }

            let total_duration = start_time.elapsed();
            log_info!("[{}] delete_comment completed in {:?}. Comment deleted successfully",
                request_id, total_duration);

            Ok(Json(json!({
                "success": true,
                "message": "Comment deleted successfully",
                "comment_id": comment_id
            })))
        }
        Ok(_) => {
            log_warn!("[{}] Comment not found during deletion: {}", request_id, comment_id);
            Err(AppError::invalid_data("Comment not found"))
        }
        Err(e) => {
            log_error!("[{}] Error deleting comment: {}", request_id, e);
            Err(AppError::from(e))
        }
    }
}

pub async fn like_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting like_comment handler. Comment ID: {}, User ID: {}",
        request_id, comment_id, payload.user_id);

    let collection: Collection<Comment> = state.db.collection("comments");

    let object_id = match ObjectId::parse_str(&comment_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed comment ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse comment ObjectId: {}", request_id, e);
            return Err(AppError::invalid_data("Invalid comment ID"));
        }
    };

    log_trace!("[{}] Finding comment", request_id);
    let find_start = std::time::Instant::now();
    let comment = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(comment) => {
            log_debug!("[{}] Comment found. Current likes: {}", request_id, comment.likes_count);
            comment
        }
        None => {
            log_warn!("[{}] Comment not found: {}", request_id, comment_id);
            return Err(AppError::invalid_data("Comment not found"));
        }
    };
    let find_duration = find_start.elapsed();
    log_trace!("[{}] Find completed in {:?}", request_id, find_duration);

    if comment.liked_by.contains(&payload.user_id) {
        log_info!("[{}] User {} already liked comment {}. Skipping.",
            request_id, payload.user_id, comment_id);
        let comment_response = CommentResponse::from(comment);
        return Ok(Json(json!({
            "success": true,
            "message": "Comment already liked by user",
            "comment": comment_response
        })));
    }

    log_debug!("[{}] User {} has not liked comment yet. Proceeding with like.",
        request_id, payload.user_id);

    let update_doc = doc! {
        "$inc": { "likes_count": 1 },
        "$push": { "liked_by": &payload.user_id },
        "$set": { "updated_at": Utc::now() }
    };

    log_trace!("[{}] Updating comment with like", request_id);
    let update_start = std::time::Instant::now();
    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            let update_duration = update_start.elapsed();
            log_trace!("[{}] Update completed in {:?}. Matched: {}, Modified: {}",
                request_id, update_duration, result.matched_count, result.modified_count);

            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_comment) => {
                    log_info!("[{}] Comment {} liked successfully by user {}. New like count: {}",
                        request_id, comment_id, payload.user_id, updated_comment.likes_count);

                    let comment_response = CommentResponse::from(updated_comment);

                    let total_duration = start_time.elapsed();
                    log_info!("[{}] like_comment completed in {:?}", request_id, total_duration);

                    Ok(Json(json!({
                        "success": true,
                        "message": "Comment liked successfully",
                        "comment": comment_response
                    })))
                }
                None => {
                    log_error!("[{}] Comment disappeared after update: {}", request_id, comment_id);
                    Err(AppError::invalid_data("Comment not found after update"))
                }
            }
        }
        Err(e) => {
            log_error!("[{}] Error liking comment: {}", request_id, e);
            Err(AppError::from(e))
        }
    }
}

pub async fn unlike_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting unlike_comment handler. Comment ID: {}, User ID: {}",
        request_id, comment_id, payload.user_id);

    let collection: Collection<Comment> = state.db.collection("comments");

    let object_id = match ObjectId::parse_str(&comment_id) {
        Ok(oid) => {
            log_debug!("[{}] Successfully parsed comment ObjectId: {}", request_id, oid);
            oid
        }
        Err(e) => {
            log_error!("[{}] Failed to parse comment ObjectId: {}", request_id, e);
            return Err(AppError::invalid_data("Invalid comment ID"));
        }
    };

    log_trace!("[{}] Finding comment", request_id);
    let find_start = std::time::Instant::now();
    let comment = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(comment) => {
            log_debug!("[{}] Comment found. Current likes: {}", request_id, comment.likes_count);
            comment
        }
        None => {
            log_warn!("[{}] Comment not found: {}", request_id, comment_id);
            return Err(AppError::invalid_data("Comment not found"));
        }
    };
    let find_duration = find_start.elapsed();
    log_trace!("[{}] Find completed in {:?}", request_id, find_duration);

    if !comment.liked_by.contains(&payload.user_id) {
        log_info!("[{}] User {} hasn't liked comment {} yet. Skipping.",
            request_id, payload.user_id, comment_id);
        let comment_response = CommentResponse::from(comment);
        return Ok(Json(json!({
            "success": true,
            "message": "Comment not liked by user",
            "comment": comment_response
        })));
    }

    log_debug!("[{}] User {} has liked comment. Proceeding with unlike.",
        request_id, payload.user_id);

    let update_doc = doc! {
        "$inc": { "likes_count": -1 },
        "$pull": { "liked_by": &payload.user_id },
        "$set": { "updated_at": Utc::now() }
    };

    log_trace!("[{}] Updating comment with unlike", request_id);
    let update_start = std::time::Instant::now();
    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            let update_duration = update_start.elapsed();
            log_trace!("[{}] Update completed in {:?}. Matched: {}, Modified: {}",
                request_id, update_duration, result.matched_count, result.modified_count);

            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_comment) => {
                    log_info!("[{}] Comment {} unliked successfully by user {}. New like count: {}",
                        request_id, comment_id, payload.user_id, updated_comment.likes_count);

                    let comment_response = CommentResponse::from(updated_comment);

                    let total_duration = start_time.elapsed();
                    log_info!("[{}] unlike_comment completed in {:?}", request_id, total_duration);

                    Ok(Json(json!({
                        "success": true,
                        "message": "Comment unliked successfully",
                        "comment": comment_response
                    })))
                }
                None => {
                    log_error!("[{}] Comment disappeared after update: {}", request_id, comment_id);
                    Err(AppError::invalid_data("Comment not found after update"))
                }
            }
        }
        Err(e) => {
            log_error!("[{}] Error unliking comment: {}", request_id, e);
            Err(AppError::from(e))
        }
    }
}
