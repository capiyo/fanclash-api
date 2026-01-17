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

// ========== HELPER FUNCTIONS ==========
fn convert_document_to_post_response(doc: Document) -> Result<PostResponse> {
    let id = doc.get_object_id("_id")
        .map_err(|_| AppError::invalid_data("Invalid _id"))?
        .to_hex();

    let created_at = parse_date_from_document(&doc, "created_at")?;
    let updated_at = parse_date_from_document(&doc, "updated_at")?;

    let liked_by = doc.get_array("liked_by")
        .map(|arr| arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect())
        .unwrap_or_else(|_| Vec::new());

    Ok(PostResponse {
        id,
        user_id: doc.get_str("user_id")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        user_name: doc.get_str("user_name")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        caption: doc.get_str("caption")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        image_url: doc.get_str("image_url")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        cloudinary_public_id: doc.get_str("cloudinary_public_id")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        image_format: doc.get_str("image_format")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        likes_count: doc.get_i32("likes_count").unwrap_or(0),
        comments_count: doc.get_i32("comments_count").unwrap_or(0),
        shares_count: doc.get_i32("shares_count").unwrap_or(0),
        liked_by,
        is_saved: doc.get_bool("is_saved").unwrap_or(false),
        created_at,
        updated_at,
    })
}

fn parse_date_from_document(doc: &Document, field: &str) -> Result<String> {
    if let Ok(bson_datetime) = doc.get_datetime(field) {
        return Ok(bson_datetime.to_chrono().to_rfc3339());
    }

    if let Ok(date_str) = doc.get_str(field) {
        if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(date_str) {
            return Ok(datetime.with_timezone(&Utc).to_rfc3339());
        }
    }

    Ok(Utc::now().to_rfc3339())
}

fn convert_document_to_comment_response(doc: Document) -> Result<CommentResponse> {
    let id = doc.get_object_id("_id")
        .map_err(|_| AppError::invalid_data("Invalid _id"))?
        .to_hex();

    let created_at = parse_date_from_document(&doc, "created_at")?;
    let updated_at = parse_date_from_document(&doc, "updated_at")?;

    let liked_by = doc.get_array("liked_by")
        .map(|arr| arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect())
        .unwrap_or_else(|_| Vec::new());

    Ok(CommentResponse {
        id,
        post_id: doc.get_str("post_id")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        user_id: doc.get_str("user_id")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        user_name: doc.get_str("user_name")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        comment: doc.get_str("comment")
            .map(|s| s.to_string())
            .unwrap_or_default(),
        likes_count: doc.get_i32("likes_count").unwrap_or(0),
        liked_by,
        created_at,
        updated_at,
    })
}

