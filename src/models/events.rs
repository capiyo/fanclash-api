use bson::DateTime as BsonDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "match_id")]
    pub match_id: String,
    #[serde(rename = "event_type")]
    pub event_type: String,
    pub minute: i32,
    #[serde(rename = "minute_display")]
    pub minute_display: String,
    #[serde(rename = "home_score")]
    pub home_score: i32,
    #[serde(rename = "away_score")]
    pub away_score: i32,
    pub player: Option<String>,
    pub team: Option<String>,
    pub player_out: Option<String>,
    pub player_in: Option<String>,
    pub shot_type: Option<String>,
    pub on_target: Option<bool>,
    pub blocked: Option<bool>,
    #[serde(rename = "created_at")]
    pub created_at: BsonDateTime,
}

// Request struct from poller
#[derive(Debug, Deserialize)]
pub struct TimelineEventRequest {
    pub match_id: String,
    pub event_type: String,
    pub minute: i32,
    pub minute_display: String,
    pub home_score: i32,
    pub away_score: i32,
    pub player: Option<String>,
    pub team: Option<String>,
    pub player_out: Option<String>,
    pub player_in: Option<String>,
    pub shot_type: Option<String>,
    pub on_target: Option<bool>,
    pub blocked: Option<bool>,
    pub timestamp: Option<String>,
}

impl TimelineEvent {
    pub fn from_request(req: TimelineEventRequest) -> Self {
        Self {
            id: format!(
                "event_{}_{}",
                req.match_id,
                chrono::Utc::now().timestamp_millis()
            ),
            match_id: req.match_id,
            event_type: req.event_type,
            minute: req.minute,
            minute_display: req.minute_display,
            home_score: req.home_score,
            away_score: req.away_score,
            player: req.player,
            team: req.team,
            player_out: req.player_out,
            player_in: req.player_in,
            shot_type: req.shot_type,
            on_target: req.on_target,
            blocked: req.blocked,
            created_at: BsonDateTime::from_chrono(chrono::Utc::now()),
        }
    }
}
