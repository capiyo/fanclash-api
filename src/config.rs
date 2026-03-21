use crate::errors::{AppError, Result};
use reqwest::multipart;
use serde_json::Value;
use std::env;

#[derive(Clone)]
pub struct CloudinaryService {
    cloud_name: String,
    api_key: String,
    api_secret: String,
    upload_preset: String,
}

impl CloudinaryService {
    /// Infallible constructor — reads env vars but never panics or returns Err.
    /// If vars are missing, uploads will fail at call time with a clear error.
    /// This prevents the server from crashing at startup.
    pub fn new() -> Result<Self> {
        let cloud_name = env::var("CLOUDINARY_CLOUD_NAME").unwrap_or_else(|_| {
            tracing::warn!("⚠️  CLOUDINARY_CLOUD_NAME not set — image uploads will fail");
            String::new()
        });

        let api_key = env::var("CLOUDINARY_API_KEY").unwrap_or_else(|_| {
            tracing::warn!("⚠️  CLOUDINARY_API_KEY not set — image uploads will fail");
            String::new()
        });

        let api_secret = env::var("CLOUDINARY_API_SECRET").unwrap_or_else(|_| {
            tracing::warn!("⚠️  CLOUDINARY_API_SECRET not set — image uploads will fail");
            String::new()
        });

        let upload_preset = env::var("CLOUDINARY_UPLOAD_PRESET")
            .unwrap_or_else(|_| "rust_backend_upload".to_string());

        if !cloud_name.is_empty() {
            tracing::info!("✅ Cloudinary configured for cloud: {}", cloud_name);
        }

        Ok(Self {
            cloud_name,
            api_key,
            api_secret,
            upload_preset,
        })
    }

    /// Returns true if Cloudinary is properly configured.
    pub fn is_configured(&self) -> bool {
        !self.cloud_name.is_empty() && !self.api_key.is_empty() && !self.api_secret.is_empty()
    }

    /// Upload image using upload preset (unsigned)
    pub async fn upload_image_with_preset(
        &self,
        image_data: &[u8],
        folder: &str,
        public_id: Option<&str>,
    ) -> Result<(String, String)> {
        if !self.is_configured() {
            return Err(AppError::CloudinaryError(
                "Cloudinary is not configured. Set CLOUDINARY_CLOUD_NAME, CLOUDINARY_API_KEY, CLOUDINARY_API_SECRET env vars.".into(),
            ));
        }

        tracing::info!(
            "📤 Cloudinary upload — folder: {}, size: {} bytes",
            folder,
            image_data.len()
        );

        let upload_url = format!(
            "https://api.cloudinary.com/v1_1/{}/image/upload",
            self.cloud_name
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| {
                AppError::CloudinaryError(format!("Failed to create HTTP client: {}", e))
            })?;

        let mut form = multipart::Form::new()
            .text("upload_preset", self.upload_preset.clone())
            .text("folder", folder.to_string());

        if let Some(pid) = public_id {
            form = form.text("public_id", pid.to_string());
        }

        let mime_type = infer::get(image_data)
            .map(|info| info.mime_type())
            .unwrap_or("image/jpeg");

        form = form.part(
            "file",
            multipart::Part::bytes(image_data.to_vec())
                .file_name("upload.jpg")
                .mime_str(mime_type)
                .map_err(|e| {
                    AppError::CloudinaryError(format!("Failed to set MIME type: {}", e))
                })?,
        );

        let response = client
            .post(&upload_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::CloudinaryError(format!("Network error: {}", e)))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .map_err(|e| AppError::CloudinaryError(format!("Failed to read response: {}", e)))?;

        tracing::debug!("Cloudinary response {}: {}", status, response_text);

        let result: Value = serde_json::from_str(&response_text).map_err(|e| {
            AppError::CloudinaryError(format!(
                "Failed to parse response: {} — body: {}",
                e, response_text
            ))
        })?;

        if let Some(error) = result.get("error") {
            let msg = error["message"]
                .as_str()
                .unwrap_or("Unknown Cloudinary error");
            return Err(AppError::CloudinaryError(format!(
                "Cloudinary error: {}",
                msg
            )));
        }

        let secure_url = result["secure_url"]
            .as_str()
            .ok_or_else(|| AppError::CloudinaryError("No secure_url in response".into()))?
            .to_string();

        let public_id_out = result["public_id"]
            .as_str()
            .ok_or_else(|| AppError::CloudinaryError("No public_id in response".into()))?
            .to_string();

        tracing::info!("✅ Cloudinary upload successful: {}", secure_url);
        Ok((secure_url, public_id_out))
    }

