use reqwest::Client;
use serde_json::{json, Value};
use mongodb::{Collection, bson::{doc, DateTime as BsonDateTime}};
use yup_oauth2::ServiceAccountKey;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::Mutex;
use futures_util::TryStreamExt;
use std::env;

use crate::{
    errors::AppError,
    models::notification::{FCMToken, Notification},
    state::AppState,
};

const FIREBASE_PROJECT_ID: &str = "clash-66865";
// REMOVE THIS: const SERVICE_ACCOUNT_PATH: &str = "./firebase-service-account.json";

pub struct FCMService {
    authenticator: Arc<Mutex<yup_oauth2::authenticator::Authenticator<
        yup_oauth2::hyper_rustls::HttpsConnector<hyper::client::HttpConnector>
    >>>,
    client: Client,
}

impl FCMService {
    pub async fn new() -> anyhow::Result<Self> {
        // READ FROM ENVIRONMENT VARIABLES INSTEAD OF FILE
        println!("📖 Reading Firebase credentials from environment variables...");

        // Get credentials from .env
        let client_email = env::var("FIREBASE_CLIENT_EMAIL")
            .map_err(|_| anyhow!("FIREBASE_CLIENT_EMAIL not set in environment"))?;

        let private_key = env::var("FIREBASE_PRIVATE_KEY")
            .map_err(|_| anyhow!("FIREBASE_PRIVATE_KEY not set in environment"))?;

        let project_id = env::var("FIREBASE_PROJECT_ID")
            .unwrap_or_else(|_| "clash-66865".to_string());

        // Create service account key from environment variables
        let service_account_key = ServiceAccountKey {
            project_id: Some(project_id),
            client_email: Some(client_email),
            private_key: Some(private_key),
            private_key_id: None,
            client_id: None,
            auth_uri: None,
            token_uri: None,
            auth_provider_x509_cert_url: None,
            client_x509_cert_url: None,
            universe_domain: None,
            r#type: None,
        };

        println!("✅ Service account key created from environment variables");

        // Build the authenticator
        let authenticator = yup_oauth2::ServiceAccountAuthenticator::builder(service_account_key)
            .build()
            .await
            .map_err(|e| anyhow!("Failed to build authenticator: {}", e))?;

        println!("✅ Authenticator built successfully");

        Ok(Self {
            authenticator: Arc::new(Mutex::new(authenticator)),
            client: Client::new(),
        })
    }

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

    // Rest of your methods remain the same...
    pub async fn send_to_user(
        &self,
        state: &AppState,
        user_id: &str,
        title: &str,
        body: &str,
        data: Value,
        notification_type: &str,
    ) -> Result<bool, AppError> {
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

pub async fn init_fcm_service() -> anyhow::Result<Arc<FCMService>> {
    println!("🚀 Initializing FCM Service from environment variables...");
    let service = FCMService::new().await?;
    println!("✅ FCM Service initialized successfully!");
    Ok(Arc::new(service))
}