// ========== POST HANDLERS ==========
pub async fn get_posts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>> {
    use mongodb::bson::{Bson, Document, from_document};

    let start_time = std::time::Instant::now();
    let request_id = uuid::Uuid::new_v4();

    log_info!("[{}] Starting get_posts handler", request_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let mut filter = doc! {};
    if let Some(user_id) = &params.user_id {
        filter.insert("user_id", user_id);
    }

    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE)
        .max(1);
    let skip = (page - 1) * limit;

    let options = FindOptions::builder()
        .sort(doc! { "created_at": -1 })
        .skip(skip as u64)
        .limit(limit)
        .build();

    let total_count = collection.count_documents(filter.clone()).await? as i64;
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;

    let cursor = collection.find(filter).await?;
    let raw_docs: Vec<Document> = cursor.try_collect().await?;

    let mut posts = Vec::new();
    let mut errors = 0;

    for doc in raw_docs {
        #[derive(Debug, Deserialize)]
        struct TempPost {
            #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
            _id: Option<ObjectId>,
            user_id: String,
            user_name: String,
            caption: String,
            image_url: String,
            cloudinary_public_id: String,
            image_format: String,
            likes_count: i32,
            comments_count: i32,
            shares_count: i32,
            liked_by: Vec<String>,
            is_saved: bool,
            created_at: String,
            updated_at: String,
        }

        match from_document::<TempPost>(doc.clone()) {
            Ok(temp_post) => {
                let created_at = chrono::DateTime::parse_from_rfc3339(&temp_post.created_at)
                    .map_err(|_| AppError::invalid_data("Invalid created_at format"))?
                    .with_timezone(&Utc);

                let updated_at = chrono::DateTime::parse_from_rfc3339(&temp_post.updated_at)
                    .map_err(|_| AppError::invalid_data("Invalid updated_at format"))?
                    .with_timezone(&Utc);

                let post = Post {
                    _id: temp_post._id,
                    user_id: temp_post.user_id,
                    user_name: temp_post.user_name,
                    caption: temp_post.caption,
                    image_url: temp_post.image_url,
                    cloudinary_public_id: temp_post.cloudinary_public_id,
                    image_format: temp_post.image_format,
                    likes_count: temp_post.likes_count,
                    comments_count: temp_post.comments_count,
                    shares_count: temp_post.shares_count,
                    liked_by: temp_post.liked_by,
                    is_saved: temp_post.is_saved,
                    created_at,
                    updated_at,
                };
                posts.push(post);
            }
            Err(e) => {
                errors += 1;
                log_warn!("[{}] Failed to parse document: {}", request_id, e);
            }
        }
    }

    let post_responses: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();

    Ok(Json(json!({
        "success": true,
        "posts": post_responses,
        "errors": errors,
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
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting create_post handler", request_id);

    let mut caption = String::new();
    let mut user_id = String::new();
    let mut user_name = String::new();
    let mut image_data = None;
    let mut file_extension = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| {
            log_error!("[{}] Failed to process multipart field: {}", request_id, e);
            AppError::Multipart(format!("Failed to process multipart field: {}", e))
        })?
    {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "caption" => {
                caption = field
                    .text()
                    .await
                    .map_err(|e| {
                        log_error!("[{}] Failed to read caption: {}", request_id, e);
                        AppError::Multipart(format!("Failed to read caption: {}", e))
                    })?;
            }
            "userId" => {
                user_id = field
                    .text()
                    .await
                    .map_err(|e| {
                        log_error!("[{}] Failed to read user_id: {}", request_id, e);
                        AppError::Multipart(format!("Failed to read user_id: {}", e))
                    })?;
            }
            "userName" => {
                user_name = field
                    .text()
                    .await
                    .map_err(|e| {
                        log_error!("[{}] Failed to read user_name: {}", request_id, e);
                        AppError::Multipart(format!("Failed to read user_name: {}", e))
                    })?;
            }
            "image" => {
                let file_name = field.file_name().unwrap_or("image").to_string();
                let data = field.bytes().await.map_err(|e| {
                    log_error!("[{}] Failed to read image data: {}", request_id, e);
                    AppError::Multipart(format!("Failed to read image data: {}", e))
                })?;

                if data.len() as u64 > MAX_FILE_SIZE {
                    return Err(AppError::ImageTooLarge);
                }

                let ext = std::path::Path::new(&file_name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
                    return Err(AppError::InvalidImageFormat);
                }

                file_extension = Some(ext.clone());
                image_data = Some(data.to_vec());
            }
            _ => continue,
        }
    }

    if user_id.trim().is_empty() {
        return Err(AppError::InvalidUserData);
    }

    if user_name.trim().is_empty() {
        return Err(AppError::InvalidUserData);
    }

    let image_data = image_data.ok_or_else(|| {
        AppError::NoImageProvided
    })?;

    let file_extension = file_extension.ok_or_else(|| {
        AppError::InvalidImageFormat
    })?;

    let cloudinary_service = &state.cloudinary;
    let public_id = format!("post_{}_{}", user_id, Uuid::new_v4());
    let upload_path = format!("fanclash/posts/{}", user_id);

    let (image_url, cloudinary_public_id) = match cloudinary_service
        .upload_image_with_preset(
            &image_data,
            &upload_path,
            Some(&public_id),
        )
        .await
    {
        Ok(result) => result,
        Err(preset_error) => {
            cloudinary_service
                .upload_image_signed(
                    &image_data,
                    &upload_path,
                    Some(&public_id),
                )
                .await
                .map_err(|e| {
                    AppError::invalid_data(format!("Both upload methods failed. Last error: {}", e))
                })?
        }
    };

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

    collection.insert_one(&post).await?;

    let post_response = PostResponse::from(post);

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
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting get_post_by_id handler. Post ID: {}", request_id, post_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::PostNotFound),
    };

    let filter = doc! { "_id": object_id };

    let post_doc = collection.find_one(filter).await?;

    match post_doc {
        Some(doc) => {
            match convert_document_to_post_response(doc) {
                Ok(post_response) => {
                    Ok(Json(json!({
                        "success": true,
                        "post": post_response
                    })))
                }
                Err(e) => {
                    log_error!("[{}] Failed to convert document: {}", request_id, e);
                    Err(e)
                }
            }
        }
        None => Err(AppError::PostNotFound),
    }
}

pub async fn get_posts_by_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting get_posts_by_user handler. User ID: {}", request_id, user_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let filter = doc! { "user_id": &user_id };

    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE)
        .max(1);
    let skip = (page - 1) * limit;

    let options = FindOptions::builder()
        .sort(doc! { "created_at": -1 })
        .skip(skip as u64)
        .limit(limit)
        .build();

    let total_count = collection.count_documents(filter.clone()).await? as i64;
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;

    let cursor = collection.find(filter).await?;
    let raw_docs: Vec<Document> = cursor.try_collect().await?;

    let mut post_responses = Vec::new();
    for doc in raw_docs {
        match convert_document_to_post_response(doc) {
            Ok(post_response) => post_responses.push(post_response),
            Err(e) => log_warn!("[{}] Failed to convert document: {}", request_id, e),
        }
    }

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
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting update_post_caption handler. Post ID: {}", request_id, post_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::PostNotFound),
    };

    let filter = doc! { "_id": object_id };
    let now_bson = mongodb::bson::DateTime::from_chrono(Utc::now());
    let update = doc! {
        "$set": {
            "caption": payload.caption.clone(),
            "updated_at": now_bson
        }
    };

    let result = collection.update_one(filter, update).await?;

    if result.matched_count == 0 {
        return Err(AppError::PostNotFound);
    }

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
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting delete_post handler. Post ID: {}", request_id, post_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::PostNotFound),
    };

    let filter = doc! { "_id": object_id };

    let post_doc = collection.find_one(filter.clone()).await?;

    match post_doc {
        Some(doc) => {
            let cloudinary_service = &state.cloudinary;
            if let Ok(public_id) = doc.get_str("cloudinary_public_id") {
                let _ = cloudinary_service.delete_image(public_id).await;
            }

            let delete_result = collection.delete_one(filter).await?;

            if delete_result.deleted_count == 0 {
                return Err(AppError::PostNotFound);
            }

            Ok(Json(json!({
                "success": true,
                "message": "Post deleted successfully",
                "post_id": post_id
            })))
        }
        None => Err(AppError::PostNotFound),
    }
}

