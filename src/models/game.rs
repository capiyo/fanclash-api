use serde::{Deserialize, Serialize};
use bson::{oid::ObjectId, DateTime as BsonDateTime};

// Main Game model - matches your MongoDB documents EXACTLY
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,

    #[serde(rename = "match_id")]
    pub match_id: String,

    #[serde(rename = "home_team")]
    pub home_team: String,

    #[serde(rename = "away_team")]
    pub away_team: String,

    #[serde(rename = "league")]
    pub league: String,

    #[serde(rename = "home_win")]
    pub home_win: f64,

    #[serde(rename = "away_win")]
    pub away_win: f64,

    #[serde(rename = "draw")]
    pub draw: f64,

    #[serde(rename = "date")]
    pub date: String,

    #[serde(rename = "time")]
    pub time: String,

    #[serde(rename = "home_score", skip_serializing_if = "Option::is_none")]
    pub home_score: Option<i32>,

    #[serde(rename = "away_score", skip_serializing_if = "Option::is_none")]
    pub away_score: Option<i32>,

    #[serde(rename = "status")]
    pub status: String,  // "upcoming", "live", "completed"

    #[serde(rename = "is_live")]
    pub is_live: bool,

    #[serde(rename = "last_updated")]
    pub last_updated: BsonDateTime,

    #[serde(rename = "created_at", skip_serializing_if = "Option::is_none")]
    pub created_at: Option<BsonDateTime>,

    #[serde(rename = "scraped_at", skip_serializing_if = "Option::is_none")]
    pub scraped_at: Option<BsonDateTime>,
}

// For creating new games
#[derive(Debug, Deserialize)]
pub struct CreateGame {
    pub match_id: String,
    pub home_team: String,
    pub away_team: String,
    pub league: String,
    pub home_win: f64,
    pub away_win: f64,
    pub draw: f64,
    pub date: String,
    pub time: String,
}

// For updating game scores
#[derive(Debug, Deserialize)]
pub struct UpdateGameScore {
    pub match_id: String,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub status: Option<String>,
    pub is_live: Option<bool>,
}

// For live game updates
#[derive(Debug, Deserialize)]
pub struct LiveUpdate {
    pub match_id: String,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
}

// For query parameters
#[derive(Debug, Deserialize)]
pub struct GameQuery {
    pub status: Option<String>,
    pub league: Option<String>,
    pub is_live: Option<bool>,
    pub limit: Option<i64>,
    pub skip: Option<u64>,
}

// Response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: T,
    pub message: Option<String>,
}

// For paginated responses
#[derive(Debug, Serialize)]
pub struct PaginatedGames {
    pub games: Vec<Game>,
    pub total: i64,
    pub page: u64,
    pub limit: i64,
}

// For live games response
#[derive(Debug, Serialize)]
pub struct LiveGamesResponse {
    pub live_games: Vec<Game>,
    pub count: usize,
    pub last_updated: BsonDateTime,
}