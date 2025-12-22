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
use crate::models::post::{Post, PostResponse};
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

/// Helper function to convert MongoDB Document to serde_json::Value
fn document_to_json(doc: Document) -> JsonValue {
    let mut map = serde_json::Map::new();
    for (key, value) in doc {
        map.insert(key, bson_to_json_value(&value));
    }
    JsonValue::Object(map)
}

pub async fn create_post(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>> {
    let mut caption = String::new();
    let mut user_id = String::new();
    let mut user_name = String::new();
    let mut image_data = None;
    let mut file_extension = None;

    // Process multipart form data
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Multipart(format!("Failed to process multipart field: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "caption" => {
                caption = field
                    .text()
                    .await
                    .map_err(|e| AppError::Multipart(format!("Failed to read caption: {}", e)))?;
            }
            "userId" => {
                user_id = field
                    .text()
                    .await
                    .map_err(|e| AppError::Multipart(format!("Failed to read user_id: {}", e)))?;
            }
            "userName" => {
                user_name = field
                    .text()
                    .await
                    .map_err(|e| AppError::Multipart(format!("Failed to read user_name: {}", e)))?;
            }
            "image" => {
                let file_name = field.file_name().unwrap_or("image").to_string();
                let data = field.bytes().await.map_err(|e| {
                    AppError::Multipart(format!("Failed to read image data: {}", e))
                })?;

                // Validate file size
                if data.len() as u64 > MAX_FILE_SIZE {
                    return Err(AppError::ImageTooLarge);
                }

                // Validate file type
                let ext = std::path::Path::new(&file_name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
                    return Err(AppError::InvalidImageFormat);
                }

                file_extension = Some(ext);
                image_data = Some(data.to_vec());
            }
            _ => {
                continue;
            }
        }
    }

    // Validate required fields
    if user_id.trim().is_empty() {
        return Err(AppError::InvalidUserData);
    }

    if user_name.trim().is_empty() {
        return Err(AppError::InvalidUserData);
    }

    let image_data = image_data.ok_or(AppError::NoImageProvided)?;
    let file_extension = file_extension.ok_or(AppError::InvalidImageFormat)?;

    // Use Cloudinary service from AppState
    let cloudinary_service = &state.cloudinary;

    // Generate unique public ID for Cloudinary
    let public_id = format!("post_{}_{}", user_id, Uuid::new_v4());

    // Upload to Cloudinary
    let folder = format!("fanclash/posts/{}", user_id);
    let (image_url, cloudinary_public_id) = cloudinary_service
        .upload_image_with_preset(&image_data, &folder, Some(&public_id))
        .await
        .map_err(|e| AppError::invalid_data(format!("Failed to upload image: {}", e)))?;

    // Create post in MongoDB
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
        created_at: now,
        updated_at: now,
    };

    collection.insert_one(&post).await?;

    Ok(Json(json!({
        "success": true,
        "message": "Post created successfully",
        "post": {
            "id": post._id.unwrap().to_hex(),
            "image_url": image_url,
            "caption": caption,
            "user_name": user_name,
            "user_id": user_id,
            "created_at": post.created_at.to_rfc3339(),
            "updated_at": post.updated_at.to_rfc3339()
        }
    })))
}

pub async fn get_posts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = state.db.collection("posts");

    // Build filter based on query params
    let mut filter = doc! {};

    if let Some(user_id) = params.user_id {
        filter.insert("user_id", user_id);
    }

    // Pagination
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

    // Get total count for pagination metadata
    let total_count = collection.count_documents(filter.clone()).await? as i64;
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;

    // Fetch posts
    let cursor = collection.find(filter).await?;
    let posts: Vec<Post> = cursor.try_collect().await?;

    let post_responses: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();

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

pub async fn get_posts_by_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = state.db.collection("posts");

    // Build filter for specific user
    let filter = doc! { "user_id": &user_id };

    // Pagination
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

    // Get total count for pagination metadata
    let total_count = collection.count_documents(filter.clone()).await? as i64;
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;

    // Fetch posts
    let cursor = collection.find(filter).await?;
    let posts: Vec<Post> = cursor.try_collect().await?;

    let post_responses: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();

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

pub async fn get_post_by_id(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = ObjectId::parse_str(&post_id).map_err(|_| AppError::PostNotFound)?;

    let filter = doc! { "_id": object_id };

    let post = collection.find_one(filter).await?;

    match post {
        Some(post) => {
            let post_response = PostResponse::from(post);

            Ok(Json(json!({
                "success": true,
                "post": post_response
            })))
        }
        None => Err(AppError::PostNotFound),
    }
}

