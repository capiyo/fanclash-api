use serde::{Deserialize, Serialize};
use bson::{oid::ObjectId, DateTime as BsonDateTime}; // Rename import for clarity
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub _id: Option<ObjectId>,
    pub home_team: String,
    pub away_team: String,
    pub league: String,
    pub home_win: f64,
    pub away_win: f64,
    pub draw: f64,
    pub date: String,
    pub status: String,

    // CHANGE: Use BSON's native DateTime type directly
    pub created_at: BsonDateTime,

    // CHANGE: Use BSON's native DateTime type directly
    pub updated_at: BsonDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateGame {
    pub home_team: String,
    pub away_team: String,
    pub league: String,
    pub home_win: f64,
    pub away_win: f64,
    pub draw: f64,
    pub date: String,
}