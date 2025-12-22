use mongodb::Database;
use std::sync::Arc;

use crate::services::cloudinary::CloudinaryService;
use crate::services::mpesa_service::MpesaService;

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub mpesa_service: Option<Arc<MpesaService>>,
    pub cloudinary: CloudinaryService,
}

impl AppState {
    pub fn new(db: Database) -> Result<Self, crate::errors::AppError> {
        let cloudinary = CloudinaryService::new()?;

        Ok(AppState {
            db,
            mpesa_service: None,
            cloudinary,
        })
    }

    pub fn with_mpesa(mut self, mpesa_service: Arc<MpesaService>) -> Self {
        self.mpesa_service = Some(mpesa_service);
        self
    }
}