pub async fn update_post(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = ObjectId::parse_str(&post_id).map_err(|_| AppError::PostNotFound)?;

    // First, get the existing post
    let filter = doc! { "_id": object_id };
    let existing_post = collection.find_one(filter.clone()).await?;

    let existing_post = existing_post.ok_or(AppError::PostNotFound)?;

    let mut caption = existing_post.caption.clone();
    let mut image_data = None;
    let mut file_extension = None;
    let mut update_image = false;

    // Process multipart form data for update
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Multipart(format!("Failed to process multipart field: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "caption" => {
                let new_caption = field
                    .text()
                    .await
                    .map_err(|e| AppError::Multipart(format!("Failed to read caption: {}", e)))?;
                if !new_caption.trim().is_empty() {
                    caption = new_caption;
                }
            }
            "image" => {
                let file_name = field.file_name().unwrap_or("image").to_string();
                let data = field.bytes().await.map_err(|e| {
                    AppError::Multipart(format!("Failed to read image data: {}", e))
                })?;

                // Validate file size
                if data.len() as u64 > MAX_FILE_SIZE {
                    return Err(AppError::ImageTooLarge);
                }

                // Validate file type
                let ext = std::path::Path::new(&file_name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
                    return Err(AppError::InvalidImageFormat);
                }

                file_extension = Some(ext);
                image_data = Some(data.to_vec());
                update_image = true;
            }
            _ => {
                continue;
            }
        }
    }

    let mut update_doc = doc! {
        "updated_at": Utc::now(),
        "caption": &caption
    };

    // Use Cloudinary service from AppState
    let cloudinary_service = &state.cloudinary;
    let mut new_image_url = existing_post.image_url.clone();
    let mut new_cloudinary_public_id = existing_post.cloudinary_public_id.clone();
    let mut new_image_format = existing_post.image_format.clone();

    // If updating image, upload new one to Cloudinary
    if update_image {
        let image_data = image_data.ok_or(AppError::NoImageProvided)?;
        let file_extension = file_extension.ok_or(AppError::InvalidImageFormat)?;

        // Upload new image to Cloudinary (use same public ID to replace)
        let folder = format!("fanclash/posts/{}", existing_post.user_id);
        let (uploaded_url, uploaded_public_id) = cloudinary_service
            .upload_image_with_preset(
                &image_data,
                &folder,
                Some(&existing_post.cloudinary_public_id),
            )
            .await
            .map_err(|e| AppError::invalid_data(format!("Failed to upload image: {}", e)))?;

        new_image_url = uploaded_url;
        new_cloudinary_public_id = uploaded_public_id;
        new_image_format = file_extension;

        update_doc.insert("image_url", &new_image_url);
        update_doc.insert("cloudinary_public_id", &new_cloudinary_public_id);
        update_doc.insert("image_format", &new_image_format);
    }

    // Update the post in MongoDB
    let update = doc! {
        "$set": update_doc
    };

    let result = collection.update_one(filter, update).await?;

    if result.modified_count == 0 {
        return Ok(Json(json!({
            "success": true,
            "message": "Post updated successfully",
            "changes": false
        })));
    }

    Ok(Json(json!({
        "success": true,
        "message": "Post updated successfully",
        "changes": true,
        "post": {
            "id": post_id,
            "image_url": new_image_url,
            "caption": caption,
            "updated_at": Utc::now().to_rfc3339()
        }
    })))
}

