use bson::DateTime as BsonDateTime;
use serde::{Deserialize, Serialize};

// ========== TIMELINE EVENT MODEL ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "match_id")]
    pub match_id: String,
    #[serde(rename = "event_type")]
    pub event_type: String, // "goal", "yellow_card", "red_card", "substitution", "shot", "foul", "corner", "free_kick", "offside", "half_time", "second_half", "match_end"
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
    pub shot_type: Option<String>, // "left_foot", "right_foot", "header", etc.
    pub on_target: Option<bool>,
    pub blocked: Option<bool>,
    #[serde(rename = "created_at")]
    pub created_at: BsonDateTime,
}

// ========== FOR RECEIVING FROM POLLER ==========
#[derive(Debug, Deserialize)]
pub struct TimelineEventRequest {
    #[serde(rename = "fixture_id")]
    pub fixture_id: String,
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
    #[serde(rename = "player_out")]
    pub player_out: Option<String>,
    #[serde(rename = "player_in")]
    pub player_in: Option<String>,
    #[serde(rename = "shot_type")]
    pub shot_type: Option<String>,
    #[serde(rename = "on_target")]
    pub on_target: Option<bool>,
    pub blocked: Option<bool>,
    pub timestamp: String,
}

// ========== FOR RESPONSES ==========
#[derive(Debug, Serialize)]
pub struct TimelineResponse {
    pub success: bool,
    pub data: Option<Vec<TimelineEvent>>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SingleEventResponse {
    pub success: bool,
    pub data: Option<TimelineEvent>,
    pub message: Option<String>,
}

// ========== HELPER METHODS ==========
impl TimelineEvent {
    pub fn new(
        match_id: String,
        event_type: String,
        minute: i32,
        minute_display: String,
        home_score: i32,
        away_score: i32,
    ) -> Self {
        Self {
            id: format!(
                "event_{}_{}",
                match_id,
                chrono::Utc::now().timestamp_millis()
            ),
            match_id,
            event_type,
            minute,
            minute_display,
            home_score,
            away_score,
            player: None,
            team: None,
            player_out: None,
            player_in: None,
            shot_type: None,
            on_target: None,
            blocked: None,
            created_at: BsonDateTime::from_chrono(chrono::Utc::now()),
        }
    }

    pub fn with_player(mut self, player: String, team: String) -> Self {
        self.player = Some(player);
        self.team = Some(team);
        self
    }

    pub fn with_substitution(
        mut self,
        player_out: String,
        player_in: String,
        team: String,
    ) -> Self {
        self.player_out = Some(player_out);
        self.player_in = Some(player_in);
        self.team = Some(team);
        self
    }

    pub fn with_shot(
        mut self,
        player: String,
        team: String,
        shot_type: String,
        on_target: bool,
        blocked: bool,
    ) -> Self {
        self.player = Some(player);
        self.team = Some(team);
        self.shot_type = Some(shot_type);
        self.on_target = Some(on_target);
        self.blocked = Some(blocked);
        self
    }
}
