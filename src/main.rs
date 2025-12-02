use tracing_subscriber;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router, http::{Method, HeaderValue},
};
use axum_extra::extract::Multipart;
use tower_http::cors::{Any, CorsLayer};

use std::net::SocketAddr;

mod routes;
mod models;
mod handlers;
mod middleware;
mod database;
mod errors;
mod dumper;

use routes::{auth, games, posts, pledges};
use database::connection::get_db_client;

#[tokio::main]
async fn main() {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create uploads directory if it doesn't exist
    if let Err(e) = tokio::fs::create_dir_all("uploads/images").await {
        tracing::warn!("Failed to create uploads directory: {}", e);
    }

    // Initialize MongoDB database connection
    let db = get_db_client().await;  // This should return Database, not Client

    // CORS configuration - ALLOW MULTIPLE ORIGINS
    let cors = CorsLayer::new()
        .allow_origin([
            "https://fanclash.netlify.app".parse::<HeaderValue>().unwrap(),
            "http://10.145.30.38:3001".parse::<HeaderValue>().unwrap(),
            "http://192.168.56.1:3001".parse::<HeaderValue>().unwrap(),
            "http://localhost:3000".parse::<HeaderValue>().unwrap(),
            "http://localhost:3001".parse::<HeaderValue>().unwrap(),
            "http://172.19.30.38:3001".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
        .allow_credentials(false);

    let app = Router::new()
        .route("/", get(|| async { "Peer-to-Peer Betting API" }))
        .nest("/api/auth", auth::routes())
        .nest("/api/games", games::routes())
        .nest("/api/posts", posts::routes())
        .nest("/api/pledges", pledges::routes())
        .nest("/api", posts::upload_routes())
        .layer(cors)
        .with_state(db);  // Pass the Database

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Server running on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}