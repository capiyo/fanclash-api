use axum::extract::State;
use axum::{http::HeaderValue, http::Method, response::Json, routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber;

mod config;
mod database;
mod dumper;
mod errors;
mod handlers;
mod middleware;
mod models;
mod routes;
mod services;
mod state;

use database::connection::get_db_client;
use state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    create_directories().await;

    let db = get_db_client().await;
    let app_state = initialize_app_state(db).await;

    let app = build_router(app_state).await;
    start_server(app).await;
}

async fn create_directories() {
    let dirs = ["uploads/images", "uploads/mpesa_receipts"];
    for dir in dirs {
        if let Err(e) = tokio::fs::create_dir_all(dir).await {
            tracing::warn!("Failed to create {}: {}", dir, e);
        }
    }
}

async fn initialize_app_state(db: mongodb::Database) -> AppState {
    // Initialize AppState with Cloudinary
    let mut app_state = match AppState::new(db) {
        Ok(state) => {
            tracing::info!("✅ Cloudinary service initialized successfully");
            state
        }
        Err(e) => {
            tracing::error!("❌ Failed to initialize Cloudinary service: {}", e);
            // You can choose to panic or continue without Cloudinary
            // For now, we'll panic since Cloudinary is likely required for image uploads
            panic!("Failed to initialize Cloudinary service: {}", e);
        }
    };

    tracing::info!("🔧 Attempting to initialize M-Pesa service...");

    // Try to load AppConfig
    let config_result = std::panic::catch_unwind(|| config::AppConfig::from_env());

    match config_result {
        Ok(config) => {
            tracing::info!("✅ App config loaded successfully");
            tracing::info!("📱 Short code: {}", config.mpesa_short_code);
            tracing::info!("🌐 Environment: {}", config.mpesa_environment);

            // Create M-Pesa service
            let mpesa_service = Arc::new(services::mpesa_service::MpesaService::new(config));

            // Try to get access token to verify credentials
            match mpesa_service.get_access_token().await {
                Ok(token) => {
                    tracing::info!("✅ M-Pesa access token obtained");
                    tracing::debug!("Token (first 20 chars): {}", &token[0..20.min(token.len())]);
                    app_state = app_state.with_mpesa(mpesa_service);
                    tracing::info!("✅ M-Pesa service initialized and ready");
                }
                Err(e) => {
                    tracing::error!("❌ Failed to get M-Pesa access token: {}", e);
                    tracing::warn!("M-Pesa service will be disabled");
                }
            }
        }
        Err(_) => {
            tracing::error!("❌ Failed to load App config (panic caught)");
            tracing::warn!("M-Pesa service will be disabled");
        }
    }

    app_state
}
async fn build_router(app_state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        /* .allow_origin([
                  "https://fanclash.netlify.app".parse::<HeaderValue>().unwrap(),
        "https://fanclash-app.netlify.app".parse::<HeaderValue>().unwrap(),
        "http://10.145.30.38:3001".parse::<HeaderValue>().unwrap(),
        "http://192.168.56.1:3001".parse::<HeaderValue>().unwrap(),
        "http://localhost:3000".parse::<HeaderValue>().unwrap(),
        "http://localhost:3001".parse::<HeaderValue>().unwrap(),
        "http://172.19.30.38:3001".parse::<HeaderValue>().unwrap(),)]*/
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .allow_credentials(false);

    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_check))
        .route("/api/health", get(api_health_check))
         //.nest("/api/engagement", routes::fixture_engagement::routes())
        .nest("/api/auth", routes::auth::routes())
        .nest("/api/games", routes::games::routes())
        .nest("/api/posts", routes::posts::routes())
        .nest("/api/bets", routes::bets::bets_routes())
        .nest("/api/pledges", routes::pledges::routes())
        .nest("/api/mpesa", routes::mpesa::mpesa_routes())
        .nest("/api/votes", routes::vote_routes::vote_routes())
        .nest("/api/archive", routes::archive::archive_routes())
          .nest("/api/chats", routes::chat::routes())

        //.nest("/api/chats", routes::ch)
       // .nest("/api/comments", routes::po))
         .nest("/comments", routes::posts::comment_routes())
        //.nest("/api/comments", routes::co)
        //.nest("/api/stats", routes::vote_routes::vote_stats_routes())
        .nest("/api/profile", routes::user_profile::user_profile_routes())
        .nest("/api", routes::posts::upload_routes())
        .layer(cors)
        .with_state(app_state)
}

async fn start_server(app: Router) {
    let port = std::env::var("PORT").unwrap_or_else(|_| "10000".to_string());
    let addr = SocketAddr::from(([0, 0, 0, 0], port.parse().unwrap_or(10000)));

    tracing::info!("🚀 Server starting on {}", addr);

    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            axum::serve(listener, app).await.unwrap();
        }
        Err(e) => {
            tracing::error!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    }
}

async fn root_handler() -> &'static str {
    "🚀 Peer-to-Peer Betting API"
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

async fn api_health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    use mongodb::bson::doc;

    let db_status = match state.db.run_command(doc! {"ping": 1}).await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    Json(serde_json::json!({
        "status": "healthy",
        "database": db_status,
        "mpesa": state.mpesa_service.is_some(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}
