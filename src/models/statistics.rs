use bson::DateTime as BsonDateTime;
use serde::{Deserialize, Serialize};

// ========== STATISTICS MODEL ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStatistics {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "match_id")]
    pub match_id: String,
    pub minute: i32,
    #[serde(rename = "minute_display")]
    pub minute_display: String,
    #[serde(rename = "home_score")]
    pub home_score: i32,
    #[serde(rename = "away_score")]
    pub away_score: i32,
    #[serde(rename = "ball_possession_home")]
    pub ball_possession_home: i32,
    #[serde(rename = "ball_possession_away")]
    pub ball_possession_away: i32,
    #[serde(rename = "total_shots_home")]
    pub total_shots_home: i32,
    #[serde(rename = "total_shots_away")]
    pub total_shots_away: i32,
    #[serde(rename = "shots_on_target_home")]
    pub shots_on_target_home: i32,
    #[serde(rename = "shots_on_target_away")]
    pub shots_on_target_away: i32,
    #[serde(rename = "corners_home")]
    pub corners_home: i32,
    #[serde(rename = "corners_away")]
    pub corners_away: i32,
    #[serde(rename = "fouls_home")]
    pub fouls_home: i32,
    #[serde(rename = "fouls_away")]
    pub fouls_away: i32,
    #[serde(rename = "offsides_home")]
    pub offsides_home: i32,
    #[serde(rename = "offsides_away")]
    pub offsides_away: i32,
    #[serde(rename = "yellow_cards_home")]
    pub yellow_cards_home: i32,
    #[serde(rename = "yellow_cards_away")]
    pub yellow_cards_away: i32,
    #[serde(rename = "red_cards_home")]
    pub red_cards_home: i32,
    #[serde(rename = "red_cards_away")]
    pub red_cards_away: i32,
    #[serde(rename = "pass_accuracy_home")]
    pub pass_accuracy_home: i32,
    #[serde(rename = "pass_accuracy_away")]
    pub pass_accuracy_away: i32,
    #[serde(rename = "created_at")]
    pub created_at: BsonDateTime,
}

// ========== FOR RECEIVING FROM POLLER ==========
#[derive(Debug, Deserialize)]
pub struct StatisticsRequest {
    #[serde(rename = "fixture_id")]
    pub fixture_id: String,
    pub minute: i32,
    #[serde(rename = "minute_display")]
    pub minute_display: String,
    #[serde(rename = "home_score")]
    pub home_score: i32,
    #[serde(rename = "away_score")]
    pub away_score: i32,
    pub statistics: serde_json::Value,
    pub timestamp: String,
}

// ========== FOR RESPONSES ==========
#[derive(Debug, Serialize)]
pub struct StatisticsResponse {
    pub success: bool,
    pub data: Option<MatchStatistics>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatisticsListResponse {
    pub success: bool,
    pub data: Vec<MatchStatistics>,
    pub count: usize,
}
