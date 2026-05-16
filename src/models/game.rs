use bson::DateTime as BsonDateTime;
use serde::{Deserialize, Serialize};

// ========== VOTER STRUCT - Individual voter in the array ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voter {
    #[serde(rename = "userId")]
    pub user_id: String,

    #[serde(rename = "userName")]
    pub user_name: String,

    #[serde(rename = "selection")]
    pub selection: String,

    #[serde(rename = "votedAt")]
    pub voted_at: BsonDateTime,
}

// ========== TIMELINE EVENT STRUCT ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    #[serde(rename = "match_id")]
    pub match_id: String,

    #[serde(rename = "event_type")]
    pub event_type: String, // "goal", "yellow_card", "half_time", "full_time", "corner"

    #[serde(rename = "data")]
    pub data: TimelineEventData,

    #[serde(rename = "timestamp")]
    pub timestamp: BsonDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEventData {
    #[serde(rename = "minute", skip_serializing_if = "Option::is_none")]
    pub minute: Option<i32>,

    #[serde(rename = "scorer", skip_serializing_if = "Option::is_none")]
    pub scorer: Option<String>,

    #[serde(rename = "scored_team", skip_serializing_if = "Option::is_none")]
    pub scored_team: Option<String>,

    #[serde(rename = "player", skip_serializing_if = "Option::is_none")]
    pub player: Option<String>,

    #[serde(rename = "team", skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,

    #[serde(rename = "home_score", skip_serializing_if = "Option::is_none")]
    pub home_score: Option<i32>,

    #[serde(rename = "away_score", skip_serializing_if = "Option::is_none")]
    pub away_score: Option<i32>,

    #[serde(rename = "score_display", skip_serializing_if = "Option::is_none")]
    pub score_display: Option<String>,
}