pub async fn update_post_caption(
    State(state): State<AppState>,
    Path(post_id): Path<String>,
    axum::extract::Json(payload): axum::extract::Json<UpdateCaptionRequest>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = ObjectId::parse_str(&post_id).map_err(|_| AppError::PostNotFound)?;

    let filter = doc! { "_id": object_id };
    let update = doc! {
        "$set": {
            "caption": payload.caption.clone(),
            "updated_at": Utc::now()
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
    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = ObjectId::parse_str(&post_id).map_err(|_| AppError::PostNotFound)?;

    let filter = doc! { "_id": object_id };

    // First get the post to find the Cloudinary public ID
    let post = collection.find_one(filter.clone()).await?;

    match post {
        Some(post) => {
            // Use Cloudinary service from AppState
            let cloudinary_service = &state.cloudinary;

            // Try to delete from Cloudinary
            match cloudinary_service
                .delete_image(&post.cloudinary_public_id)
                .await
            {
                Ok(_) => {
                    println!(
                        "Successfully deleted image from Cloudinary: {}",
                        post.cloudinary_public_id
                    );
                }
                Err(e) => {
                    eprintln!(
                        "Failed to delete from Cloudinary (continuing with DB delete): {}",
                        e
                    );
                }
            }

            // Delete from MongoDB
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
    let collection: Collection<Post> = state.db.collection("posts");

    let filter = doc! { "user_id": &user_id };

    // Get all posts by user to delete from Cloudinary
    let cursor = collection.find(filter.clone()).await?;
    let posts: Vec<Post> = cursor.try_collect().await?;

    if posts.is_empty() {
        return Ok(Json(json!({
            "success": true,
            "message": "No posts found for user",
            "deleted_count": 0
        })));
    }

    // Use Cloudinary service from AppState
    let cloudinary_service = &state.cloudinary;
    let mut deleted_from_cloudinary = 0;

    // Delete all images from Cloudinary
    for post in &posts {
        match cloudinary_service
            .delete_image(&post.cloudinary_public_id)
            .await
        {
            Ok(_) => {
                deleted_from_cloudinary += 1;
                println!("Deleted from Cloudinary: {}", post.cloudinary_public_id);
            }
            Err(e) => {
                eprintln!("Failed to delete from Cloudinary: {}", e);
            }
        }
    }

    // Delete all posts from MongoDB
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
    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = ObjectId::parse_str(&post_id).map_err(|_| AppError::PostNotFound)?;

    let filter = doc! { "_id": object_id };
    let post = collection.find_one(filter).await?;

    match post {
        Some(post) => {
            // Use Cloudinary service from AppState
            let cloudinary_service = &state.cloudinary;
            let thumbnail_url = cloudinary_service.generate_thumbnail_url(
                &post.cloudinary_public_id,
                width,
                height,
            );

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
        None => Err(AppError::PostNotFound),
    }
}

pub async fn get_post_with_transform(
    State(state): State<AppState>,
    Path((post_id, transformations)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = state.db.collection("posts");

    let object_id = ObjectId::parse_str(&post_id).map_err(|_| AppError::PostNotFound)?;

    let filter = doc! { "_id": object_id };
    let post = collection.find_one(filter).await?;

    match post {
        Some(post) => {
            // Use Cloudinary service from AppState
            let cloudinary_service = &state.cloudinary;
            let transformed_url = cloudinary_service
                .generate_transformed_url(&post.cloudinary_public_id, &transformations);

            Ok(Json(json!({
                "success": true,
                "transformed_url": transformed_url,
                "post_id": post_id,
                "transformations": transformations,
                "original_url": post.image_url
            })))
        }
        None => Err(AppError::PostNotFound),
    }
}

pub async fn search_posts(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = state.db.collection("posts");

    // Clone the params for later use in response
    let search_query = params.q.clone();
    let search_user_id = params.user_id.clone();
    let search_start_date = params.start_date;
    let search_end_date = params.end_date;

    // Build search query
    let mut filter = doc! {};

    if let Some(query) = params.q {
        filter.insert(
            "caption",
            doc! {
                "$regex": query,
                "$options": "i"
            },
        );
    }

    if let Some(user_id) = params.user_id {
        filter.insert("user_id", user_id);
    }

    if let Some(start_date) = params.start_date {
        filter.insert("created_at", doc! { "$gte": start_date });
    }

    if let Some(end_date) = params.end_date {
        filter.insert("created_at", doc! { "$lte": end_date });
    }

    // Pagination
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

    // Get total count
    let total_count = collection.count_documents(filter.clone()).await? as i64;
    let total_pages = (total_count as f64 / limit as f64).ceil() as i64;

    // Fetch posts
    let cursor = collection.find(filter).await?;
    let posts: Vec<Post> = cursor.try_collect().await?;

    let post_responses: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();

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
    let collection: Collection<Post> = state.db.collection("posts");

    // Total posts count
    let total_posts: u64 = collection.count_documents(doc! {}).await?;

    // Posts by day (last 7 days)
    let seven_days_ago = Utc::now() - chrono::Duration::days(7);
    let posts_last_week: u64 = collection
        .count_documents(doc! { "created_at": { "$gte": seven_days_ago } })
        .await?;

    // Posts per user (top 10)
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

    // Convert BSON documents to JSON values using our helper function
    let top_users: Vec<JsonValue> = top_users_docs.into_iter().map(document_to_json).collect();

    // Posts by hour of day
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

// Replace the get_user_post_stats function with this fixed version:
pub async fn get_user_post_stats(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = state.db.collection("posts");

    let filter = doc! { "user_id": &user_id };

    // Total posts by user
    let total_posts: u64 = collection.count_documents(filter.clone()).await?;

    // Latest post - using FindOptions with sort
    let latest_options = mongodb::options::FindOneOptions::builder()
        .sort(doc! { "created_at": -1 })
        .build();

    let latest_post = collection.find_one(filter.clone()).await?;

    // First post - using FindOptions with sort
    let first_options = mongodb::options::FindOneOptions::builder()
        .sort(doc! { "created_at": 1 })
        .build();

    let first_post = collection.find_one(filter.clone()).await?;

    // Posts per month
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
            "latest_post": latest_post.and_then(|p| p._id.map(|id| id.to_hex())),
            "first_post": first_post.and_then(|p| p._id.map(|id| id.to_hex())),
            "posts_by_month": posts_by_month,
            "timestamp": Utc::now().to_rfc3339()
        }
    })))
}