    /// Fallback: signed upload
    pub async fn upload_image_signed(
        &self,
        image_data: &[u8],
        folder: &str,
        public_id: Option<&str>,
    ) -> Result<(String, String)> {
        if !self.is_configured() {
            return Err(AppError::CloudinaryError(
                "Cloudinary is not configured.".into(),
            ));
        }

        let timestamp = chrono::Utc::now().timestamp().to_string();

        let mut params_to_sign = vec![
            format!("folder={}", folder),
            format!("timestamp={}", timestamp),
        ];

        if let Some(pid) = public_id {
            params_to_sign.push(format!("public_id={}", pid));
        }

        params_to_sign.sort();
        let params_string = params_to_sign.join("&");
        let signature_string = format!("{}{}", params_string, self.api_secret);
        let signature = format!("{:x}", md5::compute(signature_string));

        let upload_url = format!(
            "https://api.cloudinary.com/v1_1/{}/image/upload",
            self.cloud_name
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let mut form = multipart::Form::new()
            .text("api_key", self.api_key.clone())
            .text("timestamp", timestamp)
            .text("signature", signature)
            .text("folder", folder.to_string());

        if let Some(pid) = public_id {
            form = form.text("public_id", pid.to_string());
        }

        form = form.part(
            "file",
            multipart::Part::bytes(image_data.to_vec())
                .file_name("image.jpg")
                .mime_str("image/jpeg")?,
        );

        let response = client.post(&upload_url).multipart(form).send().await?;
        let response_text = response.text().await?;
        let result: Value = serde_json::from_str(&response_text)?;

        if let Some(error) = result.get("error") {
            let msg = error["message"].as_str().unwrap_or("Unknown error");
            return Err(AppError::CloudinaryError(format!(
                "Signed upload failed: {}",
                msg
            )));
        }

        let secure_url = result["secure_url"]
            .as_str()
            .ok_or_else(|| AppError::CloudinaryError("No secure URL".into()))?
            .to_string();

        let public_id_out = result["public_id"]
            .as_str()
            .ok_or_else(|| AppError::CloudinaryError("No public ID".into()))?
            .to_string();

        Ok((secure_url, public_id_out))
    }

    pub async fn delete_image(&self, public_id: &str) -> Result<()> {
        if !self.is_configured() {
            return Ok(()); // Silently skip if not configured
        }

        let timestamp = chrono::Utc::now().timestamp().to_string();
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
        let response = client.post(&delete_url).form(&params).send().await?;
        let result: Value = response.json().await?;

        if result["result"] != "ok" {
            return Err(AppError::CloudinaryError(format!(
                "Failed to delete image: {}",
                result["result"]
            )));
        }

        Ok(())
    }

    pub fn generate_transformed_url(&self, public_id: &str, transformations: &str) -> String {
        format!(
            "https://res.cloudinary.com/{}/image/upload/{}/{}",
            self.cloud_name, transformations, public_id
        )
    }

    pub fn generate_thumbnail_url(&self, public_id: &str, width: u32, height: u32) -> String {
        let transformations = format!("c_fill,w_{},h_{},q_auto", width, height);
        self.generate_transformed_url(public_id, &transformations)
    }
}
