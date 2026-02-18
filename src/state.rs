use mongodb::Database;
use std::sync::Arc;

use crate::services::cloudinary::CloudinaryService;
use crate::services::mpesa_service::MpesaService;
use crate::services::fcm_service::FCMService;  // ADD THIS IMPORT

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub mpesa_service: Option<Arc<MpesaService>>,
    pub fcm_service: Option<Arc<FCMService>>,  // ADD THIS FIELD
    pub cloudinary: CloudinaryService,
}

impl AppState {
    pub fn new(db: Database) -> Result<Self, crate::errors::AppError> {
        let cloudinary = CloudinaryService::new()?;

        Ok(AppState {
            db,
            mpesa_service: None,
            fcm_service: None,  // ADD THIS (initialized as None)
            cloudinary,
        })
    }

    pub fn with_mpesa(mut self, mpesa_service: Arc<MpesaService>) -> Self {
        self.mpesa_service = Some(mpesa_service);
        self
    }

    // ADD THIS METHOD for FCM
    pub fn with_fcm(mut self, fcm_service: Arc<FCMService>) -> Self {
        self.fcm_service = Some(fcm_service);
        self
    }
}