// ========== STATISTICS STRUCT ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStatistics {
    #[serde(rename = "match_id")]
    pub match_id: String,

    #[serde(rename = "minute")]
    pub minute: i32,

    #[serde(rename = "stats")]
    pub stats: StatisticsData,

    #[serde(rename = "recorded_at")]
    pub recorded_at: BsonDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatisticsData {
    #[serde(rename = "ball_possession", skip_serializing_if = "Option::is_none")]
    pub ball_possession: Option<HomeAway<i32>>,

    #[serde(rename = "total_shots", skip_serializing_if = "Option::is_none")]
    pub total_shots: Option<HomeAway<i32>>,

    #[serde(rename = "shots_on_target", skip_serializing_if = "Option::is_none")]
    pub shots_on_target: Option<HomeAway<i32>>,

    #[serde(rename = "corners", skip_serializing_if = "Option::is_none")]
    pub corners: Option<HomeAway<i32>>,

    #[serde(rename = "fouls", skip_serializing_if = "Option::is_none")]
    pub fouls: Option<HomeAway<i32>>,

    #[serde(rename = "offsides", skip_serializing_if = "Option::is_none")]
    pub offsides: Option<HomeAway<i32>>,

    #[serde(rename = "yellow_cards", skip_serializing_if = "Option::is_none")]
    pub yellow_cards: Option<HomeAway<i32>>,

    #[serde(rename = "red_cards", skip_serializing_if = "Option::is_none")]
    pub red_cards: Option<HomeAway<i32>>,

    #[serde(rename = "pass_accuracy", skip_serializing_if = "Option::is_none")]
    pub pass_accuracy: Option<HomeAway<i32>>,

    #[serde(rename = "tackles", skip_serializing_if = "Option::is_none")]
    pub tackles: Option<HomeAway<i32>>,

    #[serde(rename = "saves", skip_serializing_if = "Option::is_none")]
    pub saves: Option<HomeAway<i32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HomeAway<T> {
    pub home: T,
    pub away: T,
}

// ========== MAIN GAME MODEL - UPDATED ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    #[serde(rename = "_id")]
    pub id: String,

    #[serde(rename = "match_id")]
    pub match_id: String,

    #[serde(rename = "sofascore_id", skip_serializing_if = "Option::is_none")]
    pub sofascore_id: Option<i64>,

    #[serde(rename = "round", skip_serializing_if = "Option::is_none")]
    pub round: Option<i32>,

    #[serde(rename = "home_team")]
    pub home_team: String,

    #[serde(rename = "away_team")]
    pub away_team: String,

    #[serde(rename = "league")]
    pub league: String,

    #[serde(rename = "tournament", skip_serializing_if = "Option::is_none")]
    pub tournament: Option<String>,

    #[serde(rename = "year", skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,

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
    pub status: String,

    #[serde(rename = "is_live")]
    pub is_live: bool,

    #[serde(rename = "available_for_voting")]
    pub available_for_voting: bool,

    // ========== LIVE MATCH TRACKING FIELDS ==========
    #[serde(rename = "time_elapsed", default)]
    pub time_elapsed: i32,

    #[serde(rename = "period", default)]
    pub period: String,

    #[serde(rename = "last_goal_at", skip_serializing_if = "Option::is_none")]
    pub last_goal_at: Option<BsonDateTime>,

    #[serde(rename = "last_goal_minute", skip_serializing_if = "Option::is_none")]
    pub last_goal_minute: Option<i32>,

    #[serde(rename = "last_goal_scorer", skip_serializing_if = "Option::is_none")]
    pub last_goal_scorer: Option<String>,

    #[serde(rename = "live_started_at", skip_serializing_if = "Option::is_none")]
    pub live_started_at: Option<BsonDateTime>,

    #[serde(rename = "completed_at", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<BsonDateTime>,

    #[serde(rename = "last_polled_at", skip_serializing_if = "Option::is_none")]
    pub last_polled_at: Option<BsonDateTime>,

    // ========== VENUE FIELDS ==========
    #[serde(rename = "venue", default)]
    pub venue: String,

    #[serde(rename = "venue_city", default)]
    pub venue_city: String,

    #[serde(rename = "venue_country", default)]
    pub venue_country: String,

    #[serde(rename = "source")]
    pub source: String,

    #[serde(rename = "scraped_at")]
    pub scraped_at: BsonDateTime,

    #[serde(rename = "date_iso")]
    pub date_iso: String,

    // ========== COUNTER FIELDS ==========
    #[serde(rename = "votes", default)]
    pub votes: i64,

    #[serde(rename = "comments", default)]
    pub comments: i64,

    // ========== VOTERS ARRAY ==========
    #[serde(rename = "voters", default)]
    pub voters: Vec<Voter>,
}

// ========== FOR CREATING NEW GAMES ==========
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
    pub date_iso: String,
    pub source: String,
}

// ========== FOR UPDATING GAME SCORES ==========
#[derive(Debug, Deserialize)]
pub struct UpdateGameScore {
    pub match_id: String,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
    pub status: Option<String>,
    pub is_live: Option<bool>,
    pub time_elapsed: Option<i32>,
    pub period: Option<String>,
}

// ========== FOR LIVE GAME UPDATES (WebSocket) ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveGameUpdate {
    pub fixture_id: String,
    pub event_type: String, // "goal", "yellow_card", "half_time", "full_time"
    pub home_score: i32,
    pub away_score: i32,
    pub minute: i32,
    pub scorer: Option<String>, // "home_team" or "away_team" for goals
    pub player: Option<String>, // Player name for cards
    pub team: Option<String>,   // Which team for cards/corners
    pub timestamp: BsonDateTime,
}

// ========== FOR QUERY PARAMETERS ==========
#[derive(Debug, Deserialize)]
pub struct GameQuery {
    pub status: Option<String>,
    pub league: Option<String>,
    pub is_live: Option<bool>,
    pub limit: Option<i64>,
    pub skip: Option<u64>,
    pub source: Option<String>,
    pub tournament: Option<String>,
}

// ========== RESPONSE WRAPPERS ==========
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: T,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedGames {
    pub games: Vec<Game>,
    pub total: i64,
    pub page: u64,
    pub limit: i64,
}

#[derive(Debug, Serialize)]
pub struct LiveGamesResponse {
    pub live_games: Vec<Game>,
    pub count: usize,
    pub last_updated: BsonDateTime,
}