pub async fn delete_posts_by_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting delete_posts_by_user handler. User ID: {}", request_id, user_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let filter = doc! { "user_id": &user_id };

    let cursor = collection.find(filter.clone()).await?;
    let posts: Vec<Document> = cursor.try_collect().await?;

    if posts.is_empty() {
        return Ok(Json(json!({
            "success": true,
            "message": "No posts found for user",
            "deleted_count": 0
        })));
    }

    let cloudinary_service = &state.cloudinary;
    let mut deleted_from_cloudinary = 0;

    for post in &posts {
        if let Ok(public_id) = post.get_str("cloudinary_public_id") {
            let _ = cloudinary_service.delete_image(public_id).await;
            deleted_from_cloudinary += 1;
        }
    }

    let delete_result = collection.delete_many(filter).await?;

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
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting get_post_thumbnail handler. Post ID: {}, Dimensions: {}x{}",
        request_id, post_id, width, height);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::PostNotFound),
    };

    let filter = doc! { "_id": object_id };

    let post_doc = collection.find_one(filter).await?;

    match post_doc {
        Some(doc) => {
            let cloudinary_service = &state.cloudinary;
            let cloudinary_public_id = doc.get_str("cloudinary_public_id")
                .unwrap_or("");
            let thumbnail_url = cloudinary_service.generate_thumbnail_url(
                cloudinary_public_id,
                width,
                height,
            );

            let image_url = doc.get_str("image_url").unwrap_or("").to_string();

            Ok(Json(json!({
                "success": true,
                "thumbnail_url": thumbnail_url,
                "post_id": post_id,
                "original_url": image_url,
                "dimensions": {
                    "width": width,
                    "height": height
                }
            })))
        }
        None => Err(AppError::PostNotFound),
    }
}

