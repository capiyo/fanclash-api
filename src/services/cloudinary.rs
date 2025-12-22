use reqwest::multipart;
use serde_json::Value;
use std::env;
use crate::errors::{AppError, Result};

#[derive(Clone)]
pub struct CloudinaryService {
    cloud_name: String,
    api_key: String,
    api_secret: String,
    upload_preset: String,
}

impl CloudinaryService {
    pub fn new() -> Result<Self> {
        let cloud_name = env::var("CLOUDINARY_CLOUD_NAME")
            .map_err(|_| AppError::CloudinaryError("CLOUDINARY_CLOUD_NAME not set".into()))?;
        
        let api_key = env::var("CLOUDINARY_API_KEY")
            .map_err(|_| AppError::CloudinaryError("CLOUDINARY_API_KEY not set".into()))?;
        
        let api_secret = env::var("CLOUDINARY_API_SECRET")
            .map_err(|_| AppError::CloudinaryError("CLOUDINARY_API_SECRET not set".into()))?;

        let upload_preset = env::var("CLOUDINARY_UPLOAD_PRESET")
            .unwrap_or_else(|_| "ml_default".to_string());

        Ok(Self {
            cloud_name,
            api_key,
            api_secret,
            upload_preset,
        })
    }

    /// Upload image to Cloudinary using signed upload
    pub async fn upload_image(
        &self,
        image_data: &[u8],
        folder: &str,
        public_id: Option<&str>,
    ) -> Result<(String, String)> {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        
        // Generate signature
        let signature_data = format!(
            "folder={}&timestamp={}{}",
            folder, timestamp, self.api_secret
        );
        let signature = format!("{:x}", md5::compute(signature_data));

        let upload_url = format!(
            "https://api.cloudinary.com/v1_1/{}/image/upload",
            self.cloud_name
        );

        let client = reqwest::Client::new();
        
        // Build multipart form
        let mut form = multipart::Form::new()
            .text("api_key", self.api_key.clone())
            .text("timestamp", timestamp.clone())
            .text("signature", signature)
            .text("folder", folder.to_string())
            .part(
                "file",
                multipart::Part::bytes(image_data.to_vec())
                    .file_name("image.jpg")
                    .mime_str("image/jpeg")
                    .map_err(|e| AppError::CloudinaryError(e.to_string()))?
            );

        // Add public_id if provided
        if let Some(pid) = public_id {
            form = form.text("public_id", pid.to_string());
        }

        // Send request
        let response = client
            .post(&upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::CloudinaryError(format!("Upload failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AppError::CloudinaryError(format!("Cloudinary API error: {}", error_text)));
        }

        let result: Value = response.json()
            .await
            .map_err(|e| AppError::CloudinaryError(format!("Failed to parse response: {}", e)))?;

        // Check for Cloudinary error
        if let Some(error) = result.get("error") {
            let error_msg = error["message"]
                .as_str()
                .unwrap_or("Unknown Cloudinary error");
            return Err(AppError::CloudinaryError(error_msg.to_string()));
        }

        let secure_url = result["secure_url"]
            .as_str()
            .ok_or_else(|| AppError::CloudinaryError("No secure URL in response".to_string()))?
            .to_string();

        let public_id = result["public_id"]
            .as_str()
            .ok_or_else(|| AppError::CloudinaryError("No public ID in response".to_string()))?
            .to_string();

        Ok((secure_url, public_id))
    }

    /// Upload image using upload preset (unsigned - simpler)
    pub async fn upload_image_with_preset(
        &self,
        image_data: &[u8],
        folder: &str,
        public_id: Option<&str>,
    ) -> Result<(String, String)> {
        let upload_url = format!(
            "https://api.cloudinary.com/v1_1/{}/image/upload",
            self.cloud_name
        );

        let client = reqwest::Client::new();
        
        // Build multipart form
        let mut form = multipart::Form::new()
            .text("upload_preset", self.upload_preset.clone())
            .text("folder", folder.to_string())
            .part(
                "file",
                multipart::Part::bytes(image_data.to_vec())
                    .file_name("image.jpg")
                    .mime_str("image/jpeg")
                    .map_err(|e| AppError::CloudinaryError(e.to_string()))?
            );

        // Add public_id if provided
        if let Some(pid) = public_id {
            form = form.text("public_id", pid.to_string());
        }

        // Send request
        let response = client
            .post(&upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::CloudinaryError(format!("Upload failed: {}", e)))?;

        let result: Value = response.json()
            .await
            .map_err(|e| AppError::CloudinaryError(format!("Failed to parse response: {}", e)))?;

        // Check for Cloudinary error
        if let Some(error) = result.get("error") {
            let error_msg = error["message"]
                .as_str()
                .unwrap_or("Unknown Cloudinary error");
            return Err(AppError::CloudinaryError(error_msg.to_string()));
        }

        let secure_url = result["secure_url"]
            .as_str()
            .ok_or_else(|| AppError::CloudinaryError("No secure URL in response".to_string()))?
            .to_string();

        let public_id = result["public_id"]
            .as_str()
            .ok_or_else(|| AppError::CloudinaryError("No public ID in response".to_string()))?
            .to_string();

        Ok((secure_url, public_id))
    }

    /// Delete image from Cloudinary
    pub async fn delete_image(&self, public_id: &str) -> Result<()> {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        
        // Generate signature for delete
        let signature_data = format!(
            "public_id={}&timestamp={}{}",
            public_id, timestamp, self.api_secret
        );
        let signature = format!("{:x}", md5::compute(signature_data));

        let delete_url = format!(
            "https://api.cloudinary.com/v1_1/{}/image/destroy",
            self.cloud_name
        );

        let params = [
            ("public_id", public_id),
            ("api_key", &self.api_key),
            ("timestamp", &timestamp),
            ("signature", &signature),
        ];

        let client = reqwest::Client::new();
        let response = client
            .post(&delete_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AppError::CloudinaryError(format!("Delete failed: {}", e)))?;

        let result: Value = response.json()
            .await
            .map_err(|e| AppError::CloudinaryError(format!("Failed to parse response: {}", e)))?;

        if result["result"] != "ok" {
            return Err(AppError::CloudinaryError(
                format!("Failed to delete image: {}", result["result"])
            ));
        }

        Ok(())
    }

    /// Generate URL with transformations
    pub fn generate_transformed_url(
        &self,
        public_id: &str,
        transformations: &str,
    ) -> String {
        format!(
            "https://res.cloudinary.com/{}/image/upload/{}/{}",
            self.cloud_name, transformations, public_id
        )
    }

    /// Generate thumbnail URL
    pub fn generate_thumbnail_url(&self, public_id: &str, width: u32, height: u32) -> String {
        let transformations = format!("c_fill,w_{},h_{},q_auto", width, height);
        self.generate_transformed_url(public_id, &transformations)
    }
}