#[derive(Debug, Serialize)]
pub struct GameStats {
    pub total_games: i64,
    pub upcoming_games: i64,
    pub live_games: i64,
    pub completed_games: i64,
    pub by_league: Vec<LeagueStats>,
}

#[derive(Debug, Serialize)]
pub struct LeagueStats {
    pub league: String,
    pub count: i64,
    pub upcoming: i64,
    pub live: i64,
    pub completed: i64,
}

// ========== FOR BULK OPERATIONS ==========
#[derive(Debug, Deserialize)]
pub struct BulkGameUpdate {
    pub games: Vec<UpdateGameScore>,
}

#[derive(Debug, Deserialize)]
pub struct GameStatusUpdate {
    pub match_id: String,
    pub status: String,
    pub is_live: bool,
}

// ========== FOR TIMELINE REQUESTS ==========
#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub match_id: String,
    pub limit: Option<i64>,
    pub event_type: Option<String>,
}

// ========== FOR STATISTICS REQUESTS ==========
#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    pub match_id: String,
    pub minute: Option<i32>,
}

// ========== HELPER IMPLEMENTATIONS ==========
impl Game {
    pub fn is_upcoming(&self) -> bool {
        self.status == "upcoming"
    }

    pub fn is_live_game(&self) -> bool {
        self.status == "live" || self.is_live
    }

    pub fn is_completed(&self) -> bool {
        self.status == "completed"
    }

    pub fn formatted_score(&self) -> String {
        match (self.home_score, self.away_score) {
            (Some(home), Some(away)) => format!("{} - {}", home, away),
            _ => "VS".to_string(),
        }
    }

    pub fn display_date(&self) -> String {
        if !self.date_iso.is_empty() {
            self.date_iso.clone()
        } else {
            format!("{} {}", self.date, self.time)
        }
    }

    pub fn display_datetime(&self) -> String {
        format!("{} at {}", self.date, self.time)
    }

    // ========== HELPER METHODS ==========

    /// Get total vote count
    pub fn total_votes(&self) -> i64 {
        self.votes
    }

    /// Get total comment count
    pub fn total_comments(&self) -> i64 {
        self.comments
    }

    /// Check if a specific user has voted
    pub fn has_user_voted(&self, user_id: &str) -> bool {
        self.voters.iter().any(|v| v.user_id == user_id)
    }

    /// Get user's vote selection
    pub fn get_user_vote(&self, user_id: &str) -> Option<String> {
        self.voters
            .iter()
            .find(|v| v.user_id == user_id)
            .map(|v| v.selection.clone())
    }

    /// Get vote counts by selection
    pub fn get_vote_breakdown(&self) -> (i64, i64, i64) {
        let home = self
            .voters
            .iter()
            .filter(|v| v.selection == "home_team")
            .count() as i64;
        let draw = self.voters.iter().filter(|v| v.selection == "draw").count() as i64;
        let away = self
            .voters
            .iter()
            .filter(|v| v.selection == "away_team")
            .count() as i64;
        (home, draw, away)
    }

    /// Get recent voters (last N)
    pub fn get_recent_voters(&self, limit: usize) -> Vec<&Voter> {
        let mut sorted = self.voters.iter().collect::<Vec<_>>();
        sorted.sort_by(|a, b| b.voted_at.cmp(&a.voted_at));
        sorted.into_iter().take(limit).collect()
    }

    /// Get vote percentages
    pub fn get_vote_percentages(&self) -> (f64, f64, f64) {
        let total = self.votes as f64;
        if total == 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let (home, draw, away) = self.get_vote_breakdown();
        (
            (home as f64 / total) * 100.0,
            (draw as f64 / total) * 100.0,
            (away as f64 / total) * 100.0,
        )
    }

    /// Check if match is currently happening
    pub fn is_happening_now(&self) -> bool {
        self.is_live_game() && self.time_elapsed > 0 && self.time_elapsed < 95
    }

    /// Get match minute display (e.g., "67'")
    pub fn get_minute_display(&self) -> String {
        if self.time_elapsed > 0 && self.is_live_game() {
            format!("{}'", self.time_elapsed)
        } else if self.is_live_game() {
            "LIVE".to_string()
        } else {
            "".to_string()
        }
    }
}