pub async fn get_post_with_transform(
    State(state): State<AppState>,
    Path((post_id, transformations)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting get_post_with_transform handler. Post ID: {}, Transformations: {}",
        request_id, post_id, transformations);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::PostNotFound),
    };

    let filter = doc! { "_id": object_id };

    let post_doc = collection.find_one(filter).await?;

    match post_doc {
        Some(doc) => {
            let cloudinary_service = &state.cloudinary;
            let cloudinary_public_id = doc.get_str("cloudinary_public_id")
                .unwrap_or("");
            let transformed_url = cloudinary_service
                .generate_transformed_url(cloudinary_public_id, &transformations);

            let image_url = doc.get_str("image_url").unwrap_or("").to_string();

            Ok(Json(json!({
                "success": true,
                "transformed_url": transformed_url,
                "post_id": post_id,
                "transformations": transformations,
                "original_url": image_url
            })))
        }
        None => Err(AppError::PostNotFound),
    }
}

pub async fn search_posts(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting search_posts handler", request_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let mut filter = doc! {};

    if let Some(query) = &params.q {
        filter.insert(
            "caption",
            doc! {
                "$regex": query,
                "$options": "i"
            },
        );
    }

    if let Some(user_id) = &params.user_id {
        filter.insert("user_id", user_id);
    }

    if let Some(start_date) = &params.start_date {
        filter.insert("created_at", doc! { "$gte": start_date });
    }

    if let Some(end_date) = &params.end_date {
        filter.insert("created_at", doc! { "$lte": end_date });
    }

    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE)
        .max(1);
    let skip = (page - 1) * limit;

    let options = FindOptions::builder()
        .sort(doc! { "created_at": -1 })
        .skip(skip as u64)
        .limit(limit)
        .build();

    let total_count = collection.count_documents(filter.clone()).await? as i64;
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;

    let cursor = collection.find(filter).await?;
    let raw_docs: Vec<Document> = cursor.try_collect().await?;

    let mut post_responses = Vec::new();
    for doc in raw_docs {
        match convert_document_to_post_response(doc) {
            Ok(post_response) => post_responses.push(post_response),
            Err(e) => log_warn!("[{}] Failed to convert document: {}", request_id, e),
        }
    }

    Ok(Json(json!({
        "success": true,
        "posts": post_responses,
        "search_params": {
            "q": params.q,
            "user_id": params.user_id,
            "start_date": params.start_date.map(|d| d.to_rfc3339()),
            "end_date": params.end_date.map(|d| d.to_rfc3339()),
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
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting get_post_stats handler", request_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let total_posts: u64 = collection.count_documents(doc! {}).await?;

    let seven_days_ago = Utc::now() - chrono::Duration::days(7);
    let posts_last_week: u64 = collection
        .count_documents(doc! { "created_at": { "$gte": seven_days_ago } })
        .await?;

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
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting get_user_post_stats handler. User ID: {}", request_id, user_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let filter = doc! { "user_id": &user_id };

    let total_posts: u64 = collection.count_documents(filter.clone()).await?;

    let latest_post = collection.find_one(filter.clone()).await?;
    let first_post = collection.find_one(filter.clone()).await?;

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

    Ok(Json(json!({
        "success": true,
        "user_id": user_id,
        "stats": {
            "total_posts": total_posts,
            "latest_post": latest_post.and_then(|p| p.get_object_id("_id").ok().map(|id| id.to_hex())),
            "first_post": first_post.and_then(|p| p.get_object_id("_id").ok().map(|id| id.to_hex())),
            "posts_by_month": posts_by_month,
            "timestamp": Utc::now().to_rfc3339()
        }
    })))
}

// ========== LIKE HANDLERS ==========
pub async fn like_post(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting like_post handler. Post ID: {}, User ID: {}",
        request_id, post_id, payload.user_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::PostNotFound),
    };

    let post_doc = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(doc) => doc,
        None => return Err(AppError::PostNotFound),
    };

    if let Some(liked_by) = post_doc.get_array("liked_by").ok() {
        let liked_by_list: Vec<String> = liked_by
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        if liked_by_list.contains(&payload.user_id) {
            let post_response = convert_document_to_post_response(post_doc)?;

            return Ok(Json(json!({
                "success": true,
                "message": "Post already liked by user",
                "post": post_response
            })));
        }
    }

    let now_bson = mongodb::bson::DateTime::from_chrono(Utc::now());

    let update_doc = doc! {
        "$inc": { "likes_count": 1 },
        "$push": { "liked_by": &payload.user_id },
        "$set": { "updated_at": now_bson }
    };

    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_doc) => {
                    let post_response = convert_document_to_post_response(updated_doc)?;

                    Ok(Json(json!({
                        "success": true,
                        "message": "Post liked successfully",
                        "post": post_response
                    })))
                }
                None => Err(AppError::PostNotFound),
            }
        }
        Err(e) => Err(AppError::from(e)),
    }
}

