// src/services/fcm_service.rs

use reqwest::Client;
use serde_json::{json, Value};
use mongodb::{Collection, bson::{doc, DateTime as BsonDateTime}};
use std::path::Path;
use yup_oauth2::{read_service_account_key, ServiceAccountAuthenticator};
use yup_oauth2::authenticator::Authenticator;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::Mutex;
use futures_util::TryStreamExt;

use crate::{
    errors::AppError,
    models::notification::{FCMToken, Notification},
    state::AppState,
};

const FIREBASE_PROJECT_ID: &str = "clash-66865";
const SERVICE_ACCOUNT_PATH: &str = "./firebase-service-account.json";

// Use the Authenticator type with the connector type
type HyperConnector = yup_oauth2::hyper_rustls::HttpsConnector<hyper::client::HttpConnector>;
type AuthType = Authenticator<HyperConnector>;

pub struct FCMService {
    authenticator: Arc<Mutex<AuthType>>,
    client: Client,
}

impl FCMService {
    pub async fn new() -> anyhow::Result<Self> {
        // Load the service account key
        let service_account_key = read_service_account_key(Path::new(SERVICE_ACCOUNT_PATH))
            .await
            .map_err(|e| anyhow!("Failed to read service account key: {}", e))?;

        // Build the authenticator - this returns Authenticator
        let authenticator = ServiceAccountAuthenticator::builder(service_account_key)
            .build()
            .await
            .map_err(|e| anyhow!("Failed to build authenticator: {}", e))?;

        Ok(Self {
            authenticator: Arc::new(Mutex::new(authenticator)),
            client: Client::new(),
        })
    }

    // Get access token method - FIXED using the correct approach
    async fn get_access_token(&self) -> anyhow::Result<String> {
        let mut auth = self.authenticator.lock().await;

        let token = auth
            .token(&["https://www.googleapis.com/auth/firebase.messaging"])
            .await
            .map_err(|e| anyhow!("Failed to get token: {}", e))?;

        token.token()
            .map(|t| t.to_string())
            .ok_or_else(|| anyhow!("Access token was empty"))
    }
    // Send notification to a specific user
    pub async fn send_to_user(
        &self,
        state: &AppState,
        user_id: &str,
        title: &str,
        body: &str,
        data: Value,
        notification_type: &str,
    ) -> Result<bool, AppError> {
        // Get user's FCM tokens
        let tokens_collection: Collection<FCMToken> = state.db.collection("fcm_tokens");
        let filter = doc! { "user_id": user_id };

        let mut cursor = tokens_collection.find(filter).await
            .map_err(|e| AppError::InternalServerError(format!("Database error: {}", e)))?;

        let mut success = false;

        while let Some(token_doc) = cursor.try_next().await
            .map_err(|e| AppError::InternalServerError(format!("Database error: {}", e)))? {

            if self.send_to_device(&token_doc.fcm_token, title, body, data.clone(), notification_type).await {
                success = true;
            }
        }

        self.save_notification(state, user_id, notification_type, title, body, data).await?;
        Ok(success)
    }

    async fn send_to_device(
        &self,
        token: &str,
        title: &str,
        body: &str,
        data: Value,
        notification_type: &str,
    ) -> bool {
        let access_token = match self.get_access_token().await {
            Ok(token) => token,
            Err(e) => {
                eprintln!("❌ Failed to get access token: {}", e);
                return false;
            }
        };

        let message = json!({
            "message": {
                "token": token,
                "notification": {
                    "title": title,
                    "body": body,
                },
                "data": {
                    "type": notification_type,
                    "click_action": "FLUTTER_NOTIFICATION_CLICK",
                    "data": serde_json::to_string(&data).unwrap_or_default(),
                },
                "android": {
                    "priority": "high",
                    "notification": {
                        "click_action": "FLUTTER_NOTIFICATION_CLICK",
                        "channel_id": "vote_notifications",
                        "sound": "default",
                    }
                },
                "apns": {
                    "headers": {
                        "apns-priority": "10"
                    },
                    "payload": {
                        "aps": {
                            "sound": "default",
                            "badge": 1,
                            "category": "VOTE_CATEGORY"
                        }
                    }
                }
            }
        });

        match self.client
            .post(format!("https://fcm.googleapis.com/v1/projects/{}/messages:send", FIREBASE_PROJECT_ID))
            .bearer_auth(access_token)
            .json(&message)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    println!("✅ FCM v1 notification sent successfully");
                    true
                } else {
                    eprintln!("❌ FCM v1 error: {}", response.status());
                    if let Ok(error_text) = response.text().await {
                        eprintln!("Error details: {}", error_text);
                    }
                    false
                }
            }
            Err(e) => {
                eprintln!("❌ FCM v1 request failed: {}", e);
                false
            }
        }
    }

    pub async fn send_to_multiple_users(
        &self,
        state: &AppState,
        user_ids: Vec<String>,
        title: &str,
        body: &str,
        data: Value,
        notification_type: &str,
    ) -> Result<usize, AppError> {
        let mut success_count = 0;
        for user_id in user_ids {
            if self.send_to_user(state, &user_id, title, body, data.clone(), notification_type).await? {
                success_count += 1;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        Ok(success_count)
    }

    async fn save_notification(
        &self,
        state: &AppState,
        user_id: &str,
        notification_type: &str,
        title: &str,
        body: &str,
        data: Value,
    ) -> Result<(), AppError> {
        let collection: Collection<Notification> = state.db.collection("notifications");
        let notification = Notification {
            id: None,
            user_id: user_id.to_string(),
            notification_type: notification_type.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            data,
            is_read: false,
            created_at: BsonDateTime::now(),
        };
        collection.insert_one(notification).await
            .map_err(|e| AppError::InternalServerError(format!("Database error: {}", e)))?;
        Ok(())
    }
}

// Initialize once at app startup
pub async fn init_fcm_service() -> anyhow::Result<Arc<FCMService>> {
    let service = FCMService::new().await?;
    Ok(Arc::new(service))
}
