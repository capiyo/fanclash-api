use axum::{
    response::Json,
    extract::{State, Path}
};

use axum_extra::extract::Multipart;
use uuid::Uuid;
use chrono::Utc;
use std::path::Path as StdPath;
use tokio::fs;
use serde_json::json;
use mongodb::bson::{doc, oid::ObjectId};
use mongodb::{Database, Collection};
use futures_util::TryStreamExt;

use crate::errors::{AppError, Result};
use crate::models::post::{Post, PostResponse};

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
const ALLOWED_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "gif"];

pub async fn create_post(
    State(db): State<Database>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>> {
    let mut caption = String::new();
    let mut user_id = String::new();
    let mut user_name = String::new();
    let mut image_data = None;
    let mut file_extension = None;

    // Process multipart form data
    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::Multipart(e.to_string()))? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "caption" => {
                caption = field.text().await.map_err(|e| AppError::Multipart(e.to_string()))?;
            }
            "userId" => {
                user_id = field.text().await.map_err(|e| AppError::Multipart(e.to_string()))?;
            }
            "userName" => {
                user_name = field.text().await.map_err(|e| AppError::Multipart(e.to_string()))?;
            }
            "image" => {
                let file_name = field.file_name().unwrap_or("image").to_string();
                let data = field.bytes().await.map_err(|e| AppError::Multipart(e.to_string()))?;

                // Validate file size
                if data.len() as u64 > MAX_FILE_SIZE {
                    return Err(AppError::ImageTooLarge);
                }

                // Validate file type
                let ext = StdPath::new(&file_name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
                    return Err(AppError::InvalidImageFormat);
                }

                file_extension = Some(ext);
                image_data = Some(data);
            }
            _ => {}
        }
    }

    // Validate required fields
    if user_id.is_empty() || user_name.is_empty() {
        return Err(AppError::InvalidUserData);
    }

    let image_data = image_data.ok_or(AppError::NoImageProvided)?;
    let file_extension = file_extension.ok_or(AppError::InvalidImageFormat)?;

    // Create uploads directory if it doesn't exist
    fs::create_dir_all("uploads/images").await.map_err(AppError::Io)?;

    // Generate unique filename
    let file_name = format!("{}.{}", Uuid::new_v4(), file_extension);
    let file_path = format!("uploads/images/{}", file_name);
    let image_url = format!("/api/uploads/{}", file_name);

    // Save image to filesystem
    fs::write(&file_path, &image_data).await.map_err(AppError::Io)?;

    // Create post in MongoDB
    let collection: Collection<Post> = db.collection("posts");

    let post = Post {
        _id: Some(ObjectId::new()),
        user_id: user_id.clone(),
        user_name: user_name.clone(),
        caption: caption.clone(),
        image_url: image_url.clone(),
        image_path: file_path.clone(),
        created_at: Utc::now(),  // REMOVE the `Some()` wrapper
        updated_at: Utc::now(),
    };

    // FIXED: Use ? operator or map to AppError::MongoDB
    collection.insert_one(&post).await?;  // Auto-converts to AppError::MongoDB

    Ok(Json(json!({
        "success": true,
        "message": "Post created successfully",
        "post": {
            "id": post._id.unwrap().to_hex(),
            "image_url": image_url,
            "caption": caption,
            "user_name": user_name
        }
    })))
}


pub async fn get_posts(
    State(db): State<Database>
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = db.collection("posts");

    let cursor = collection.find(doc! {}).await?;
    let mut posts: Vec<Post> = cursor.try_collect().await?;

    // SIMPLIFIED: No need for pattern matching
    posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let post_responses: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();

    Ok(Json(json!({
        "success": true,
        "posts": post_responses
    })))
}



pub async fn get_posts_by_user(
    State(db): State<Database>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = db.collection("posts");

    let filter = doc! { "user_id": &user_id };

    let cursor = collection.find(filter).await?;
    let mut posts: Vec<Post> = cursor.try_collect().await?;

    // Sort by created_at descending (SIMPLE VERSION - no Option handling needed)
    posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let post_responses: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();

    Ok(Json(json!({
        "success": true,
        "posts": post_responses,
        "count": post_responses.len()
    })))
}

pub async fn delete_post(
    State(db): State<Database>,
    Path(post_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = db.collection("posts");

    let object_id = ObjectId::parse_str(&post_id)
        .map_err(|_| AppError::PostNotFound)?;

    let filter = doc! { "_id": object_id };

    // First get the post to find the image path
    let post = collection.find_one(filter.clone()).await?;  // FIXED: Use ?

    match post {
        Some(post) => {
            // Delete the image file from filesystem
            if let Err(e) = fs::remove_file(&post.image_path).await {
                eprintln!("Failed to delete image file {}: {}", post.image_path, e);
            }

            // Delete from MongoDB
            collection.delete_one(filter).await?;  // FIXED: Use ?

            Ok(Json(json!({
                "success": true,
                "message": "Post deleted successfully"
            })))
        }
        None => Err(AppError::PostNotFound),
    }
}

pub async fn update_post_caption(
    State(db): State<Database>,
    Path(post_id): Path<String>,
    axum::extract::Json(payload): axum::extract::Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>> {
    let new_caption = payload.get("caption")
        .and_then(|c| c.as_str())
        .ok_or(AppError::InvalidUserData)?;

    let collection: Collection<Post> = db.collection("posts");

    let object_id = ObjectId::parse_str(&post_id)
        .map_err(|_| AppError::PostNotFound)?;

    let filter = doc! { "_id": object_id };
    let update = doc! {
        "$set": {
            "caption": new_caption,
            "updated_at": Utc::now()
        }
    };

    // FIXED: Use ? operator
    let result = collection.update_one(filter, update).await?;

    if result.modified_count == 0 {
        return Err(AppError::PostNotFound);
    }

    Ok(Json(json!({
        "success": true,
        "message": "Post caption updated successfully"
    })))
}



pub async fn get_post_by_id(
    State(db): State<Database>,
    Path(post_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let collection: Collection<Post> = db.collection("posts");

    let object_id = ObjectId::parse_str(&post_id)
        .map_err(|_| AppError::PostNotFound)?;

    let filter = doc! { "_id": object_id };

    let post = collection.find_one(filter).await?;

    match post {
        Some(post) => Ok(Json(json!({
            "success": true,
            "post": PostResponse::from(post)
        }))),
        None => Err(AppError::PostNotFound),
    }
}