pub async fn unlike_post(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting unlike_post handler. Post ID: {}, User ID: {}",
        request_id, post_id, payload.user_id);

    let collection: Collection<Document> = state.db.collection::<Document>("posts");

    let object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::PostNotFound),
    };

    let post_doc = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(doc) => doc,
        None => return Err(AppError::PostNotFound),
    };

    let has_liked = if let Ok(liked_by) = post_doc.get_array("liked_by") {
        liked_by
            .iter()
            .any(|v| v.as_str().map(|s| s == &payload.user_id).unwrap_or(false))
    } else {
        false
    };

    if !has_liked {
        let post_response = convert_document_to_post_response(post_doc)?;

        return Ok(Json(json!({
            "success": true,
            "message": "Post not liked by user",
            "post": post_response
        })));
    }

    let now_bson = mongodb::bson::DateTime::from_chrono(Utc::now());

    let update_doc = doc! {
        "$inc": { "likes_count": -1 },
        "$pull": { "liked_by": &payload.user_id },
        "$set": { "updated_at": now_bson }
    };

    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_doc) => {
                    let post_response = convert_document_to_post_response(updated_doc)?;

                    Ok(Json(json!({
                        "success": true,
                        "message": "Post unliked successfully",
                        "post": post_response
                    })))
                }
                None => Err(AppError::PostNotFound),
            }
        }
        Err(e) => Err(AppError::from(e)),
    }
}

