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

// ========== MAIN GAME MODEL ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    #[serde(rename = "_id")]
    pub id: String,

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
    pub status: String,

    #[serde(rename = "is_live")]
    pub is_live: bool,

    #[serde(rename = "available_for_voting")]
    pub available_for_voting: bool,

    #[serde(rename = "source")]
    pub source: String,

    #[serde(rename = "scraped_at")]
    pub scraped_at: BsonDateTime,

    #[serde(rename = "date_iso")]
    pub date_iso: String,

    // ========== NEW COUNTER FIELDS ==========
    #[serde(rename = "votes", default)]
    pub votes: i64,

    #[serde(rename = "comments", default)]
    pub comments: i64,

    // ========== NEW VOTERS ARRAY ==========
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
}

// ========== FOR LIVE GAME UPDATES ==========
#[derive(Debug, Deserialize)]
pub struct LiveUpdate {
    pub match_id: String,
    pub home_score: Option<i32>,
    pub away_score: Option<i32>,
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

    // ========== NEW HELPER METHODS ==========

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
}