// ========== COMMENT HANDLERS ==========
pub async fn get_comments(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting get_comments handler. Post ID: {}", request_id, post_id);

    let collection: Collection<Document> = state.db.collection::<Document>("comments");

    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .min(MAX_PAGE_SIZE)
        .max(1);
    let skip = (page - 1) * limit;

    let options = FindOptions::builder()
        .sort(doc! { "created_at": -1 })
        .skip(skip as u64)
        .limit(limit)
        .build();

    let total_count = collection.count_documents(doc! { "post_id": &post_id }).await? as i64;
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;

    let cursor = collection.find(doc! { "post_id": &post_id }).await?;
    let raw_docs: Vec<Document> = cursor.try_collect().await?;

    let mut comment_responses = Vec::new();
    for doc in raw_docs {
        match convert_document_to_comment_response(doc) {
            Ok(comment_response) => comment_responses.push(comment_response),
            Err(e) => log_warn!("[{}] Failed to convert comment: {}", request_id, e),
        }
    }

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
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting create_comment handler. Post ID: {}, User ID: {}",
        request_id, post_id, payload.user_id);

    if payload.comment.trim().is_empty() {
        return Err(AppError::invalid_data("Comment cannot be empty"));
    }

    let comment_collection: Collection<Document> = state.db.collection::<Document>("comments");
    let post_collection: Collection<Document> = state.db.collection::<Document>("posts");

    let post_object_id = match ObjectId::parse_str(&post_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::PostNotFound),
    };

    let post_exists = post_collection.find_one(doc! { "_id": post_object_id }).await?;
    if post_exists.is_none() {
        return Err(AppError::PostNotFound);
    }

    let existing_comment = comment_collection.find_one(
        doc! {
            "post_id": &post_id,
            "user_id": &payload.user_id
        }
    ).await?;

    if existing_comment.is_some() {
        return Err(AppError::invalid_data("You have already commented on this post. You can edit your existing comment."));
    }

    let now = Utc::now();
    let now_bson = mongodb::bson::DateTime::from_chrono(now);

    let comment_doc = doc! {
        "_id": ObjectId::new(),
        "post_id": post_id.clone(),
        "user_id": payload.user_id.clone(),
        "user_name": payload.user_name.clone(),
        "comment": payload.comment.clone(),
        "likes_count": 0,
        "liked_by": [],
        "created_at": now_bson,
        "updated_at": now_bson,
    };

    match comment_collection.insert_one(comment_doc.clone()).await {
        Ok(_) => {
            let post_update_now = mongodb::bson::DateTime::from_chrono(Utc::now());
            let _ = post_collection.update_one(
                doc! { "_id": post_object_id },
                doc! { "$inc": { "comments_count": 1 }, "$set": { "updated_at": post_update_now } }
            ).await;

            let comment_response = convert_document_to_comment_response(comment_doc)?;

            Ok(Json(json!({
                "success": true,
                "message": "Comment created successfully",
                "comment": comment_response
            })))
        }
        Err(e) => Err(AppError::from(e)),
    }
}

pub async fn update_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
    Json(payload): Json<UpdateCommentRequest>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting update_comment handler. Comment ID: {}, User ID: {}",
        request_id, comment_id, payload.user_id);

    if payload.comment.trim().is_empty() {
        return Err(AppError::invalid_data("Comment cannot be empty"));
    }

    let collection: Collection<Document> = state.db.collection::<Document>("comments");

    let object_id = match ObjectId::parse_str(&comment_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::invalid_data("Invalid comment ID")),
    };

    let comment_doc = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(doc) => doc,
        None => return Err(AppError::invalid_data("Comment not found")),
    };

    let comment_user_id = comment_doc.get_str("user_id").unwrap_or("");
    if comment_user_id != payload.user_id {
        return Err(AppError::invalid_data("You can only edit your own comments"));
    }

    let now_bson = mongodb::bson::DateTime::from_chrono(Utc::now());
    let update_doc = doc! {
        "$set": {
            "comment": payload.comment,
            "updated_at": now_bson
        }
    };

    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_doc) => {
                    let comment_response = convert_document_to_comment_response(updated_doc)?;

                    Ok(Json(json!({
                        "success": true,
                        "message": "Comment updated successfully",
                        "comment": comment_response
                    })))
                }
                None => Err(AppError::invalid_data("Comment not found after update")),
            }
        }
        Err(e) => Err(AppError::from(e)),
    }
}

pub async fn delete_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting delete_comment handler. Comment ID: {}, User ID: {}",
        request_id, comment_id, payload.user_id);

    let comment_collection: Collection<Document> = state.db.collection::<Document>("comments");

    let object_id = match ObjectId::parse_str(&comment_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::invalid_data("Invalid comment ID")),
    };

    let comment_doc = match comment_collection.find_one(doc! { "_id": object_id }).await? {
        Some(doc) => doc,
        None => return Err(AppError::invalid_data("Comment not found")),
    };

    let comment_user_id = comment_doc.get_str("user_id").unwrap_or("");
    if comment_user_id != payload.user_id {
        return Err(AppError::invalid_data("You can only delete your own comments"));
    }

    match comment_collection.delete_one(doc! { "_id": object_id }).await {
        Ok(result) if result.deleted_count > 0 => {
            let post_id = comment_doc.get_str("post_id").unwrap_or("");
            let post_object_id = ObjectId::parse_str(post_id);

            if let Ok(post_id) = post_object_id {
                let post_collection: Collection<Document> = state.db.collection::<Document>("posts");
                let post_update_now = mongodb::bson::DateTime::from_chrono(Utc::now());
                let _ = post_collection.update_one(
                    doc! { "_id": post_id },
                    doc! { "$inc": { "comments_count": -1 }, "$set": { "updated_at": post_update_now } }
                ).await;
            }

            Ok(Json(json!({
                "success": true,
                "message": "Comment deleted successfully",
                "comment_id": comment_id
            })))
        }
        Ok(_) => Err(AppError::invalid_data("Comment not found")),
        Err(e) => Err(AppError::from(e)),
    }
}

pub async fn like_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting like_comment handler. Comment ID: {}, User ID: {}",
        request_id, comment_id, payload.user_id);

    let collection: Collection<Document> = state.db.collection::<Document>("comments");

    let object_id = match ObjectId::parse_str(&comment_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::invalid_data("Invalid comment ID")),
    };

    let comment_doc = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(doc) => doc,
        None => return Err(AppError::invalid_data("Comment not found")),
    };

    if let Some(liked_by) = comment_doc.get_array("liked_by").ok() {
        let liked_by_list: Vec<String> = liked_by
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        if liked_by_list.contains(&payload.user_id) {
            let comment_response = convert_document_to_comment_response(comment_doc)?;
            return Ok(Json(json!({
                "success": true,
                "message": "Comment already liked by user",
                "comment": comment_response
            })));
        }
    }

    let now_bson = mongodb::bson::DateTime::from_chrono(Utc::now());

    let update_doc = doc! {
        "$inc": { "likes_count": 1 },
        "$push": { "liked_by": &payload.user_id },
        "$set": { "updated_at": now_bson }
    };

    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_doc) => {
                    let comment_response = convert_document_to_comment_response(updated_doc)?;

                    Ok(Json(json!({
                        "success": true,
                        "message": "Comment liked successfully",
                        "comment": comment_response
                    })))
                }
                None => Err(AppError::invalid_data("Comment not found after update")),
            }
        }
        Err(e) => Err(AppError::from(e)),
    }
}

pub async fn unlike_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<String>,
    Json(payload): Json<LikeRequest>,
) -> Result<Json<serde_json::Value>> {
    let request_id = uuid::Uuid::new_v4();
    log_info!("[{}] Starting unlike_comment handler. Comment ID: {}, User ID: {}",
        request_id, comment_id, payload.user_id);

    let collection: Collection<Document> = state.db.collection::<Document>("comments");

    let object_id = match ObjectId::parse_str(&comment_id) {
        Ok(oid) => oid,
        Err(_) => return Err(AppError::invalid_data("Invalid comment ID")),
    };

    let comment_doc = match collection.find_one(doc! { "_id": object_id }).await? {
        Some(doc) => doc,
        None => return Err(AppError::invalid_data("Comment not found")),
    };

    let has_liked = if let Ok(liked_by) = comment_doc.get_array("liked_by") {
        liked_by
            .iter()
            .any(|v| v.as_str().map(|s| s == &payload.user_id).unwrap_or(false))
    } else {
        false
    };

    if !has_liked {
        let comment_response = convert_document_to_comment_response(comment_doc)?;
        return Ok(Json(json!({
            "success": true,
            "message": "Comment not liked by user",
            "comment": comment_response
        })));
    }

    let now_bson = mongodb::bson::DateTime::from_chrono(Utc::now());

    let update_doc = doc! {
        "$inc": { "likes_count": -1 },
        "$pull": { "liked_by": &payload.user_id },
        "$set": { "updated_at": now_bson }
    };

    match collection.update_one(doc! { "_id": object_id }, update_doc).await {
        Ok(result) => {
            match collection.find_one(doc! { "_id": object_id }).await? {
                Some(updated_doc) => {
                    let comment_response = convert_document_to_comment_response(updated_doc)?;

                    Ok(Json(json!({
                        "success": true,
                        "message": "Comment unliked successfully",
                        "comment": comment_response
                    })))
                }
                None => Err(AppError::invalid_data("Comment not found after update")),
            }
        }
        Err(e) => Err(AppError::from(e)),
    }
}
